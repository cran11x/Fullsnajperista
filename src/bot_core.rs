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
use spl_associated_token_account::{
    get_associated_token_address, 
    instruction::{
        create_associated_token_account,
        create_associated_token_account_idempotent,
    },
};
use solana_sdk::instruction::Instruction;
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
use crate::sell::build_sell_instruction;
use spl_token::state::Account as TokenAccount;
use solana_sdk::program_pack::Pack;
use solana_sdk::commitment_config::CommitmentConfig;
use solana_transaction_status::UiTransactionEncoding;
use solana_client::rpc_config::RpcTransactionConfig;

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
    
    // Start position monitor as background task
    let monitor_config = config.clone();
    let monitor_wallet_bytes = wallet.to_bytes();
    let monitor_wallet = Keypair::from_bytes(&monitor_wallet_bytes)?;
    let monitor_rpc = initial_config.create_rpc_client();
    let monitor_tracker = tracker.clone();
    let monitor_event_tx = event_tx.clone();
    
    tokio::spawn(async move {
        monitor_positions(
            monitor_config,
            monitor_wallet,
            monitor_rpc,
            monitor_tracker,
            monitor_event_tx,
        ).await;
    });
    
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
            &config, // Pass live config reference for target_mint_address
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
    config: &Config, // Note: This is a snapshot for WebSocket URL, but we'll read live config for target_mint
    config_arc: &Arc<std::sync::RwLock<Config>>, // Live config reference for target_mint_address
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
            // ✅ FIXED: Read config fresh each time to get latest target_mint_address
            let current_target_mint = {
                let config_guard = config_arc.read().unwrap();
                config_guard.target_mint_address
            };
            
            if let Some(target_mint) = current_target_mint {
                println!("      🎯 Target mint check: detected={}, target={}", mint_pubkey, target_mint);
                if mint_pubkey != target_mint {
                    println!("      ❌ Token {} does not match target {}", mint_pubkey, target_mint);
                    let _ = event_tx.send(TokenEvent::Filtered {
                        mint: mint.clone(),
                        reason: format!("Not target token (waiting for: {})", target_mint),
                        timestamp: Utc::now(),
                    });
                    continue;
                } else {
                    println!("      ✅ Target token MATCH! {}", mint);
                    let _ = event_tx.send(TokenEvent::Info {
                        message: format!("🎯 Target token detected! {}", mint),
                        timestamp: Utc::now(),
                    });
                }
            } else {
                // No target mint set - process all tokens
                println!("      ℹ️  No target mint set - processing all tokens");
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
                        eprintln!("✅ Buy successful! Signature: {}", signature);
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
                            signature: signature.clone(),
                            mc,
                            timestamp: Utc::now(),
                        });
                        
                        // Check if one shot mode is enabled - stop bot after successful buy
                        if config.one_shot_mode {
                            eprintln!("🎯 One Shot Mode: Buy successful, stopping bot...");
                            let _ = event_tx.send(TokenEvent::Info {
                                message: "One Shot Mode: Buy successful, stopping bot".to_string(),
                                timestamp: Utc::now(),
                            });
                            // Break from WebSocket loop to stop bot
                            return Ok(true);
                        }
                    } else {
                        eprintln!("⚠️  Buy returned None signature - buy may not have been recorded");
                    }
                }
                Err(e) => {
                    eprintln!("❌ Processing error: {}", e);
                    let reason = if e.to_string().contains("SKIP") {
                        e.to_string()
                    } else {
                        format!("Processing error: {}", e)
                    };
                    
                    let _ = event_tx.send(TokenEvent::Filtered {
                        mint,
                        reason: reason.clone(),
                        timestamp: Utc::now(),
                    });
                    
                    // In one shot mode, also stop on errors to prevent wasting credits
                    // (but not on SKIP errors which are expected filter rejections)
                    if config.one_shot_mode && !reason.contains("SKIP") {
                        eprintln!("🎯 One Shot Mode: Processing error occurred, stopping bot to prevent wasting credits");
                        let _ = event_tx.send(TokenEvent::Info {
                            message: "One Shot Mode: Error occurred, stopping bot".to_string(),
                            timestamp: Utc::now(),
                        });
                        return Ok(true);
                    }
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
    mut accounts: PumpBuyAccounts,
    metrics: SharedMetrics,
    das_rate_limiter: Arc<crate::rate_limiter::RateLimiter>,
    socials_rate_limiter: Arc<crate::rate_limiter::RateLimiter>,
    event_tx: mpsc::UnboundedSender<TokenEvent>,
) -> Result<Option<String>> {
    let mint = accounts.mint;
    let dev_buy_lamports = accounts.dev_buy_sol;
    let dev_buy_sol = dev_buy_lamports as f64 / 1e9;
    
    // ========================================================================
    // SECTION 1: TOKEN INFO
    // ========================================================================
    eprintln!();
    eprintln!("╔═══════════════════════════════════════════════════════════════╗");
    eprintln!("║                    TOKEN DETECTED                              ║");
    eprintln!("╚═══════════════════════════════════════════════════════════════╝");
    eprintln!("  Mint:           {}", mint);
    eprintln!("  Creator:        {}", accounts.creator);
    eprintln!("  Dev Buy:        {:.6} SOL ({:.2} USD)", dev_buy_sol, dev_buy_sol * config.sol_price_usd);
    eprintln!("  Bonding Curve:  {}", accounts.bonding_curve);
    eprintln!("  Creator Vault:  {}", accounts.creator_vault);
    eprintln!();
    
    let min_sol = config.min_dev_buy_usd / config.sol_price_usd;
    let max_sol = config.max_dev_buy_usd / config.sol_price_usd;
    let filter_start = std::time::Instant::now();
    
    // ========================================================================
    // SECTION 2: FILTER CHECKS
    // ========================================================================
    eprintln!("╔═══════════════════════════════════════════════════════════════╗");
    eprintln!("║                    FILTER CHECKS                             ║");
    eprintln!("╚═══════════════════════════════════════════════════════════════╝");
    
    // Filter #1: Dev Buy Amount
    eprintln!("  [1/3] Dev Buy Amount");
    eprintln!("        Current:  {:.6} SOL ({:.2} USD)", dev_buy_sol, dev_buy_sol * config.sol_price_usd);
    eprintln!("        Required: {:.6} - {:.6} SOL ({:.2} - {:.2} USD)", min_sol, max_sol, config.min_dev_buy_usd, config.max_dev_buy_usd);
    if dev_buy_sol < min_sol || dev_buy_sol > max_sol {
        let filter_time = filter_start.elapsed().as_millis() as u64;
        {
            let mut m = metrics.write().unwrap();
            m.record_filter(FilterReason::DevBuy, filter_time);
        }
        eprintln!("        ❌ FAILED: Outside required range");
        eprintln!();
        return Err(anyhow!("SKIP: Dev buy {:.2} SOL (want {:.2}-{:.2})",
                           dev_buy_sol, min_sol, max_sol));
    }
    eprintln!("        ✅ PASSED");
    
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
    
    // Filter #2: Creator Token Count
    eprintln!("  [2/3] Creator Token Count");
    let (creator_count, das_check_failed) = match das_result {
        Ok(count) => {
            eprintln!("        Current:  {} tokens", count);
            eprintln!("        Required: {} - {} tokens", config.min_dev_tokens, config.max_dev_tokens);
            if count < config.min_dev_tokens as u32 {
                let filter_time = filter_start.elapsed().as_millis() as u64;
                if let Ok(mut m) = metrics.write() {
                    m.record_filter(FilterReason::CreatorCount, filter_time);
                    m.record_error(ErrorType::Validation);
                }
                eprintln!("        ❌ FAILED: Below minimum ({} < {})", count, config.min_dev_tokens);
                eprintln!();
                return Err(anyhow!("SKIP: Creator has only {} tokens (min: {})",
                                   count, config.min_dev_tokens));
            }
            if count > config.max_dev_tokens as u32 {
                let filter_time = filter_start.elapsed().as_millis() as u64;
                if let Ok(mut m) = metrics.write() {
                    m.record_filter(FilterReason::CreatorCount, filter_time);
                    m.record_error(ErrorType::Validation);
                }
                eprintln!("        ❌ FAILED: Above maximum ({} > {})", count, config.max_dev_tokens);
                eprintln!();
                return Err(anyhow!("SKIP: Creator has {} tokens (max: {})",
                                   count, config.max_dev_tokens));
            }
            eprintln!("        ✅ PASSED");
            (count, false)
        }
        Err(e) => {
            let _filter_time = filter_start.elapsed().as_millis() as u64;
            let mut m = metrics.write().unwrap();
            m.record_error(ErrorType::Network);
            eprintln!("        ⚠️  WARNING: DAS check failed ({})", e);
            eprintln!("        ⚠️  Continuing anyway (unknown count)");
            (0, true)
        }
    };
    
    // Process MC result
    let (curve, _mc_sol, mc_usd) = match mc_result {
        Ok(data) => data,
        Err(_) => {
            (BondingCurveAccount::default(), 0.0, 0.0)
        }
    };
    
    let token_price_sol = curve.get_token_price_sol();
    
    // Filter #3: Socials
    eprintln!("  [3/3] Socials");
    eprintln!("        Required:  socials={}, twitter={}, min_count={}", require_socials, require_twitter, min_socials);
    let socials_opt = if let Some(socials) = socials_result {
        eprintln!("        Found:     Twitter={}, Website={}, Telegram={}", 
                 socials.has_twitter(), socials.has_website(), socials.has_telegram());
        eprintln!("        Total:     {} socials", socials.count());
        if require_socials && !socials.has_any() {
            let filter_time = filter_start.elapsed().as_millis() as u64;
            if let Ok(mut m) = metrics.write() {
                m.record_filter(FilterReason::Socials, filter_time);
            }
            eprintln!("        ❌ FAILED: No socials found (required)");
            eprintln!();
            return Err(anyhow!("SKIP: No socials"));
        }
        if require_twitter && !socials.has_twitter() {
            let filter_time = filter_start.elapsed().as_millis() as u64;
            if let Ok(mut m) = metrics.write() {
                m.record_filter(FilterReason::Socials, filter_time);
            }
            eprintln!("        ❌ FAILED: No Twitter found (required)");
            eprintln!();
            return Err(anyhow!("SKIP: No Twitter"));
        }
        if socials.count() < min_socials {
            let filter_time = filter_start.elapsed().as_millis() as u64;
            if let Ok(mut m) = metrics.write() {
                m.record_filter(FilterReason::Socials, filter_time);
            }
            eprintln!("        ❌ FAILED: Only {} socials (minimum: {})", socials.count(), min_socials);
            eprintln!();
            return Err(anyhow!("SKIP: Need {} socials", min_socials));
        }
        eprintln!("        ✅ PASSED");
        Some(socials)
    } else {
        if require_socials {
            let filter_time = filter_start.elapsed().as_millis() as u64;
            if let Ok(mut m) = metrics.write() {
                m.record_filter(FilterReason::Socials, filter_time);
                m.record_error(ErrorType::Network);
            }
            eprintln!("        ❌ FAILED: Could not verify socials (network error)");
            eprintln!();
            return Err(anyhow!("SKIP: Could not verify socials"));
        }
        eprintln!("        ✅ PASSED: Not required");
        None
    };
    
    eprintln!("╚═══════════════════════════════════════════════════════════════╝");
    eprintln!("  ✅ ALL FILTERS PASSED - Proceeding with buy");
    eprintln!();
    
    // ========================================================================
    // SECTION 3: ACCOUNT VERIFICATION
    // ========================================================================
    eprintln!("╔═══════════════════════════════════════════════════════════════╗");
    eprintln!("║                 ACCOUNT VERIFICATION                           ║");
    eprintln!("╚═══════════════════════════════════════════════════════════════╝");
    
    // Verify bonding curve account
    eprintln!("  [1/3] Bonding Curve Account");
    eprintln!("        Address: {}", accounts.bonding_curve);
    let mut bonding_curve_ready = false;
    let max_wait_attempts = 5;
    let wait_interval_ms = 100;
    
    for attempt in 1..=max_wait_attempts {
        match rpc.get_account_with_commitment(&accounts.bonding_curve, CommitmentConfig::confirmed()).await {
            Ok(account_info) => {
                let account = match account_info.value {
                    Some(acc) => acc,
                    None => {
                        if attempt < max_wait_attempts {
                            tokio::time::sleep(Duration::from_millis(wait_interval_ms)).await;
                            continue;
                        } else {
                            eprintln!("        ❌ FAILED: Account not found");
                            eprintln!();
                            return Err(anyhow!("SKIP: Bonding curve account not found - token not ready"));
                        }
                    }
                };
                
                if account.data.is_empty() {
                    if attempt < max_wait_attempts {
                        tokio::time::sleep(Duration::from_millis(wait_interval_ms)).await;
                        continue;
                    } else {
                        eprintln!("        ❌ FAILED: Account exists but is empty");
                        eprintln!();
                        return Err(anyhow!("SKIP: Bonding curve account not ready for trading"));
                    }
                }
                eprintln!("        ✅ PASSED: Initialized ({} bytes)", account.data.len());
                bonding_curve_ready = true;
                break;
            }
            Err(_) => {
                if attempt < max_wait_attempts {
                    tokio::time::sleep(Duration::from_millis(wait_interval_ms)).await;
                    continue;
                } else {
                    eprintln!("        ❌ FAILED: Account not found");
                    eprintln!();
                    return Err(anyhow!("SKIP: Bonding curve account not found - token not ready"));
                }
            }
        }
    }
    
    if !bonding_curve_ready {
        eprintln!("        ❌ FAILED: Account not ready");
        eprintln!();
        return Err(anyhow!("SKIP: Bonding curve account not ready"));
    }
    
    // Build transaction
    let user_wallet = wallet.pubkey();
    let token_program_2022_id = Pubkey::from_str("TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb").unwrap();
    let user_ata = spl_associated_token_account::get_associated_token_address_with_program_id(
        &user_wallet, 
        &accounts.mint,
        &token_program_2022_id
    );
    
    // Calculate token amount for tracking
    let token_amount = {
        use crate::buy::get_cached_global;
        if let Ok(global) = get_cached_global() {
            global.get_initial_buy_price(config.buy_amount_lamports())
        } else {
            0
        }
    };
    
    // Set hardcoded Global Volume
    let hardcoded_global_volume = Pubkey::from_str("Hq2wp8uJ9jCPsYgNHex8RtqdvMPfVGoYwjvF1ATiwn2Y")
        .expect("Invalid hardcoded Global Volume address");
    accounts.global_volume = hardcoded_global_volume;
    // ========================================================================
    // SECTION 4: BUILD BUY INSTRUCTION
    // ========================================================================
    eprintln!("╔═══════════════════════════════════════════════════════════════╗");
    eprintln!("║              BUILDING BUY INSTRUCTION                         ║");
    eprintln!("╚═══════════════════════════════════════════════════════════════╝");
    
    let buy_ix = build_buy_instruction(
        rpc,
        &accounts,
        &user_wallet,
        &user_ata,
        config.buy_amount_lamports(),
    ).await?;
    
    eprintln!("  ✅ Buy instruction built ({} accounts)", buy_ix.accounts.len());
    eprintln!();
    
    let mut rng = rand::thread_rng();
    use crate::constants::HELIUS_TIP_ACCOUNTS;
    let tip_account = Pubkey::from_str(
        HELIUS_TIP_ACCOUNTS.choose(&mut rng).unwrap()
    )?;
    
    // Check if ATA already exists
    let ata_exists = rpc.get_account(&user_ata).await.is_ok();
    
    let mut instructions = vec![
        ComputeBudgetInstruction::set_compute_unit_limit(config.compute_units),
        ComputeBudgetInstruction::set_compute_unit_price(config.priority_fee),
    ];
    
    // Create ATA instruction if needed
    {
        let ata_ix = if !ata_exists {
            create_associated_token_account(
                &user_wallet,
                &user_wallet,
                &accounts.mint,
                &token_program_2022_id,
            )
        } else {
            create_associated_token_account_idempotent(
                &user_wallet,
                &user_wallet,
                &accounts.mint,
                &token_program_2022_id,
            )
        };
        
        let expected_ata_program = Pubkey::from_str("ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL")
            .unwrap_or_else(|_| ata_ix.program_id);
        
        let fixed_ata_ix = if ata_ix.program_id != expected_ata_program {
            Instruction {
                program_id: expected_ata_program,
                accounts: ata_ix.accounts,
                data: ata_ix.data,
            }
        } else {
            ata_ix
        };
        
        instructions.push(fixed_ata_ix);
    }
    
    // Verify Associated Bonding Curve account
    eprintln!("  [2/3] Associated Bonding Curve");
    eprintln!("        Address: {}", accounts.associated_bonding_curve);
    let mut abc_account_opt = None;
    let max_wait_attempts = 5;
    
    for attempt in 1..=max_wait_attempts {
        match rpc.get_account_with_commitment(&accounts.associated_bonding_curve, CommitmentConfig::confirmed()).await {
            Ok(response) => {
                if let Some(account) = response.value {
                    if !account.data.is_empty() {
                        abc_account_opt = Some(account);
                        break;
                    }
                }
            }
            Err(_) => {}
        }
        
        if attempt < max_wait_attempts {
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }
    
    if let Some(account) = abc_account_opt {
        let owner = account.owner;
        let token_program_2022 = Pubkey::from_str("TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb").unwrap();
        let token_program = Pubkey::from_str("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA").unwrap();
        
        if account.data.is_empty() {
            eprintln!("        ❌ FAILED: Account not initialized");
            eprintln!();
            return Err(anyhow!("SKIP: Associated Bonding Curve account not initialized - token may not be ready"));
        }
        
        if owner != token_program_2022 && owner != token_program {
            eprintln!("        ❌ FAILED: Wrong owner ({})", owner);
            eprintln!();
            return Err(anyhow!("SKIP: Associated Bonding Curve has wrong owner - token may not be ready"));
        }
        
        eprintln!("        ✅ PASSED: Initialized ({} bytes)", account.data.len());
    } else {
        eprintln!("        ❌ FAILED: Account not found");
        eprintln!();
        return Err(anyhow!("SKIP: Associated Bonding Curve account does not exist - token not ready"));
    }
    
    // Verify User Token Account
    eprintln!("  [3/3] User Token Account");
    eprintln!("        Address: {}", user_ata);
    let user_ata_account = rpc.get_account(&user_ata).await;
    if let Ok(user_acc) = user_ata_account {
        let owner = user_acc.owner;
        let token_program_2022 = Pubkey::from_str("TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb").unwrap();
        let token_program = Pubkey::from_str("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA").unwrap();
        
        if owner != token_program_2022 && owner != token_program {
            eprintln!("        ⚠️  WARNING: Unexpected owner ({})", owner);
        } else if !user_acc.data.is_empty() {
            eprintln!("        ✅ PASSED: Initialized ({} bytes)", user_acc.data.len());
        } else {
            eprintln!("        ⚠️  WARNING: Exists but empty (will be initialized)");
        }
    } else {
        eprintln!("        ℹ️  Does not exist (will be created)");
    }
    
    eprintln!("╚═══════════════════════════════════════════════════════════════╝");
    eprintln!();
    
    // ========================================================================
    // SECTION 5: PDA VERIFICATION
    // ========================================================================
    eprintln!("╔═══════════════════════════════════════════════════════════════╗");
    eprintln!("║                    PDA VERIFICATION                           ║");
    eprintln!("╚═══════════════════════════════════════════════════════════════╝");
    
    let mut pda_mismatches = Vec::new();
    
    // Verify critical PDAs
    let (expected_global, _) = crate::pda_derivation::derive_global_pda();
    let (expected_bc, _) = crate::pda_derivation::derive_bonding_curve_pda(&accounts.mint);
    let (expected_ea, _) = crate::pda_derivation::derive_event_authority_pda();
    let (expected_user_volume, _) = crate::pda_derivation::derive_user_volume_pda(&user_wallet);
    
    let checks = vec![
        (0, "Global", expected_global),
        (3, "Bonding Curve", expected_bc),
        (10, "Event Authority", expected_ea),
        (13, "User Volume", expected_user_volume),
    ];
    
    for (idx, name, expected) in checks {
        if idx < buy_ix.accounts.len() {
            let actual = buy_ix.accounts[idx].pubkey;
            if actual != expected {
                eprintln!("  [{}] {} ❌ MISMATCH", idx, name);
                eprintln!("        Expected: {}", expected);
                eprintln!("        Got:      {}", actual);
                pda_mismatches.push((idx, name, actual, expected));
            } else {
                eprintln!("  [{}] {} ✅ VERIFIED", idx, name);
            }
        }
    }
    
    if !pda_mismatches.is_empty() {
        eprintln!();
        eprintln!("  ❌ CRITICAL: {} PDA mismatch(es) detected!", pda_mismatches.len());
        eprintln!("  ❌ Transaction will fail with Error 2006 - aborting!");
        eprintln!();
        return Err(anyhow!("CRITICAL: {} PDA mismatch(es) detected! This will cause Error 2006.", pda_mismatches.len()));
    }
    
    eprintln!("╚═══════════════════════════════════════════════════════════════╝");
    eprintln!("  ✅ ALL PDAs VERIFIED");
    eprintln!();
    
    // ========================================================================
    // SECTION 6: BUILD TRANSACTION
    // ========================================================================
    eprintln!("╔═══════════════════════════════════════════════════════════════╗");
    eprintln!("║                 BUILDING TRANSACTION                          ║");
    eprintln!("╚═══════════════════════════════════════════════════════════════╝");
    
    instructions.push(buy_ix);
    
    instructions.push(system_instruction::transfer(
        &user_wallet,
        &tip_account,
        config.jito_tip,
    ));
    
    eprintln!("  Instructions: {} total", instructions.len());
    eprintln!("    [0] Compute Budget (CU limit)");
    eprintln!("    [1] Compute Budget (CU price)");
    eprintln!("    [2] Create ATA");
    eprintln!("    [3] Buy");
    eprintln!("    [4] Jito Tip");
    eprintln!();
    
    let recent_blockhash = rpc.get_latest_blockhash().await?;
    
    // ========================================================================
    // SECTION 7: SEND TRANSACTION
    // ========================================================================
    eprintln!("╔═══════════════════════════════════════════════════════════════╗");
    eprintln!("║                 SENDING TRANSACTION                           ║");
    eprintln!("╚═══════════════════════════════════════════════════════════════╝");
    
    if config.mock_buy {
        eprintln!("  🧪 MOCK BUY MODE: Skipping transaction submission");
        
        // In mock mode, we don't send the transaction, but we can still check if we would have succeeded
        // by checking if the token account already exists (from a previous real buy)
        tokio::time::sleep(Duration::from_millis(500)).await;
        
        let token_account_exists = rpc.get_account(&user_ata).await.is_ok();
        if token_account_exists {
            let token_balance = match rpc.get_account_data(&user_ata).await {
                Ok(data) => {
                    if let Ok(token_account) = spl_token::state::Account::unpack(&data) {
                        token_account.amount
                    } else {
                        0
                    }
                }
                Err(_) => 0
            };
            
            if token_balance > 0 {
                eprintln!("  ✅ Token account exists (balance: {})", token_balance);
            } else {
                eprintln!("  ⚠️  Token account exists but balance is 0");
            }
        } else {
            eprintln!("  ℹ️  Token account does not exist (would be created in real mode)");
        }
        
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
        
        // Generate a mock signature for testing (but mark it clearly as MOCK)
        use solana_sdk::signature::Signature;
        use rand::Rng;
        let mut rng = rand::thread_rng();
        let mut mock_sig_bytes = [0u8; 64];
        rng.fill(&mut mock_sig_bytes);
        let mock_sig = Signature::from(mock_sig_bytes);
        let mock_signature = format!("MOCK_{}", mock_sig.to_string());
        
        // Record buy in tracker (same as real buy, but with MOCK signature)
        eprintln!("📝 Recording mock buy in tracker...");
        match tracker.write() {
            Ok(mut tracker_opt) => {
                match tracker_opt.as_mut() {
                    Some(tracker) => {
                        let buy = TokenBuy {
                            token_number,
                            mint: mint.to_string(),
                            signature: mock_signature.clone(), // Use mock signature
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
                            token_amount: if token_amount > 0 { Some(token_amount) } else { None },
                            user_token_account: Some(user_ata.to_string()),
                            bonding_curve: Some(accounts.bonding_curve.to_string()),
                            sold: false,
                            sell_signature: None,
                        };
                        
                        match tracker.record_buy(buy) {
                            Ok(_) => {
                                eprintln!("✅ Mock buy recorded in tracker (total buys: {})", tracker.total_buys());
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
        
        // Record as successful submission (mock)
        let mut m = metrics.write().unwrap();
        m.record_submission(SubmissionMethod::Helius, true, 0);
        
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
    // Use enhanced validation that includes all fees (priority fee, jito tip)
    if let Err(e) = crate::validation::validate_before_submission_with_fees(
        rpc,
        &user_wallet,
        config.buy_amount_lamports(),
        config.priority_fee,
        config.jito_tip,
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
    eprintln!("✅ Transaction built successfully, submitting via Helius, Jito, and RPC...");
    
    let tx_helius = tx.clone();
    let tx_jito = tx.clone();
    let tx_rpc = tx.clone();
    
    let wallet_bytes = wallet.to_bytes();
    let wallet_clone = Keypair::from_bytes(&wallet_bytes)?;
    let jito_tip = config.jito_tip;
    let rpc_url = config.rpc_url.clone();
    
    let helius_task = tokio::spawn(async move {
        match send_helius_transaction(tx_helius).await {
            Ok(sig) => Ok(sig), // Return signature directly, not formatted string
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
            Ok(sig) => Ok(sig.to_string()),
            Err(e) => Err(anyhow::anyhow!("RPC error: {}", e)),
        }
    });
    
    let submission_start = std::time::Instant::now();
    
    // Try all methods in parallel and use the first successful one
    // This ensures we don't give up if one method fails (e.g., Jito rate limit)
    // We use tokio::join! to wait for all, then pick the first successful one
    eprintln!("🚀 Starting parallel submission: Helius, Jito, RPC...");
    let (helius_res, jito_res, rpc_res) = tokio::join!(helius_task, jito_task, rpc_task);
    eprintln!("⏱️  All submission methods completed in {}ms", submission_start.elapsed().as_millis());
    
    // Process results and find the first successful one
    let mut results = Vec::new();
    
    // Process Helius result
    let submission_time = submission_start.elapsed().as_millis() as u64;
    match helius_res {
        Ok(Ok(sig)) => {
            let mut m = metrics.write().unwrap();
            m.record_submission(SubmissionMethod::Helius, true, submission_time);
            eprintln!("✅ Helius submission succeeded");
            results.push((Ok(()), SubmissionMethod::Helius, Some(sig)));
        }
        Ok(Err(e)) => {
            let mut m = metrics.write().unwrap();
            m.record_submission(SubmissionMethod::Helius, false, submission_time);
            m.record_error(ErrorType::Submission);
            eprintln!("❌ Helius failed: {}", e);
            results.push((Err(anyhow!("Helius failed: {}", e)), SubmissionMethod::Helius, None));
        }
        Err(e) => {
            let mut m = metrics.write().unwrap();
            m.record_submission(SubmissionMethod::Helius, false, submission_time);
            m.record_error(ErrorType::Submission);
            eprintln!("❌ Helius task error: {}", e);
            results.push((Err(anyhow!("Helius task error: {}", e)), SubmissionMethod::Helius, None));
        }
    }
    
    // Process Jito result
    match jito_res {
        Ok(Ok(bundle_id)) => {
            let mut m = metrics.write().unwrap();
            m.record_submission(SubmissionMethod::Jito, true, submission_time);
            eprintln!("✅ Jito submission succeeded");
            results.push((Ok(()), SubmissionMethod::Jito, Some(bundle_id)));
        }
        Ok(Err(e)) => {
            let mut m = metrics.write().unwrap();
            m.record_submission(SubmissionMethod::Jito, false, submission_time);
            m.record_error(ErrorType::Submission);
            eprintln!("❌ Jito failed: {}", e);
            results.push((Err(anyhow!("Jito failed: {}", e)), SubmissionMethod::Jito, None));
        }
        Err(e) => {
            let mut m = metrics.write().unwrap();
            m.record_submission(SubmissionMethod::Jito, false, submission_time);
            m.record_error(ErrorType::Submission);
            eprintln!("❌ Jito task error: {}", e);
            results.push((Err(anyhow!("Jito task error: {}", e)), SubmissionMethod::Jito, None));
        }
    }
    
    // Process RPC result
    match rpc_res {
        Ok(Ok(sig)) => {
            let mut m = metrics.write().unwrap();
            m.record_submission(SubmissionMethod::Rpc, true, submission_time);
            eprintln!("✅ RPC submission succeeded");
            results.push((Ok(()), SubmissionMethod::Rpc, Some(sig)));
        }
        Ok(Err(e)) => {
            let mut m = metrics.write().unwrap();
            m.record_submission(SubmissionMethod::Rpc, false, submission_time);
            m.record_error(ErrorType::Rpc);
            eprintln!("❌ RPC failed: {}", e);
            results.push((Err(anyhow!("RPC failed: {}", e)), SubmissionMethod::Rpc, None));
        }
        Err(e) => {
            let mut m = metrics.write().unwrap();
            m.record_submission(SubmissionMethod::Rpc, false, submission_time);
            m.record_error(ErrorType::Rpc);
            eprintln!("❌ RPC task error: {}", e);
            results.push((Err(anyhow!("RPC task error: {}", e)), SubmissionMethod::Rpc, None));
        }
    }
    
    // Find the first successful result
    let mut first_error = None;
    let (submission_result, submission_method, actual_signature) = 
        results.into_iter()
            .find_map(|(res, method, sig)| {
                if res.is_ok() {
                    Some((res, method, sig))
                } else {
                    if first_error.is_none() {
                        first_error = Some((res, method, sig));
                    }
                    None
                }
            })
            .unwrap_or_else(|| {
                first_error.unwrap_or((Err(anyhow!("All submission methods failed")), SubmissionMethod::Helius, None))
            });
    
    // Log submission result
    if let Some(sig) = &actual_signature {
        eprintln!("  Method:     {:?}", submission_method);
        eprintln!("  Signature:  {}", sig);
        eprintln!("  Link:       https://solscan.io/tx/{}", sig);
    } else {
        eprintln!("  Method:     {:?}", submission_method);
        eprintln!("  Status:     Failed");
    }
    
    // Verify transaction execution
    let buy_succeeded = if submission_result.is_ok() {
        if let Some(sig_str) = &actual_signature {
            if sig_str.starts_with("Jito:") {
                eprintln!("  Verification: Jito bundle (cannot verify immediately)");
                true
            } else {
                let sig = match solana_sdk::signature::Signature::from_str(&sig_str) {
                    Ok(s) => s,
                    Err(_) => {
                        eprintln!("  ❌ Invalid signature format");
                        return Err(anyhow!("Invalid transaction signature"));
                    }
                };
                
                tokio::time::sleep(Duration::from_millis(500)).await;
                
                match verify_transaction_success(rpc, &sig, &user_ata).await {
                    Ok(true) => {
                        eprintln!("  Verification: ✅ Buy succeeded");
                        true
                    }
                    Ok(false) => {
                        eprintln!("  Verification: ❌ Buy failed");
                        false
                    }
                    Err(e) => {
                        eprintln!("  Verification: ⚠️  Could not verify ({})", e);
                        true
                    }
                }
            }
        } else {
            false
        }
    } else {
        false
    };
    
    eprintln!("╚═══════════════════════════════════════════════════════════════╝");
    eprintln!();
    
    if buy_succeeded {
        eprintln!("╔═══════════════════════════════════════════════════════════════╗");
        eprintln!("║                    RECORDING BUY                             ║");
        eprintln!("╚═══════════════════════════════════════════════════════════════╝");
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
        
        if let Ok(mut tracker_opt) = tracker.write() {
            if let Some(tracker) = tracker_opt.as_mut() {
                let buy_signature = actual_signature.as_ref()
                    .map(|s| s.clone())
                    .unwrap_or_else(|| init_signature.clone());
                
                let buy = TokenBuy {
                    token_number,
                    mint: mint.to_string(),
                    signature: buy_signature,
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
                    token_amount: if token_amount > 0 { Some(token_amount) } else { None },
                    user_token_account: Some(user_ata.to_string()),
                    bonding_curve: Some(accounts.bonding_curve.to_string()),
                    sold: false,
                    sell_signature: None,
                };
                
                if let Err(e) = tracker.record_buy(buy) {
                    eprintln!("  ❌ Tracker error: {}", e);
                } else {
                    eprintln!("  ✅ Recorded in tracker (Total buys: {})", tracker.total_buys());
                }
            }
        }
        
        let final_signature = actual_signature.as_ref().map(|s| s.clone()).unwrap_or_else(|| init_signature.clone());
        eprintln!("╚═══════════════════════════════════════════════════════════════╝");
        eprintln!();
        Ok(Some(final_signature))
    } else {
        eprintln!("  ❌ Transaction failed or buy did not execute");
        eprintln!("╚═══════════════════════════════════════════════════════════════╝");
        eprintln!();
        
        if config.one_shot_mode {
            eprintln!("  🎯 One Shot Mode: Stopping bot");
        }
        
        match submission_result {
            Ok(_) => Err(anyhow!("Transaction submission succeeded but buy verification failed")),
            Err(e) => Err(e),
        }
    }
}

/// Verify that a transaction was successfully executed and buy instruction succeeded
async fn verify_transaction_success(
    rpc: &RpcClient,
    signature: &solana_sdk::signature::Signature,
    expected_token_account: &Pubkey,
) -> Result<bool> {
    // Try to get transaction status
    let tx_result = rpc.get_transaction_with_config(
        signature,
        RpcTransactionConfig {
            encoding: Some(UiTransactionEncoding::JsonParsed),
            max_supported_transaction_version: Some(0),
            commitment: Some(CommitmentConfig::confirmed()),
        }
    ).await;
    
    match tx_result {
        Ok(tx) => {
            // Check if transaction was successful
            if let Some(meta) = tx.transaction.meta {
                // Check if transaction errored
                if let Some(err) = meta.err {
                    eprintln!("   Transaction error: {:?}", err);
                    // Log inner instructions if available for debugging
                    eprintln!("   Transaction failed, checking if ATA creation was the issue...");
                    return Ok(false);
                }
                
                // Check if token account was created (indicates buy succeeded)
                // We can check if the expected token account exists
                if let Ok(account) = rpc.get_account(expected_token_account).await {
                    // Token account exists - buy likely succeeded
                    // But we should also check the token balance
                    if let Ok(token_account_data) = spl_token::state::Account::unpack(&account.data) {
                        if token_account_data.amount > 0 {
                            return Ok(true);
                        }
                    }
                }
                
                // Transaction succeeded but we can't verify token account
                // Assume success if transaction didn't error
                Ok(true)
            } else {
                // No metadata - can't verify
                Ok(false)
            }
        }
        Err(e) => {
            // Transaction not found or error getting it
            Err(anyhow!("Could not get transaction: {}", e))
        }
    }
}

/// Monitor active positions and trigger sells when conditions are met
async fn monitor_positions(
    config: Arc<std::sync::RwLock<Config>>,
    wallet: Keypair,
    rpc: RpcClient,
    tracker: Arc<std::sync::RwLock<Option<TokenTracker>>>,
    event_tx: mpsc::UnboundedSender<TokenEvent>,
) {
    loop {
        // Check if auto-sell is enabled
        let enabled = {
            let cfg = config.read().unwrap();
            cfg.enable_auto_sell
        };

        if !enabled {
            tokio::time::sleep(Duration::from_secs(10)).await;
            continue;
        }

        // Get config values
        let (stop_loss_percent, take_profit_mc_usd, monitor_interval, sol_price_usd) = {
            let cfg = config.read().unwrap();
            (cfg.stop_loss_percent, cfg.take_profit_mc_usd, cfg.monitor_interval_sec, cfg.sol_price_usd)
        };

        // Get active positions
        let active_positions = {
            if let Ok(tracker_guard) = tracker.read() {
                if let Some(tracker_ref) = tracker_guard.as_ref() {
                    tracker_ref.get_active_positions()
                } else {
                    Vec::new()
                }
            } else {
                Vec::new()
            }
        };

        if active_positions.is_empty() {
            tokio::time::sleep(Duration::from_secs(monitor_interval)).await;
            continue;
        }

        // Check each position
        for position in active_positions {
            // Skip if we don't have required data
            let bonding_curve_str = match &position.bonding_curve {
                Some(bc) => bc,
                None => continue,
            };

            let bonding_curve = match Pubkey::from_str(bonding_curve_str) {
                Ok(pk) => pk,
                Err(_) => continue,
            };

            let entry_mc = match position.mc_at_entry_usd {
                Some(mc) => mc,
                None => continue,
            };

            // Fetch current MC
            let current_mc_result = fetch_bonding_curve_mc(
                &rpc,
                &bonding_curve,
                sol_price_usd,
            ).await;

            let current_mc = match current_mc_result {
                Ok((_, _, mc_usd)) => mc_usd,
                Err(e) => {
                    eprintln!("Failed to fetch MC for {}: {}", position.mint, e);
                    continue;
                }
            };

            // Check stop loss: current_mc < entry_mc * (1.0 - stop_loss_percent/100.0)
            let stop_loss_threshold = entry_mc * (1.0 - stop_loss_percent / 100.0);
            let should_sell_stop_loss = current_mc < stop_loss_threshold;

            // Check take profit: current_mc >= take_profit_mc_usd
            let should_sell_take_profit = current_mc >= take_profit_mc_usd;

            if should_sell_stop_loss || should_sell_take_profit {
                let reason = if should_sell_stop_loss {
                    "stop_loss"
                } else {
                    "take_profit"
                };

                println!("🔄 Auto-selling {}: {} (Entry MC: ${:.0}, Current MC: ${:.0})",
                         reason, position.mint, entry_mc, current_mc);

                // Clone wallet for execute_sell
                let wallet_bytes = wallet.to_bytes();
                let wallet_clone = match Keypair::from_bytes(&wallet_bytes) {
                    Ok(kp) => kp,
                    Err(e) => {
                        eprintln!("Failed to clone wallet: {}", e);
                        continue;
                    }
                };

                // Get config clone to avoid holding lock across await
                let config_clone = {
                    let cfg = config.read().unwrap();
                    (*cfg).clone()
                };

                // Execute sell
                match execute_sell(
                    &config_clone,
                    &wallet_clone,
                    &rpc,
                    &tracker,
                    &position,
                    reason,
                    &event_tx,
                ).await {
                    Ok(sig) => {
                        println!("✅ Auto-sell executed: {} - {}", position.mint, sig);
                    }
                    Err(e) => {
                        eprintln!("❌ Auto-sell failed for {}: {}", position.mint, e);
                    }
                }
            }
        }

        // Wait before next check
        tokio::time::sleep(Duration::from_secs(monitor_interval)).await;
    }
}

/// Execute sell transaction for a position
async fn execute_sell(
    config: &Config,
    wallet: &Keypair,
    rpc: &RpcClient,
    tracker: &Arc<std::sync::RwLock<Option<TokenTracker>>>,
    position: &TokenBuy,
    reason: &str, // "stop_loss" or "take_profit"
    event_tx: &mpsc::UnboundedSender<TokenEvent>,
) -> Result<String> {
    let mint = Pubkey::from_str(&position.mint)?;
    let user_token_account = Pubkey::from_str(
        position.user_token_account.as_ref()
            .ok_or_else(|| anyhow!("User token account not found"))?
    )?;
    let bonding_curve = Pubkey::from_str(
        position.bonding_curve.as_ref()
            .ok_or_else(|| anyhow!("Bonding curve not found"))?
    )?;

    // Fetch token balance
    let account_data = rpc.get_account_data(&user_token_account).await?;
    let token_account = TokenAccount::unpack(&account_data)?;
    let token_balance = token_account.amount;

    if token_balance == 0 {
        return Err(anyhow!("Token balance is 0"));
    }

    // Calculate sell amount based on sell_percent
    let sell_amount = (token_balance as f64 * (config.sell_percent / 100.0)) as u64;
    if sell_amount == 0 {
        return Err(anyhow!("Sell amount is 0"));
    }

    // Reconstruct PumpBuyAccounts from position
    let associated_bonding_curve = get_associated_token_address(
        &bonding_curve,
        &mint,
    );
    
    let creator = Pubkey::from_str(&position.creator)?;
    
    let accounts = PumpBuyAccounts {
        mint,
        bonding_curve,
        associated_bonding_curve,
        creator_vault: creator, // Simplified - creator_vault is typically the creator
        event_authority: config.event_authority,
        global_volume: config.global_volume,
        global: config.global_account,
        fee_recipient: config.fee_recipient,
        fee_config: config.fee_config,
        fee_program: config.fee_program,
        dev_buy_sol: 0,
        creator,
        associated_bonding_curve_instruction: None,
    };

    let user_wallet = wallet.pubkey();

    // Build sell instruction
    let sell_ix = build_sell_instruction(
        &accounts,
        &user_wallet,
        &user_token_account,
        sell_amount,
    ).await?;

    // Build transaction
    let recent_blockhash = rpc.get_latest_blockhash().await?;
    let instructions = vec![
        ComputeBudgetInstruction::set_compute_unit_limit(config.compute_units),
        ComputeBudgetInstruction::set_compute_unit_price(config.priority_fee),
        sell_ix,
    ];

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

    // Send transaction using same submission mode as buy
    let tx_sig = match config.submission_mode {
        crate::config::SubmissionMode::Helius => {
            send_helius_transaction(tx).await?
        }
        crate::config::SubmissionMode::Jito => {
            let bundle_id = send_jito_bundle(tx, wallet, recent_blockhash, config.jito_tip).await?;
            return Ok(format!("Jito: {}", bundle_id));
        }
        crate::config::SubmissionMode::Rpc => {
            rpc.send_transaction(&tx).await?.to_string()
        }
        crate::config::SubmissionMode::All => {
            // Try all methods
            let tx_helius = tx.clone();
            let tx_jito = tx.clone();
            let tx_rpc = tx.clone();
            let wallet_bytes = wallet.to_bytes();
            let wallet_clone = Keypair::from_bytes(&wallet_bytes)?;
            let jito_tip = config.jito_tip;
            let rpc_url = config.rpc_url.clone();

            let helius_task = tokio::spawn(async move {
                send_helius_transaction(tx_helius).await
            });
            let jito_task = tokio::spawn(async move {
                send_jito_bundle(tx_jito, &wallet_clone, recent_blockhash, jito_tip).await
            });
            let rpc_task = tokio::spawn(async move {
                let rpc_client = RpcClient::new(rpc_url);
                rpc_client.send_transaction(&tx_rpc).await
            });

            tokio::select! {
                res = helius_task => {
                    match res {
                        Ok(Ok(sig)) => sig,
                        _ => return Err(anyhow!("All submission methods failed")),
                    }
                }
                res = jito_task => {
                    match res {
                        Ok(Ok(bundle_id)) => return Ok(format!("Jito: {}", bundle_id)),
                        _ => return Err(anyhow!("All submission methods failed")),
                    }
                }
                res = rpc_task => {
                    match res {
                        Ok(Ok(sig)) => sig.to_string(),
                        _ => return Err(anyhow!("All submission methods failed")),
                    }
                }
            }
        }
    };

    let signature = tx_sig.to_string();

    // Calculate PnL (simplified - would need current token price)
    // For now, we'll set PnL to None as we'd need to fetch the actual SOL received from the sell
    let pnl: Option<f64> = None;

    // Update tracker
    if let Ok(mut tracker_opt) = tracker.write() {
        if let Some(tracker) = tracker_opt.as_mut() {
            if let Err(e) = tracker.mark_as_sold(&position.mint, signature.clone()) {
                eprintln!("Failed to mark position as sold: {}", e);
            }
        }
    }

    // Send event
    let _ = event_tx.send(TokenEvent::Sold {
        mint: position.mint.clone(),
        signature: signature.clone(),
        reason: reason.to_string(),
        pnl,
        timestamp: Utc::now(),
    });

    Ok(signature)
}

