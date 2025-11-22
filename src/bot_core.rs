// bot_core.rs - Core bot logic extracted from main.rs for GUI integration

use anyhow::{anyhow, Result};
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::{
    compute_budget::ComputeBudgetInstruction,
    message::{v0, VersionedMessage},
    pubkey::Pubkey,
    signature::{Keypair, Signer},
    transaction::VersionedTransaction,
    system_instruction,
};
use spl_associated_token_account::{get_associated_token_address, instruction::create_associated_token_account};
use std::str::FromStr;
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message as WsMessage};
use futures_util::{StreamExt, SinkExt};
use rand::seq::SliceRandom;
use chrono::Utc;
use std::time::Duration;
use std::sync::Arc;
use tokio::sync::mpsc;

use crate::detection::PumpBuyAccounts;
use crate::websocket::{is_initialize_bonding_curve, extract_signature};
use crate::buy::build_buy_instruction;
use crate::jito::send_jito_bundle;
use crate::helius::send_helius_transaction;
use crate::socials::{check_token_socials, Socials};
use crate::das_check::check_creator_token_count_das;
use crate::accounts::{TokenBuy, TokenTracker, SeenTokens, fetch_bonding_curve_mc, BondingCurveAccount};
use crate::config::Config;
use crate::constants::PUMP_PROGRAM_ID;
use crate::metrics::{SharedMetrics, FilterReason, SubmissionMethod, ErrorType};
use crate::gui::{TokenEvent, BotControl};

pub async fn run_bot(
    config: Arc<std::sync::RwLock<Config>>,
    wallet: Keypair,
    metrics: SharedMetrics,
    tracker: Arc<std::sync::RwLock<Option<TokenTracker>>>,
    seen_tokens: Arc<SeenTokens>,
    health_monitor: Arc<std::sync::Mutex<crate::health::HealthMonitor>>,
    das_rate_limiter: Arc<crate::rate_limiter::RateLimiter>,
    socials_rate_limiter: Arc<crate::rate_limiter::RateLimiter>,
    event_tx: mpsc::UnboundedSender<TokenEvent>,
    mut control_rx: mpsc::UnboundedReceiver<BotControl>,
    wallet_balance: Arc<std::sync::RwLock<f64>>,
) -> Result<()> {
    // Load initial config
    let initial_config = {
        let cfg = config.read().unwrap();
        (*cfg).clone()
    };
    
    // Create RPC client
    let rpc = initial_config.create_rpc_client();
    
    // Update wallet balance
    if let Ok(balance) = rpc.get_balance(&wallet.pubkey()).await {
        if let Ok(mut bal) = wallet_balance.write() {
            *bal = balance as f64 / 1e9;
        }
    }
    
    // Pre-load global account
    crate::buy::preload_global(&rpc, &initial_config.global_account).await?;
    
    let mut detected = 0;
    let mut reconnect_count = 0;
    
    // Main bot loop
    loop {
        // Check for control messages (non-blocking)
        if let Ok(control) = control_rx.try_recv() {
            match control {
                BotControl::Stop => {
                    let _ = event_tx.send(TokenEvent::Info {
                        message: "Bot stopped by user".to_string(),
                        timestamp: Utc::now(),
                    });
                    break;
                }
                BotControl::UpdateConfig(new_config) => {
                    // Update config, will be used in next iteration
                    if let Ok(mut cfg) = config.write() {
                        *cfg = new_config;
                    }
                }
                BotControl::Restart => {
                    // Restart by breaking and restarting the loop
                    reconnect_count = 0;
                }
                BotControl::Start => {
                    // Already running, ignore
                }
            }
        }
        
        // Get current config
        let current_config = {
            let cfg = config.read().unwrap();
            (*cfg).clone()
        };
        
        reconnect_count += 1;
        
        let reconnect_delay = if reconnect_count == 1 {
            Duration::from_secs(0)
        } else if reconnect_count < 5 {
            Duration::from_secs(5)
        } else {
            Duration::from_secs(15)
        };
        
        if reconnect_delay.as_secs() > 0 {
            let _ = event_tx.send(TokenEvent::Info {
                message: format!("Reconnecting in {}s...", reconnect_delay.as_secs()),
                timestamp: Utc::now(),
            });
            tokio::time::sleep(reconnect_delay).await;
        }
        
        // Connect to WebSocket
        match listen_websocket_once(
            &current_config,
            &wallet,
            &rpc,
            &tracker,
            &mut detected,
            seen_tokens.clone(),
            metrics.clone(),
            health_monitor.clone(),
            das_rate_limiter.clone(),
            socials_rate_limiter.clone(),
            event_tx.clone(),
            &mut control_rx,
        ).await {
            Ok(stopped) => {
                if stopped {
                    // Bot was stopped by user
                    return Ok(());
                }
                reconnect_count = 0;
            }
            Err(e) => {
                let _ = event_tx.send(TokenEvent::Error {
                    message: format!("WebSocket error: {}", e),
                    timestamp: Utc::now(),
                });
            }
        }
        
        if reconnect_count > 10 {
            tokio::time::sleep(Duration::from_secs(60)).await;
            reconnect_count = 0;
        }
    }
    
    Ok(())
}

async fn listen_websocket_once(
    config: &Config, // Note: This is a snapshot, not a live reference
    wallet: &Keypair,
    rpc: &RpcClient,
    tracker: &Arc<std::sync::RwLock<Option<TokenTracker>>>,
    detected: &mut u32,
    seen_tokens: Arc<SeenTokens>,
    metrics: SharedMetrics,
    health_monitor: Arc<std::sync::Mutex<crate::health::HealthMonitor>>,
    das_rate_limiter: Arc<crate::rate_limiter::RateLimiter>,
    socials_rate_limiter: Arc<crate::rate_limiter::RateLimiter>,
    event_tx: mpsc::UnboundedSender<TokenEvent>,
    control_rx: &mut mpsc::UnboundedReceiver<BotControl>,
) -> Result<bool> {
    // Returns Ok(true) if stopped, Ok(false) if normal exit
    let pump_program = Pubkey::from_str(PUMP_PROGRAM_ID)?;
    
    let subscribe_msg = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "logsSubscribe",
        "params": [
            {
                "mentions": [pump_program.to_string()]
            },
            {
                "commitment": "processed"
            }
        ]
    });
    
    let _ = event_tx.send(TokenEvent::Info {
        message: format!("Connecting to WebSocket: {}", &config.wss_url),
        timestamp: Utc::now(),
    });
    
    let (ws_stream, _) = connect_async(&config.wss_url).await.map_err(|e| {
        anyhow!("WebSocket connection failed: {}", e)
    })?;
    
    // Update health monitor
    {
        let mut monitor = health_monitor.lock().unwrap();
        monitor.check_websocket(true);
    }
    
    let _ = event_tx.send(TokenEvent::Info {
        message: "WebSocket connected successfully".to_string(),
        timestamp: Utc::now(),
    });
    
    let (mut write, mut read) = ws_stream.split();
    
    let _ = event_tx.send(TokenEvent::Info {
        message: "Subscribing to pump.fun program logs...".to_string(),
        timestamp: Utc::now(),
    });
    
    write.send(WsMessage::Text(subscribe_msg.to_string())).await.map_err(|e| {
        anyhow!("Failed to send subscription: {}", e)
    })?;
    
    let _ = event_tx.send(TokenEvent::Info {
        message: "Subscription sent, listening for new tokens...".to_string(),
        timestamp: Utc::now(),
    });
    
    let mut message_count = 0;
    loop {
        // Check for Stop signal before waiting for WebSocket message
        if let Ok(control) = control_rx.try_recv() {
            if matches!(control, BotControl::Stop) {
                let _ = event_tx.send(TokenEvent::Info {
                    message: "Bot stopped by user (in WebSocket loop)".to_string(),
                    timestamp: Utc::now(),
                });
                return Ok(true); // Signal that we stopped
            }
        }
        
        // Use tokio::select to check both WebSocket and control channel
        tokio::select! {
            // Check for control messages
            control_result = control_rx.recv() => {
                if let Some(control) = control_result {
                    if matches!(control, BotControl::Stop) {
                        let _ = event_tx.send(TokenEvent::Info {
                            message: "Bot stopped by user (in WebSocket loop)".to_string(),
                            timestamp: Utc::now(),
                        });
                        return Ok(true); // Signal that we stopped
                    }
                } else {
                    // Channel closed
                    return Ok(false);
                }
            }
            // Check for WebSocket messages
            msg_result = read.next() => {
                match msg_result {
                    Some(msg) => {
                        let msg = msg?;
        // Log every 100 messages to show activity
        message_count += 1;
        if message_count % 100 == 0 {
            let _ = event_tx.send(TokenEvent::Info {
                message: format!("Received {} WebSocket messages (still listening)...", message_count),
                timestamp: Utc::now(),
            });
        }
        
        if let WsMessage::Text(text) = msg {
            // Debug: Log first few messages to see what we're receiving
            if message_count <= 3 {
                let preview = if text.len() > 200 {
                    format!("{}...", &text[..200])
                } else {
                    text.clone()
                };
                let _ = event_tx.send(TokenEvent::Info {
                    message: format!("WebSocket message #{}: {}", message_count, preview),
                    timestamp: Utc::now(),
                });
            }
            
            // Try to parse notification
            let notification: serde_json::Value = match serde_json::from_str(&text) {
                Ok(v) => v,
                Err(e) => {
                    // Log parse errors occasionally
                    if message_count % 50 == 0 {
                        let _ = event_tx.send(TokenEvent::Info {
                            message: format!("Non-JSON message received (count: {}) - {}", message_count, e),
                            timestamp: Utc::now(),
                        });
                    }
                    continue;
                }
            };
            
            // Check if this is a subscription confirmation
            if let Some(method) = notification["method"].as_str() {
                if method == "logsNotification" || method.contains("Notification") {
                    // This is a log notification - good!
                } else if method == "subscription" || notification["id"].as_u64().is_some() {
                    // This might be subscription confirmation
                    let _ = event_tx.send(TokenEvent::Info {
                        message: format!("Subscription response received: {}", method),
                        timestamp: Utc::now(),
                    });
                    continue;
                }
            }
            
            // Check if this is a bonding curve initialization
            let is_init = is_initialize_bonding_curve(&notification);
            if !is_init {
                // Log occasionally what kind of logs we're seeing
                if let Some(logs) = notification["params"]["result"]["value"]["logs"].as_array() {
                    if message_count % 100 == 0 && !logs.is_empty() {
                        let first_log = logs.first().and_then(|l| l.as_str()).unwrap_or("");
                        let _ = event_tx.send(TokenEvent::Info {
                            message: format!("Received logs (not pump.fun): {}...", &first_log[..first_log.len().min(50)]),
                            timestamp: Utc::now(),
                        });
                    }
                }
                continue;
            }
            
            // Found a potential token!
            let _ = event_tx.send(TokenEvent::Info {
                message: "Potential token detected, processing...".to_string(),
                timestamp: Utc::now(),
            });
            
            let init_signature = match extract_signature(&notification) {
                Some(s) => s,
                None => {
                    let _ = event_tx.send(TokenEvent::Error {
                        message: "Token detected but signature missing!".to_string(),
                        timestamp: Utc::now(),
                    });
                    continue;
                }
            };
            
            *detected += 1;
            
            {
                let mut m = metrics.write().unwrap();
                m.record_detection(0);
            }
            
            let _ = event_tx.send(TokenEvent::Info {
                message: format!("Processing token #{} (signature: {}...)", *detected, &init_signature[..8]),
                timestamp: Utc::now(),
            });
            
            let (accounts, mint_pubkey) = match PumpBuyAccounts::from_initialize_tx(rpc, &init_signature).await {
                Ok((acc, m)) => (acc, m),
                Err(e) => {
                    let _ = event_tx.send(TokenEvent::Error {
                        message: format!("Parse error: {}", e),
                        timestamp: Utc::now(),
                    });
                    continue;
                }
            };
            
            let mint = mint_pubkey.to_string();
            
            // Check if target mint is set - if so, only process that specific token
            if let Some(target_mint) = config.target_mint_address {
                if mint_pubkey != target_mint {
                    let _ = event_tx.send(TokenEvent::Filtered {
                        mint: mint.clone(),
                        reason: format!("Not target token (waiting for: {})", target_mint),
                        timestamp: Utc::now(),
                    });
                    continue;
                } else {
                    let _ = event_tx.send(TokenEvent::Info {
                        message: format!("🎯 Target token detected! {}", mint),
                        timestamp: Utc::now(),
                    });
                }
            }
            
            // Send detection event
            let _ = event_tx.send(TokenEvent::Detected {
                mint: mint.clone(),
                timestamp: Utc::now(),
            });
            
            // Duplicate check
            if !seen_tokens.check_and_mark(&mint) {
                let _ = event_tx.send(TokenEvent::Filtered {
                    mint,
                    reason: "Duplicate token".to_string(),
                    timestamp: Utc::now(),
                });
                continue;
            }
            
            // Process token
            match process_and_buy(
                config,
                wallet,
                rpc,
                tracker,
                *detected,
                init_signature,
                accounts,
                metrics.clone(),
                das_rate_limiter.clone(),
                socials_rate_limiter.clone(),
                event_tx.clone(),
            ).await {
                Ok(sig) => {
                    if let Some(signature) = sig {
                        // Get MC if available from tracker - extract value to avoid lifetime issues
                        let mc = {
                            let tracker_guard = tracker.read().ok();
                            if let Some(tracker_ref) = tracker_guard.as_ref().and_then(|t| t.as_ref()) {
                                let buys = tracker_ref.get_recent_buys(1);
                                buys.first().and_then(|b| b.mc_at_entry_usd)
                            } else {
                                None
                            }
                        };
                        
                        let _ = event_tx.send(TokenEvent::Bought {
                            mint,
                            signature,
                            mc,
                            timestamp: Utc::now(),
                        });
                    }
                }
                Err(e) => {
                    let reason = if e.to_string().contains("SKIP") {
                        e.to_string()
                    } else {
                        format!("Processing error: {}", e)
                    };
                    
                    let _ = event_tx.send(TokenEvent::Filtered {
                        mint,
                        reason,
                        timestamp: Utc::now(),
                    });
                }
            }
        } else if let WsMessage::Ping(data) = msg {
            // Respond to ping with pong
            if let Err(e) = write.send(WsMessage::Pong(data)).await {
                let _ = event_tx.send(TokenEvent::Error {
                    message: format!("Failed to send pong: {}", e),
                    timestamp: Utc::now(),
                });
                return Err(anyhow!("Failed to send pong: {}", e));
            }
                    } else if let WsMessage::Close(_) = msg {
                        // Connection closed by server
                        let _ = event_tx.send(TokenEvent::Info {
                            message: "WebSocket connection closed by server".to_string(),
                            timestamp: Utc::now(),
                        });
                        return Ok(false); // Normal exit
                    }
                    // Ignore other message types (Pong, Binary, etc.)
                    }
                    None => {
                        // WebSocket stream ended
                        return Ok(false); // Normal exit
                    }
                }
            }
        }
    }
}

async fn process_and_buy(
    config: &Config,
    wallet: &Keypair,
    rpc: &RpcClient,
    tracker: &Arc<std::sync::RwLock<Option<TokenTracker>>>,
    token_number: u32,
    init_signature: String,
    accounts: PumpBuyAccounts,
    metrics: SharedMetrics,
    das_rate_limiter: Arc<crate::rate_limiter::RateLimiter>,
    socials_rate_limiter: Arc<crate::rate_limiter::RateLimiter>,
    event_tx: mpsc::UnboundedSender<TokenEvent>,
) -> Result<Option<String>> {
    let process_start = std::time::Instant::now();
    let mint = accounts.mint;
    let dev_buy_lamports = accounts.dev_buy_sol;
    let dev_buy_sol = dev_buy_lamports as f64 / 1e9;
    
    let min_sol = config.min_dev_buy_usd / config.sol_price_usd;
    let max_sol = config.max_dev_buy_usd / config.sol_price_usd;
    
    let filter_start = std::time::Instant::now();
    
    // Dev buy filter
    if dev_buy_sol < min_sol || dev_buy_sol > max_sol {
        let filter_time = filter_start.elapsed().as_millis() as u64;
        {
            let mut m = metrics.write().unwrap();
            m.record_filter(FilterReason::DevBuy, filter_time);
        }
        return Err(anyhow!("SKIP: Dev buy {:.2} SOL (want {:.2}-{:.2})",
                           dev_buy_sol, min_sol, max_sol));
    }
    
    let require_socials = config.require_socials;
    let require_twitter = config.require_twitter;
    let min_socials = config.min_socials_count;
    let mint_str = mint.to_string();
    
    let api_key = config.helius_api_key.clone();
    let creator = accounts.creator;
    // ⚡ PARALLEL: Run all checks simultaneously for maximum speed
    let das_rate_limiter_clone = das_rate_limiter.clone();
    let socials_rate_limiter_clone = socials_rate_limiter.clone();
    let metrics_clone = metrics.clone();
    let metrics_clone_socials = metrics.clone(); // Clone for socials future
    let api_key_das = api_key.clone();
    let api_key_socials = api_key.clone();
    let mint_str_socials = mint_str.clone();
    
    // ⚡ PARALLEL: Run all checks simultaneously using futures::future::join_all
    // Use explicit async blocks with type annotations
    let das_fut: std::pin::Pin<Box<dyn std::future::Future<Output = Result<u32, anyhow::Error>> + Send>> = Box::pin(async move {
        if let Err(_e) = das_rate_limiter_clone.check() {
            let mut m = metrics_clone.write().unwrap();
            m.record_error(ErrorType::Network);
            Err(anyhow!("DAS rate limit exceeded"))
        } else {
            check_creator_token_count_das(&creator, &api_key_das).await
        }
    });
    
    let socials_fut: std::pin::Pin<Box<dyn std::future::Future<Output = Option<Socials>> + Send>> = Box::pin(async move {
        if require_socials || require_twitter || min_socials > 0 {
            if let Err(_e) = socials_rate_limiter_clone.check() {
                let mut m = metrics_clone_socials.write().unwrap();
                m.record_error(ErrorType::Network);
                None
            } else {
                check_token_socials(&mint_str_socials, &api_key_socials).await.ok()
            }
        } else {
            None
        }
    });
    
    let mc_fut = fetch_bonding_curve_mc(
        rpc,
        &accounts.bonding_curve,
        config.sol_price_usd,
    );
    
    // Await all futures - they run concurrently due to Box::pin
    let das_result = das_fut.await;
    let mc_result = mc_fut.await;
    let socials_result = socials_fut.await;
    
    // Process DAS result
    let creator_count = match das_result {
        Ok(count) => {
            eprintln!("🔍 Filter check: creator_count={}, min_dev_tokens={}, max_dev_tokens={}", 
                     count, config.min_dev_tokens, config.max_dev_tokens);
            if count < config.min_dev_tokens as u32 {
                let filter_time = filter_start.elapsed().as_millis() as u64;
                if let Ok(mut m) = metrics.write() {
                    m.record_filter(FilterReason::CreatorCount, filter_time);
                    m.record_error(ErrorType::Validation);
                }
                return Err(anyhow!("SKIP: Creator has only {} tokens (min: {})",
                                   count, config.min_dev_tokens));
            }
            if count > config.max_dev_tokens as u32 {
                let filter_time = filter_start.elapsed().as_millis() as u64;
                if let Ok(mut m) = metrics.write() {
                    m.record_filter(FilterReason::CreatorCount, filter_time);
                    m.record_error(ErrorType::Validation);
                }
                return Err(anyhow!("SKIP: Creator has {} tokens (max: {})",
                                   count, config.max_dev_tokens));
            }
            count
        }
        Err(e) => {
            let filter_time = filter_start.elapsed().as_millis() as u64;
            let mut m = metrics.write().unwrap();
            m.record_error(ErrorType::Network);
            // Network errors don't count as filter rejections
            return Err(anyhow!("DAS check failed: {}", e));
        }
    };
    
    // Process MC result
    let (curve, _mc_sol, mc_usd) = match mc_result {
        Ok(data) => data,
        Err(e) => {
            (BondingCurveAccount::default(), 0.0, 0.0)
        }
    };
    
    let token_price_sol = curve.get_token_price_sol();
    
    // Process socials
    let socials_opt = if let Some(socials) = socials_result {
        if require_socials && !socials.has_any() {
            let filter_time = filter_start.elapsed().as_millis() as u64;
            if let Ok(mut m) = metrics.write() {
                m.record_filter(FilterReason::Socials, filter_time);
            }
            return Err(anyhow!("SKIP: No socials"));
        }
        if require_twitter && !socials.has_twitter() {
            let filter_time = filter_start.elapsed().as_millis() as u64;
            if let Ok(mut m) = metrics.write() {
                m.record_filter(FilterReason::Socials, filter_time);
            }
            return Err(anyhow!("SKIP: No Twitter"));
        }
        if socials.count() < min_socials {
            let filter_time = filter_start.elapsed().as_millis() as u64;
            if let Ok(mut m) = metrics.write() {
                m.record_filter(FilterReason::Socials, filter_time);
            }
            return Err(anyhow!("SKIP: Need {} socials", min_socials));
        }
        Some(socials)
    } else {
        if require_socials {
            let filter_time = filter_start.elapsed().as_millis() as u64;
            if let Ok(mut m) = metrics.write() {
                m.record_filter(FilterReason::Socials, filter_time);
                m.record_error(ErrorType::Network);
            }
            return Err(anyhow!("SKIP: Could not verify socials"));
        }
        None
    };
    
    // Build transaction
    let user_wallet = wallet.pubkey();
    let user_ata = get_associated_token_address(&user_wallet, &accounts.mint);
    
    let buy_ix = build_buy_instruction(
        rpc,
        &accounts,
        &user_wallet,
        &user_ata,
        config.buy_amount_lamports(),
    ).await?;
    
    let mut rng = rand::thread_rng();
    use crate::constants::HELIUS_TIP_ACCOUNTS;
    let tip_account = Pubkey::from_str(
        HELIUS_TIP_ACCOUNTS.choose(&mut rng).unwrap()
    )?;
    
    let instructions = vec![
        ComputeBudgetInstruction::set_compute_unit_limit(config.compute_units),
        ComputeBudgetInstruction::set_compute_unit_price(config.priority_fee),
        create_associated_token_account(
            &user_wallet,
            &user_wallet,
            &accounts.mint,
            &spl_token::id(),
        ),
        buy_ix,
        system_instruction::transfer(
            &user_wallet,
            &tip_account,
            config.jito_tip,
        ),
    ];
    
    let recent_blockhash = rpc.get_latest_blockhash().await?;
    
    // Check if mock buy mode is enabled - skip validation if mock buy
    if config.mock_buy {
        // Generate a mock signature for testing
        use solana_sdk::signature::Signature;
        use rand::Rng;
        let mut rng = rand::thread_rng();
        let mut mock_sig_bytes = [0u8; 64];
        rng.fill(&mut mock_sig_bytes);
        let mock_sig = Signature::from(mock_sig_bytes);
        let mock_signature = format!("MOCK_{}", mock_sig.to_string());
        
        eprintln!("🧪 MOCK BUY: Simulating buy for {} SOL (signature: {})", 
                 config.buy_amount_sol, mock_signature);
        
        // Record as successful submission
        let mut m = metrics.write().unwrap();
        m.record_submission(SubmissionMethod::Helius, true, 0);
        
        // Try to get MC for tracker (optional, don't fail if it doesn't work)
        let mc_entry_result = fetch_bonding_curve_mc(
            rpc,
            &accounts.bonding_curve,
            config.sol_price_usd,
        ).await;
        
        let (mc_entry_usd, token_price_entry) = match mc_entry_result {
            Ok((curve, _, mc)) => {
                let price = curve.get_token_price_sol();
                (Some(mc), if price > 0.0 { Some(price) } else { None })
            }
            Err(_) => (None, None)
        };
        
        // Record buy in tracker (same as real buy)
        eprintln!("📝 Attempting to record mock buy in tracker...");
        match tracker.write() {
            Ok(mut tracker_opt) => {
                match tracker_opt.as_mut() {
                    Some(tracker) => {
                        eprintln!("✅ Tracker is initialized, recording buy...");
                        let buy = TokenBuy {
                            token_number,
                            mint: mint.to_string(),
                            signature: init_signature.clone(),
                            creator: accounts.creator.to_string(),
                            dev_buy_sol,
                            our_buy_sol: config.buy_amount_sol,
                            timestamp: Utc::now(),
                            has_socials: socials_opt.as_ref().map(|s| s.has_any()).unwrap_or(false),
                            twitter: socials_opt.as_ref().and_then(|s| s.twitter.clone()),
                            website: socials_opt.as_ref().and_then(|s| s.website.clone()),
                            telegram: socials_opt.as_ref().and_then(|s| s.telegram.clone()),
                            creator_token_count: creator_count,
                            detection_method: if dev_buy_lamports > 0 {
                                "instruction".to_string()
                            } else {
                                "balance_fallback".to_string()
                            },
                            mc_at_detection_usd: if mc_usd > 0.0 { Some(mc_usd) } else { None },
                            mc_at_entry_usd: mc_entry_usd,
                            token_price_sol: token_price_entry.or(if token_price_sol > 0.0 { Some(token_price_sol) } else { None }),
                        };
                        
                        match tracker.record_buy(buy) {
                            Ok(_) => {
                                eprintln!("✅ Mock buy successfully recorded in tracker (total buys: {})", tracker.total_buys());
                            }
                            Err(e) => {
                                eprintln!("⚠️  Tracker error (mock buy): {}", e);
                            }
                        }
                    }
                    None => {
                        eprintln!("⚠️  Tracker is None - tracking might be disabled or not initialized");
                    }
                }
            }
            Err(e) => {
                eprintln!("⚠️  Failed to acquire tracker write lock (mock buy): {}", e);
            }
        }
        
        // Send mock buy event
        let _ = event_tx.send(TokenEvent::Bought {
            mint: accounts.mint.to_string(),
            signature: mock_signature.clone(),
            mc: mc_entry_usd,
            timestamp: Utc::now(),
        });
        
        return Ok(Some(mock_signature));
    }
    
    // Only validate if NOT in mock buy mode
    if let Err(e) = crate::validation::validate_before_submission(
        rpc,
        &user_wallet,
        config.buy_amount_lamports(),
        &recent_blockhash,
    ).await {
        let mut m = metrics.write().unwrap();
        m.record_error(ErrorType::Validation);
        return Err(anyhow!("Pre-flight validation failed: {}", e));
    }
    
    let msg = v0::Message::try_compile(
        &user_wallet,
        &instructions,
        &[],
        recent_blockhash,
    )?;
    
    let tx = VersionedTransaction::try_new(
        VersionedMessage::V0(msg),
        &[wallet],
    )?;
    
    let tx_sig = tx.signatures[0];
    
    let tx_helius = tx.clone();
    let tx_jito = tx.clone();
    let tx_rpc = tx.clone();
    
    let wallet_bytes = wallet.to_bytes();
    let wallet_clone = Keypair::from_bytes(&wallet_bytes)?;
    let jito_tip = config.jito_tip;
    let rpc_url = config.rpc_url.clone();
    
    let helius_task = tokio::spawn(async move {
        match send_helius_transaction(tx_helius).await {
            Ok(sig) => Ok(format!("Helius: {}", sig)),
            Err(e) => Err(e),
        }
    });
    
    let jito_task = tokio::spawn(async move {
        match send_jito_bundle(tx_jito, &wallet_clone, recent_blockhash, jito_tip).await {
            Ok(bundle_id) => Ok(format!("Jito: {}", bundle_id)),
            Err(e) => Err(e),
        }
    });
    
    let rpc_task = tokio::spawn(async move {
        let rpc_client = RpcClient::new(rpc_url);
        match rpc_client.send_transaction(&tx_rpc).await {
            Ok(sig) => Ok(format!("RPC: {}", sig)),
            Err(e) => Err(anyhow::anyhow!("RPC error: {}", e)),
        }
    });
    
    let submission_start = std::time::Instant::now();
    let result = tokio::select! {
        res = helius_task => {
            let submission_time = submission_start.elapsed().as_millis() as u64;
            match res {
                Ok(Ok(_)) => {
                    let mut m = metrics.write().unwrap();
                    m.record_submission(SubmissionMethod::Helius, true, submission_time);
                    Ok(())
                }
                Ok(Err(e)) => {
                    let mut m = metrics.write().unwrap();
                    m.record_submission(SubmissionMethod::Helius, false, submission_time);
                    m.record_error(ErrorType::Submission);
                    Err(anyhow!("Helius failed: {}", e))
                }
                Err(e) => {
                    let mut m = metrics.write().unwrap();
                    m.record_submission(SubmissionMethod::Helius, false, submission_time);
                    m.record_error(ErrorType::Submission);
                    Err(anyhow!("Helius task error: {}", e))
                }
            }
        }
        res = jito_task => {
            let submission_time = submission_start.elapsed().as_millis() as u64;
            match res {
                Ok(Ok(_)) => {
                    let mut m = metrics.write().unwrap();
                    m.record_submission(SubmissionMethod::Jito, true, submission_time);
                    Ok(())
                }
                Ok(Err(e)) => {
                    let mut m = metrics.write().unwrap();
                    m.record_submission(SubmissionMethod::Jito, false, submission_time);
                    m.record_error(ErrorType::Submission);
                    Err(anyhow!("Jito failed: {}", e))
                }
                Err(e) => {
                    let mut m = metrics.write().unwrap();
                    m.record_submission(SubmissionMethod::Jito, false, submission_time);
                    m.record_error(ErrorType::Submission);
                    Err(anyhow!("Jito task error: {}", e))
                }
            }
        }
        res = rpc_task => {
            let submission_time = submission_start.elapsed().as_millis() as u64;
            match res {
                Ok(Ok(_)) => {
                    let mut m = metrics.write().unwrap();
                    m.record_submission(SubmissionMethod::Rpc, true, submission_time);
                    Ok(())
                }
                Ok(Err(e)) => {
                    let mut m = metrics.write().unwrap();
                    m.record_submission(SubmissionMethod::Rpc, false, submission_time);
                    m.record_error(ErrorType::Rpc);
                    Err(anyhow!("RPC failed: {}", e))
                }
                Err(e) => {
                    let mut m = metrics.write().unwrap();
                    m.record_submission(SubmissionMethod::Rpc, false, submission_time);
                    m.record_error(ErrorType::Rpc);
                    Err(anyhow!("RPC task error: {}", e))
                }
            }
        }
    };
    
    if result.is_ok() {
        // Wait a bit for TX to settle, then fetch entry MC
        tokio::time::sleep(Duration::from_millis(300)).await;
        
        let mc_entry_result = fetch_bonding_curve_mc(
            rpc,
            &accounts.bonding_curve,
            config.sol_price_usd,
        ).await;
        
        let (mc_entry_usd, token_price_entry) = match mc_entry_result {
            Ok((curve, _, mc)) => {
                let price = curve.get_token_price_sol();
                (Some(mc), if price > 0.0 { Some(price) } else { None })
            }
            Err(_) => (None, None)
        };
        
        // Record buy in tracker
        if let Ok(mut tracker_opt) = tracker.write() {
            if let Some(tracker) = tracker_opt.as_mut() {
                let buy = TokenBuy {
                    token_number,
                    mint: mint.to_string(),
                    signature: init_signature.clone(),
                    creator: accounts.creator.to_string(),
                    dev_buy_sol,
                    our_buy_sol: config.buy_amount_sol,
                    timestamp: Utc::now(),
                    has_socials: socials_opt.as_ref().map(|s| s.has_any()).unwrap_or(false),
                    twitter: socials_opt.as_ref().and_then(|s| s.twitter.clone()),
                    website: socials_opt.as_ref().and_then(|s| s.website.clone()),
                    telegram: socials_opt.as_ref().and_then(|s| s.telegram.clone()),
                    creator_token_count: creator_count,
                    detection_method: if dev_buy_lamports > 0 {
                        "instruction".to_string()
                    } else {
                        "balance_fallback".to_string()
                    },
                    mc_at_detection_usd: if mc_usd > 0.0 { Some(mc_usd) } else { None },
                    mc_at_entry_usd: mc_entry_usd,
                    token_price_sol: token_price_entry.or(if token_price_sol > 0.0 { Some(token_price_sol) } else { None }),
                };
                
                if let Err(e) = tracker.record_buy(buy) {
                    eprintln!("Tracker error: {}", e);
                }
            }
        }
        
        Ok(Some(init_signature))
    } else {
        Err(result.unwrap_err())
    }
}

