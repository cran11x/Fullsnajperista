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
    get_associated_token_address_with_program_id,
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
use std::time::{Duration, Instant};
use std::sync::Arc;
use std::collections::HashMap;
use tokio::sync::mpsc;

use crate::detection::PumpBuyAccounts;
use crate::websocket::{is_initialize_bonding_curve, extract_signature};
use crate::buy::build_buy_instruction;
use crate::jito::send_jito_bundle;
use crate::helius::send_helius_transaction;
use crate::socials::{check_token_socials, Socials};
use crate::das_check::check_creator_token_count_das;
use crate::filters::check_creator_token_count;
use crate::accounts::{TokenBuy, TokenTracker, SeenTokens, fetch_bonding_curve_mc, BondingCurveAccount};
use crate::config::Config;
use crate::constants::PUMP_PROGRAM_ID;
use crate::metrics::{SharedMetrics, FilterReason, SubmissionMethod, ErrorType};
use crate::gui::{TokenEvent, BotControl};
use crate::sell::build_sell_instruction;
use crate::token_logger::{TokenLogger, SocialsInfo};
// use crate::blockhash_cache::get_cached_blockhash;
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
    
    // Create token logger
    let logger = Arc::new(std::sync::Mutex::new(
        crate::token_logger::create_logger()
            .unwrap_or_else(|e| {
                eprintln!("⚠️  Failed to create token logger: {}", e);
                // Create a dummy logger that does nothing
                crate::token_logger::TokenLogger::new("/dev/null").unwrap()
            })
    ));
    
    // Start position monitor as background task
    let monitor_config = config.clone();
    let monitor_wallet_bytes = wallet.to_bytes();
    let monitor_wallet = Keypair::from_bytes(&monitor_wallet_bytes)?;
    let monitor_rpc = initial_config.create_rpc_client();
    let monitor_tracker = tracker.clone();
    let monitor_event_tx = event_tx.clone();
    
    tokio::spawn(async move {
        eprintln!("🚀 Starting auto-sell position monitor...");
        monitor_positions(
            monitor_config,
            monitor_wallet,
            monitor_rpc,
            monitor_tracker,
            monitor_event_tx,
        ).await;
        eprintln!("⚠️  WARNING: Auto-sell monitor loop exited unexpectedly!");
    });
    
    // Start ultra-fast PnL monitor as separate background task
    let pnl_config = config.clone();
    let pnl_rpc = initial_config.create_rpc_client();
    let pnl_tracker = tracker.clone();
    
    tokio::spawn(async move {
        monitor_pnl_ultra_fast(
            pnl_config,
            pnl_rpc,
            pnl_tracker,
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
                BotControl::ManualSell(_) => {
                    // Ignore manual sell before WebSocket connection is established
                }
                BotControl::ManualBuy { .. } => {
                    // Ignore manual buy before WebSocket connection is established
                }
            }
        }
        
        // Get current config
        let current_config = {
            let cfg = config.read().unwrap();
            (*cfg).clone()
        };
        
        reconnect_count += 1;
        
        // Exponential backoff: 0s, 1s, 2s, 4s, 8s, 16s, max 60s
        let reconnect_delay = if reconnect_count == 1 {
            Duration::from_secs(0)
        } else {
            let delay_secs = (1u64 << (reconnect_count - 2)).min(60);
            Duration::from_secs(delay_secs)
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
            logger.clone(),
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
    logger: Arc<std::sync::Mutex<TokenLogger>>,
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
    let mut last_health_check = Instant::now();
    const HEALTH_CHECK_INTERVAL_SECS: u64 = 30; // Check every 30 seconds
    const MAX_SILENCE_SECS: u64 = 60; // Reconnect if no messages for 60 seconds
    
    let mut stop_buying = false; // Flag for One Shot Mode to stop buying but keep monitoring

    loop {
        // Periodic health check - verify we're still receiving messages
        if last_health_check.elapsed() > Duration::from_secs(HEALTH_CHECK_INTERVAL_SECS) {
            let mut monitor = health_monitor.lock().unwrap();
            if !monitor.check_message_activity(MAX_SILENCE_SECS) {
                let _ = event_tx.send(TokenEvent::Error {
                    message: format!("WebSocket connection appears dead (no messages for {}s), reconnecting...", MAX_SILENCE_SECS),
                    timestamp: Utc::now(),
                });
                return Err(anyhow!("WebSocket connection unhealthy - no messages received"));
            }
            last_health_check = Instant::now();
        }
        
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
                    match control {
                        BotControl::Stop => {
                            let _ = event_tx.send(TokenEvent::Info {
                                message: "Bot stopped by user (in WebSocket loop)".to_string(),
                                timestamp: Utc::now(),
                            });
                            return Ok(true); // Signal that we stopped
                        }
                        BotControl::ManualSell(mint_to_sell) => {
                            eprintln!("╔═══════════════════════════════════════════════════════════════╗");
                            eprintln!("║              MANUAL SELL REQUESTED                           ║");
                            eprintln!("╚═══════════════════════════════════════════════════════════════╝");
                            eprintln!("📩 Received ManualSell command for {}", mint_to_sell);
                            let _ = event_tx.send(TokenEvent::Info {
                                message: format!("🚨 Manual sell requested for {}", mint_to_sell),
                                timestamp: Utc::now(),
                            });
                            
                            // Find position
                            eprintln!("   🔍 Searching for position...");
                            let position_opt = if let Ok(tracker_guard) = tracker.read() {
                                if let Some(tracker_ref) = tracker_guard.as_ref() {
                                    let positions = tracker_ref.get_active_positions();
                                    eprintln!("      - Found {} active positions", positions.len());
                                    positions.into_iter()
                                        .find(|p| p.mint == mint_to_sell)
                                } else {
                                    eprintln!("      - ⚠️  Tracker is None");
                                    None
                                }
                            } else {
                                eprintln!("      - ❌ Failed to acquire tracker read lock");
                                None
                            };
                            
                            if let Some(position) = position_opt {
                                eprintln!("   ✅ Position found:");
                                eprintln!("      - Mint: {}", position.mint);
                                eprintln!("      - Creator: {}", position.creator);
                                eprintln!("      - Buy signature: {}", position.signature);
                                eprintln!("      - Buy SOL: {}", position.our_buy_sol);
                                
                                // Clone resources for sell execution
                                eprintln!("   🔧 Preparing for sell execution...");
                                let wallet_bytes = wallet.to_bytes();
                                let wallet_clone = match Keypair::from_bytes(&wallet_bytes) {
                                    Ok(kp) => {
                                        eprintln!("      - ✅ Wallet cloned successfully");
                                        kp
                                    },
                                    Err(e) => {
                                        eprintln!("      - ❌ Failed to clone wallet for manual sell: {}", e);
                                        continue;
                                    }
                                };
                                
                                let config_clone = {
                                    let cfg = config_arc.read().unwrap();
                                    (*cfg).clone()
                                };
                                eprintln!("      - ✅ Config cloned");
                                eprintln!("         - Sell percent: {}%", config_clone.sell_percent);
                                eprintln!("         - Submission mode: {:?}", config_clone.submission_mode);
                                
                                let rpc_client = config_clone.create_rpc_client();
                                eprintln!("      - ✅ RPC client created");
                                
                                // Execute sell in background task to not block WebSocket loop
                                let tracker_clone = tracker.clone();
                                let event_tx_clone = event_tx.clone();
                                
                                eprintln!("   🚀 Spawning sell task for {}", position.mint);
                                tokio::spawn(async move {
                                    eprintln!("   🔄 Sell task started for {}", position.mint);
                                    match execute_sell(
                                        &config_clone,
                                        &wallet_clone,
                                        &rpc_client,
                                        &tracker_clone,
                                        &position,
                                        "manual_sell",
                                        &event_tx_clone,
                                    ).await {
                                        Ok(sig) => {
                                            eprintln!("   ✅ Manual sell executed successfully!");
                                            eprintln!("      - Signature: {}", sig);
                                            let _ = event_tx_clone.send(TokenEvent::Info {
                                                message: format!("✅ Manual sell executed: {}", sig),
                                                timestamp: Utc::now(),
                                            });
                                        }
                                        Err(e) => {
                                            eprintln!("   ❌ Manual sell failed!");
                                            eprintln!("      - Error: {}", e);
                                            eprintln!("      - Error details: {:?}", e);
                                            let _ = event_tx_clone.send(TokenEvent::Error {
                                                message: format!("❌ Manual sell failed: {}", e),
                                                timestamp: Utc::now(),
                                            });
                                        }
                                    }
                                });
                            } else {
                                eprintln!("   ❌ Position not found for manual sell: {}", mint_to_sell);
                                let _ = event_tx.send(TokenEvent::Error {
                                    message: format!("Position not found for manual sell: {}", mint_to_sell),
                                    timestamp: Utc::now(),
                                });
                            }
                        }
                        BotControl::ManualBuy { mint, sol_amount } => {
                            eprintln!("📩 Received ManualBuy command for {}", mint);
                            let _ = event_tx.send(TokenEvent::Info {
                                message: format!("🚀 Manual buy requested for {}", mint),
                                timestamp: Utc::now(),
                            });
                            
                            // Parse mint address
                            let mint_pubkey = match Pubkey::from_str(&mint) {
                                Ok(pk) => pk,
                                Err(e) => {
                                    let _ = event_tx.send(TokenEvent::Error {
                                        message: format!("❌ Invalid mint address: {}", e),
                                        timestamp: Utc::now(),
                                    });
                                    continue;
                                }
                            };
                            
                            // Clone resources for buy execution
                            let wallet_bytes = wallet.to_bytes();
                            let wallet_clone = match Keypair::from_bytes(&wallet_bytes) {
                                Ok(kp) => kp,
                                Err(e) => {
                                    eprintln!("Failed to clone wallet for manual buy: {}", e);
                                    let _ = event_tx.send(TokenEvent::Error {
                                        message: format!("❌ Failed to clone wallet: {}", e),
                                        timestamp: Utc::now(),
                                    });
                                    continue;
                                }
                            };
                            
                            let config_clone = {
                                let cfg = config_arc.read().unwrap();
                                (*cfg).clone()
                            };
                            
                            let rpc_client = config_clone.create_rpc_client();
                            
                            // Execute buy in background task to not block WebSocket loop
                            let tracker_clone = tracker.clone();
                            let event_tx_clone = event_tx.clone();
                            let metrics_clone = metrics.clone();
                            let sol_amount_clone = sol_amount;
                            
                            eprintln!("🚀 Spawning buy task for {}", mint);
                            tokio::spawn(async move {
                                eprintln!("🔄 Buy task started for {}", mint);
                                
                                // Create PumpBuyAccounts from mint address
                                let accounts = match PumpBuyAccounts::from_mint_address(&rpc_client, &mint_pubkey).await {
                                    Ok(acc) => acc,
                                    Err(e) => {
                                        let _ = event_tx_clone.send(TokenEvent::Error {
                                            message: format!("❌ Failed to create accounts from mint: {}", e),
                                            timestamp: Utc::now(),
                                        });
                                        return;
                                    }
                                };
                                
                                // Override buy amount if specified
                                let buy_amount = sol_amount_clone.unwrap_or(config_clone.buy_amount_lamports());
                                
                                // Execute buy using existing logic
                                match execute_manual_buy(
                                    &config_clone,
                                    &wallet_clone,
                                    &rpc_client,
                                    &tracker_clone,
                                    accounts,
                                    buy_amount,
                                    &metrics_clone,
                                    &event_tx_clone,
                                ).await {
                                    Ok(sig) => {
                                        let _ = event_tx_clone.send(TokenEvent::Bought {
                                            mint: mint.clone(),
                                            signature: sig,
                                            mc: None,
                                            timestamp: Utc::now(),
                                        });
                                    }
                                    Err(e) => {
                                        let _ = event_tx_clone.send(TokenEvent::Error {
                                            message: format!("❌ Manual buy failed: {}", e),
                                            timestamp: Utc::now(),
                                        });
                                    }
                                }
                            });
                        }
                        _ => {} // Ignore other messages like Start/UpdateConfig which are handled by GUI
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
        
        // Record message received for health monitoring
        {
            let mut monitor = health_monitor.lock().unwrap();
            monitor.record_message_received();
        }
        
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
            
            // If we are in stop_buying mode (One Shot triggered), skip processing new tokens
            if stop_buying {
                if message_count % 50 == 0 {
                    println!("      💤 Monitor-only mode (One Shot active) - skipping new token");
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
                    let reason = format!("Not target token (waiting for: {})", target_mint);
                    let _ = event_tx.send(TokenEvent::Filtered {
                        mint: mint.clone(),
                        reason: reason.clone(),
                        timestamp: Utc::now(),
                    });
                    // Log filtered token
                    if let Ok(logger_guard) = logger.lock() {
                        let _ = logger_guard.log_filtered(
                            mint.clone(),
                            reason,
                            Some(init_signature.clone()),
                            Some(accounts.dev_buy_sol as f64 / 1e9),
                            Some(accounts.creator.to_string()),
                            None,
                            None,
                        );
                    }
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
                let reason = "Duplicate token".to_string();
                let _ = event_tx.send(TokenEvent::Filtered {
                    mint: mint.clone(),
                    reason: reason.clone(),
                    timestamp: Utc::now(),
                });
                // Log filtered token
                if let Ok(logger_guard) = logger.lock() {
                    let _ = logger_guard.log_filtered(
                        mint.clone(),
                        reason,
                        Some(init_signature.clone()),
                        Some(accounts.dev_buy_sol as f64 / 1e9),
                        Some(accounts.creator.to_string()),
                        None,
                        None,
                    );
                }
                continue;
            }
            
            // Blacklist/Whitelist check
            {
                let config_guard = config_arc.read().unwrap();
                
                // Check if token is blacklisted
                if config_guard.blacklisted_tokens.contains(&mint_pubkey) {
                    let reason = "Token is blacklisted".to_string();
                    let _ = event_tx.send(TokenEvent::Filtered {
                        mint: mint.clone(),
                        reason: reason.clone(),
                        timestamp: Utc::now(),
                    });
                    // Log filtered token
                    if let Ok(logger_guard) = logger.lock() {
                        let _ = logger_guard.log_filtered(
                            mint.clone(),
                            reason,
                            Some(init_signature.clone()),
                            Some(accounts.dev_buy_sol as f64 / 1e9),
                            Some(accounts.creator.to_string()),
                            None,
                            None,
                        );
                    }
                    continue;
                }
                
                // Check if creator is blacklisted
                if config_guard.blacklisted_creators.contains(&accounts.creator) {
                    let reason = format!("Creator {} is blacklisted", accounts.creator);
                    let _ = event_tx.send(TokenEvent::Filtered {
                        mint: mint.clone(),
                        reason: reason.clone(),
                        timestamp: Utc::now(),
                    });
                    // Log filtered token
                    if let Ok(logger_guard) = logger.lock() {
                        let _ = logger_guard.log_filtered(
                            mint.clone(),
                            reason,
                            Some(init_signature.clone()),
                            Some(accounts.dev_buy_sol as f64 / 1e9),
                            Some(accounts.creator.to_string()),
                            None,
                            None,
                        );
                    }
                    continue;
                }
                
                // Check whitelist (if set, token must be on whitelist)
                if let Some(ref whitelist) = config_guard.whitelisted_tokens {
                    if !whitelist.contains(&mint_pubkey) {
                        let reason = "Token not on whitelist".to_string();
                        let _ = event_tx.send(TokenEvent::Filtered {
                            mint: mint.clone(),
                            reason: reason.clone(),
                            timestamp: Utc::now(),
                        });
                        // Log filtered token
                        if let Ok(logger_guard) = logger.lock() {
                            let _ = logger_guard.log_filtered(
                                mint.clone(),
                                reason,
                                Some(init_signature.clone()),
                                Some(accounts.dev_buy_sol as f64 / 1e9),
                                Some(accounts.creator.to_string()),
                                None,
                                None,
                            );
                        }
                        continue;
                    }
                }
            }
            
            let config_for_buy = {
                let cfg = config_arc.read().unwrap();
                (*cfg).clone()
            };
            
            // Save values before moving accounts
            let dev_buy_sol = accounts.dev_buy_sol as f64 / 1e9;
            let creator = accounts.creator.to_string();
            
            match process_and_buy(
                &config_for_buy,
                wallet,
                rpc,
                tracker,
                *detected,
                init_signature.clone(),
                accounts,
                metrics.clone(),
                das_rate_limiter.clone(),
                socials_rate_limiter.clone(),
                event_tx.clone(),
                logger.clone(),
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
                            mint: mint.clone(),
                            signature: signature.clone(),
                            mc,
                            timestamp: Utc::now(),
                        });
                        
                        // Log bought token
                        if let Ok(logger_guard) = logger.lock() {
                            let _ = logger_guard.log_bought(
                                mint.clone(),
                                signature.clone(),
                                Some(init_signature.clone()),
                                mc,
                                Some(dev_buy_sol),
                                Some(creator.clone()),
                            );
                        }
                        
                        // Check if one shot mode is enabled - stop bot after successful buy
                        if config.one_shot_mode {
                            eprintln!("🎯 One Shot Mode: Buy successful, entering monitor-only mode...");
                            let _ = event_tx.send(TokenEvent::Info {
                                message: "One Shot Mode: Buy successful, stopped buying new tokens".to_string(),
                                timestamp: Utc::now(),
                            });
                            // Instead of breaking, we set the flag to stop buying new tokens
                            // This keeps the connection open for manual sells and monitoring
                            stop_buying = true;
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
                        mint: mint.clone(),
                        reason: reason.clone(),
                        timestamp: Utc::now(),
                    });
                    
                    // Log filtered token (will be logged in process_and_buy with more details)
                    
                    // In one shot mode, also stop on errors to prevent wasting credits
                    // (but not on SKIP errors which are expected filter rejections)
                    if config.one_shot_mode && !reason.contains("SKIP") {
                        eprintln!("🎯 One Shot Mode: Processing error occurred, entering monitor-only mode");
                        let _ = event_tx.send(TokenEvent::Info {
                            message: "One Shot Mode: Error occurred, stopped buying new tokens".to_string(),
                            timestamp: Utc::now(),
                        });
                        stop_buying = true;
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
    logger: Arc<std::sync::Mutex<TokenLogger>>,
) -> Result<Option<String>> {
    let mint = accounts.mint;
    let dev_buy_lamports = accounts.dev_buy_sol;
    let dev_buy_sol = dev_buy_lamports as f64 / 1e9;
    
    // Debug: Log config values at start
    println!("      🔍 DEBUG PROCESS_AND_BUY START: mint={}, creator={}, min_dev_tokens={}, max_dev_tokens={}", 
             mint, accounts.creator, config.min_dev_tokens, config.max_dev_tokens);
    
    // ========================================================================
    // SECTION 1: TOKEN INFO
    // ========================================================================
    
    let min_sol = config.min_dev_buy_usd / config.sol_price_usd;
    let max_sol = config.max_dev_buy_usd / config.sol_price_usd;
    let filter_start = std::time::Instant::now();
    
    // ========================================================================
    // SECTION 2: FILTER CHECKS
    // ========================================================================
    
    // Filter #1: Dev Buy Amount
    if dev_buy_sol < min_sol || dev_buy_sol > max_sol {
        let filter_time = filter_start.elapsed().as_millis() as u64;
        {
            let mut m = metrics.write().unwrap();
            m.record_filter(FilterReason::DevBuy, filter_time);
        }
        let reason = format!("SKIP: Dev buy {:.2} SOL (want {:.2}-{:.2})", dev_buy_sol, min_sol, max_sol);
        // Log filtered token
        if let Ok(logger_guard) = logger.lock() {
            let _ = logger_guard.log_filtered(
                mint.to_string(),
                reason.clone(),
                Some(init_signature.clone()),
                Some(dev_buy_sol),
                Some(accounts.creator.to_string()),
                None,
                None,
            );
        }
        return Err(anyhow!(reason));
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
    
    let mc_fut: std::pin::Pin<Box<dyn std::future::Future<Output = Result<(BondingCurveAccount, f64, f64), anyhow::Error>> + Send>> = Box::pin(fetch_bonding_curve_mc(
        rpc,
        &accounts.bonding_curve,
        config.sol_price_usd,
    ));
    
    // ⚡ PARALLEL: Await all futures simultaneously using tokio::join!
    let (das_result, mc_result, socials_result): (
        Result<u32, anyhow::Error>,
        Result<(BondingCurveAccount, f64, f64), anyhow::Error>,
        Option<Socials>
    ) = tokio::join!(
        das_fut,
        mc_fut,
        socials_fut
    );
    
    // Filter #2: Creator Token Count (using filter function from filters.rs)
    println!("      🔍 DEBUG DEV TOKENS FILTER: min_dev_tokens={}, max_dev_tokens={}", 
             config.min_dev_tokens, config.max_dev_tokens);
    let creator_count = match das_result {
        Ok(count) => {
            println!("      🔍 DEBUG DEV TOKENS FILTER: DAS returned count={} for creator={}", 
                     count, accounts.creator);
            
            // ⚠️ NOTE: DAS API may return 0 for some creators even if they have many tokens.
            // Instead of skipping, we let it go through the filter. If min_dev_tokens = 0,
            // tokens with count=0 will pass. If min_dev_tokens > 0, they will be filtered out.
            // This allows good tokens (like 9taecBUD...) to pass even if DAS returns 0.
            
            // Use filter function from filters.rs for consistency
            if !check_creator_token_count(count, config) {
                let reason = if count < config.min_dev_tokens as u32 {
                    format!("SKIP: Creator has only {} tokens (min: {})", count, config.min_dev_tokens)
                } else {
                    format!("SKIP: Creator has {} tokens (max: {})", count, config.max_dev_tokens)
                };
                println!("      ❌ SKIP: Creator token count filter failed - {}", reason);
                let filter_time = filter_start.elapsed().as_millis() as u64;
                if let Ok(mut m) = metrics.write() {
                    m.record_filter(FilterReason::CreatorCount, filter_time);
                    m.record_error(ErrorType::Validation);
                }
                // Log filtered token
                if let Ok(logger_guard) = logger.lock() {
                    let _ = logger_guard.log_filtered(
                        mint.to_string(),
                        reason.clone(),
                        Some(init_signature.clone()),
                        Some(dev_buy_sol),
                        Some(accounts.creator.to_string()),
                        Some(count),
                        None,
                    );
                }
                return Err(anyhow!(reason));
            }
            println!("      ✅ DEBUG FILTER PASSED: count={} is within range [{}, {}]", 
                    count, config.min_dev_tokens, config.max_dev_tokens);
            count
        }
        Err(e) => {
            // DAS check failed - skip token since we can't verify creator token count
            let reason = format!("SKIP: DAS API failed - {}", e);
            println!("      ❌ SKIP: DAS API failed - cannot verify creator token count: {}", e);
            let filter_time = filter_start.elapsed().as_millis() as u64;
            if let Ok(mut m) = metrics.write() {
                m.record_filter(FilterReason::CreatorCount, filter_time);
                m.record_error(ErrorType::Network);
            }
            // Log filtered token
            if let Ok(logger_guard) = logger.lock() {
                let _ = logger_guard.log_filtered(
                    mint.to_string(),
                    reason.clone(),
                    Some(init_signature.clone()),
                    Some(dev_buy_sol),
                    Some(accounts.creator.to_string()),
                    None,
                    None,
                );
            }
            return Err(anyhow!(reason));
        }
    };
    
    println!("      🔍 DEBUG: After dev tokens filter, creator_count={}, continuing...", creator_count);
    
    // Process MC result
    let (curve, _mc_sol, mc_usd) = match mc_result {
        Ok(data) => data,
        Err(_) => {
            (BondingCurveAccount::default(), 0.0, 0.0)
        }
    };
    
    let token_price_sol = curve.get_token_price_sol();
    
    // Filter #3: Socials
    let socials_opt = if let Some(socials) = socials_result {
        if require_socials && !socials.has_any() {
            let filter_time = filter_start.elapsed().as_millis() as u64;
            if let Ok(mut m) = metrics.write() {
                m.record_filter(FilterReason::Socials, filter_time);
            }
            let reason = "SKIP: No socials".to_string();
            // Log filtered token
            if let Ok(logger_guard) = logger.lock() {
                let socials_info = Some(SocialsInfo {
                    twitter: socials.twitter.clone(),
                    telegram: socials.telegram.clone(),
                    website: socials.website.clone(),
                    count: socials.count(),
                });
                let _ = logger_guard.log_filtered(
                    mint.to_string(),
                    reason.clone(),
                    Some(init_signature.clone()),
                    Some(dev_buy_sol),
                    Some(accounts.creator.to_string()),
                    Some(creator_count),
                    socials_info,
                );
            }
            return Err(anyhow!(reason));
        }
        if require_twitter && !socials.has_twitter() {
            let filter_time = filter_start.elapsed().as_millis() as u64;
            if let Ok(mut m) = metrics.write() {
                m.record_filter(FilterReason::Socials, filter_time);
            }
            let reason = "SKIP: No Twitter".to_string();
            // Log filtered token
            if let Ok(logger_guard) = logger.lock() {
                let socials_info = Some(SocialsInfo {
                    twitter: socials.twitter.clone(),
                    telegram: socials.telegram.clone(),
                    website: socials.website.clone(),
                    count: socials.count(),
                });
                let _ = logger_guard.log_filtered(
                    mint.to_string(),
                    reason.clone(),
                    Some(init_signature.clone()),
                    Some(dev_buy_sol),
                    Some(accounts.creator.to_string()),
                    Some(creator_count),
                    socials_info,
                );
            }
            return Err(anyhow!(reason));
        }
        if socials.count() < min_socials {
            let filter_time = filter_start.elapsed().as_millis() as u64;
            if let Ok(mut m) = metrics.write() {
                m.record_filter(FilterReason::Socials, filter_time);
            }
            let reason = format!("SKIP: Need {} socials", min_socials);
            // Log filtered token
            if let Ok(logger_guard) = logger.lock() {
                let socials_info = Some(SocialsInfo {
                    twitter: socials.twitter.clone(),
                    telegram: socials.telegram.clone(),
                    website: socials.website.clone(),
                    count: socials.count(),
                });
                let _ = logger_guard.log_filtered(
                    mint.to_string(),
                    reason.clone(),
                    Some(init_signature.clone()),
                    Some(dev_buy_sol),
                    Some(accounts.creator.to_string()),
                    Some(creator_count),
                    socials_info,
                );
            }
            return Err(anyhow!(reason));
        }
        Some(socials)
    } else {
        if require_socials {
            let filter_time = filter_start.elapsed().as_millis() as u64;
            if let Ok(mut m) = metrics.write() {
                m.record_filter(FilterReason::Socials, filter_time);
                m.record_error(ErrorType::Network);
            }
            let reason = "SKIP: Could not verify socials".to_string();
            // Log filtered token
            if let Ok(logger_guard) = logger.lock() {
                let _ = logger_guard.log_filtered(
                    mint.to_string(),
                    reason.clone(),
                    Some(init_signature.clone()),
                    Some(dev_buy_sol),
                    Some(accounts.creator.to_string()),
                    Some(creator_count),
                    None,
                );
            }
            return Err(anyhow!(reason));
        }
        None
    };
    
    // ========================================================================
    // SECTION 3: ACCOUNT VERIFICATION
    // ========================================================================
    
    // Verify bonding curve account
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
                            let reason = "SKIP: Bonding curve account not found - token not ready".to_string();
                            // Log filtered token
                            if let Ok(logger_guard) = logger.lock() {
                                let socials_info = socials_opt.as_ref().map(|s| SocialsInfo {
                                    twitter: s.twitter.clone(),
                                    telegram: s.telegram.clone(),
                                    website: s.website.clone(),
                                    count: s.count(),
                                });
                                let _ = logger_guard.log_filtered(
                                    mint.to_string(),
                                    reason.clone(),
                                    Some(init_signature.clone()),
                                    Some(dev_buy_sol),
                                    Some(accounts.creator.to_string()),
                                    Some(creator_count),
                                    socials_info,
                                );
                            }
                            return Err(anyhow!(reason));
                        }
                    }
                };
                
                if account.data.is_empty() {
                    if attempt < max_wait_attempts {
                        tokio::time::sleep(Duration::from_millis(wait_interval_ms)).await;
                        continue;
                    } else {
                        let reason = "SKIP: Bonding curve account not ready for trading".to_string();
                        // Log filtered token
                        if let Ok(logger_guard) = logger.lock() {
                            let socials_info = socials_opt.as_ref().map(|s| SocialsInfo {
                                twitter: s.twitter.clone(),
                                telegram: s.telegram.clone(),
                                website: s.website.clone(),
                                count: s.count(),
                            });
                            let _ = logger_guard.log_filtered(
                                mint.to_string(),
                                reason.clone(),
                                Some(init_signature.clone()),
                                Some(dev_buy_sol),
                                Some(accounts.creator.to_string()),
                                Some(creator_count),
                                socials_info,
                            );
                        }
                        return Err(anyhow!(reason));
                    }
                }
                
                // Check if bonding curve is complete (token migrated - can't buy anymore)
                if !account.data.is_empty() {
                    use borsh::BorshDeserialize;
                    if let Ok(curve) = BondingCurveAccount::try_from_slice(&account.data[..]) {
                        if curve.complete {
                            let reason = "SKIP: Token is complete (migrated) - cannot buy on bonding curve".to_string();
                            // Log filtered token
                            if let Ok(logger_guard) = logger.lock() {
                                let socials_info = socials_opt.as_ref().map(|s| SocialsInfo {
                                    twitter: s.twitter.clone(),
                                    telegram: s.telegram.clone(),
                                    website: s.website.clone(),
                                    count: s.count(),
                                });
                                let _ = logger_guard.log_filtered(
                                    mint.to_string(),
                                    reason.clone(),
                                    Some(init_signature.clone()),
                                    Some(dev_buy_sol),
                                    Some(accounts.creator.to_string()),
                                    Some(creator_count),
                                    socials_info,
                                );
                            }
                            return Err(anyhow!(reason));
                        }
                    }
                }
                
                bonding_curve_ready = true;
                break;
            }
            Err(_) => {
                if attempt < max_wait_attempts {
                    tokio::time::sleep(Duration::from_millis(wait_interval_ms)).await;
                    continue;
                } else {
                    let reason = "SKIP: Bonding curve account not found - token not ready".to_string();
                    // Log filtered token
                    if let Ok(logger_guard) = logger.lock() {
                        let socials_info = socials_opt.as_ref().map(|s| SocialsInfo {
                            twitter: s.twitter.clone(),
                            telegram: s.telegram.clone(),
                            website: s.website.clone(),
                            count: s.count(),
                        });
                        let _ = logger_guard.log_filtered(
                            mint.to_string(),
                            reason.clone(),
                            Some(init_signature.clone()),
                            Some(dev_buy_sol),
                            Some(accounts.creator.to_string()),
                            Some(creator_count),
                            socials_info,
                        );
                    }
                    return Err(anyhow!(reason));
                }
            }
        }
    }
    
    if !bonding_curve_ready {
        let reason = "SKIP: Bonding curve account not ready".to_string();
        // Log filtered token
        if let Ok(logger_guard) = logger.lock() {
            let socials_info = socials_opt.as_ref().map(|s| SocialsInfo {
                twitter: s.twitter.clone(),
                telegram: s.telegram.clone(),
                website: s.website.clone(),
                count: s.count(),
            });
            let _ = logger_guard.log_filtered(
                mint.to_string(),
                reason.clone(),
                Some(init_signature.clone()),
                Some(dev_buy_sol),
                Some(accounts.creator.to_string()),
                Some(creator_count),
                socials_info,
            );
        }
        return Err(anyhow!(reason));
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
    
    let buy_ix = build_buy_instruction(
        rpc,
        &accounts,
        &user_wallet,
        &user_ata,
        config.buy_amount_lamports(),
        config.slippage_percent,
        Some(&curve), // Use current bonding curve price for accurate token amount
    ).await?;
    
    let mut rng = rand::thread_rng();
    use crate::constants::HELIUS_TIP_ACCOUNTS;
    let tip_account = Pubkey::from_str(
        HELIUS_TIP_ACCOUNTS.choose(&mut rng).unwrap()
    )?;
    
    // Check if ATA already exists (could be optimized with batch call if checking multiple accounts)
    let ata_exists = rpc.get_account(&user_ata).await.is_ok();
    
    // Calculate priority fee (dynamic or static)
    let priority_fee = if config.enable_dynamic_priority_fee {
        match config.calculate_dynamic_priority_fee(rpc).await {
            Ok(fee) => fee,
            Err(_e) => config.priority_fee
        }
    } else {
        config.priority_fee
    };
    
    let mut instructions = vec![
        ComputeBudgetInstruction::set_compute_unit_limit(config.compute_units),
        ComputeBudgetInstruction::set_compute_unit_price(priority_fee),
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
            let reason = "SKIP: Associated Bonding Curve account not initialized - token may not be ready".to_string();
            // Log filtered token
            if let Ok(logger_guard) = logger.lock() {
                let socials_info = socials_opt.as_ref().map(|s| SocialsInfo {
                    twitter: s.twitter.clone(),
                    telegram: s.telegram.clone(),
                    website: s.website.clone(),
                    count: s.count(),
                });
                let _ = logger_guard.log_filtered(
                    mint.to_string(),
                    reason.clone(),
                    Some(init_signature.clone()),
                    Some(dev_buy_sol),
                    Some(accounts.creator.to_string()),
                    Some(creator_count),
                    socials_info,
                );
            }
            return Err(anyhow!(reason));
        }
        
        if owner != token_program_2022 && owner != token_program {
            let reason = "SKIP: Associated Bonding Curve has wrong owner - token may not be ready".to_string();
            // Log filtered token
            if let Ok(logger_guard) = logger.lock() {
                let socials_info = socials_opt.as_ref().map(|s| SocialsInfo {
                    twitter: s.twitter.clone(),
                    telegram: s.telegram.clone(),
                    website: s.website.clone(),
                    count: s.count(),
                });
                let _ = logger_guard.log_filtered(
                    mint.to_string(),
                    reason.clone(),
                    Some(init_signature.clone()),
                    Some(dev_buy_sol),
                    Some(accounts.creator.to_string()),
                    Some(creator_count),
                    socials_info,
                );
            }
            return Err(anyhow!(reason));
        }
    } else {
        let reason = "SKIP: Associated Bonding Curve account does not exist - token not ready".to_string();
        // Log filtered token
        if let Ok(logger_guard) = logger.lock() {
            let socials_info = socials_opt.as_ref().map(|s| SocialsInfo {
                twitter: s.twitter.clone(),
                telegram: s.telegram.clone(),
                website: s.website.clone(),
                count: s.count(),
            });
            let _ = logger_guard.log_filtered(
                mint.to_string(),
                reason.clone(),
                Some(init_signature.clone()),
                Some(dev_buy_sol),
                Some(accounts.creator.to_string()),
                Some(creator_count),
                socials_info,
            );
        }
        return Err(anyhow!(reason));
    }
    
    // Verify User Token Account
    let _user_ata_account = rpc.get_account(&user_ata).await;
    
    // ========================================================================
    // SECTION 5: PDA VERIFICATION
    // ========================================================================
    
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
        (13, "User Volume", expected_user_volume),  // Index 13 (after Pump Program at 11, Global Volume at 12)
    ];
    
    for (idx, _name, expected) in checks {
        if idx < buy_ix.accounts.len() {
            let actual = buy_ix.accounts[idx].pubkey;
            if actual != expected {
                pda_mismatches.push((idx, _name, actual, expected));
            }
        }
    }
    
    if !pda_mismatches.is_empty() {
        return Err(anyhow!("CRITICAL: {} PDA mismatch(es) detected! This will cause Error 2006.", pda_mismatches.len()));
    }
    
    // ========================================================================
    // SECTION 6: BUILD TRANSACTION
    // ========================================================================
    
    instructions.push(buy_ix);
    
    instructions.push(system_instruction::transfer(
        &user_wallet,
        &tip_account,
        config.jito_tip,
    ));
    
    let recent_blockhash = rpc.get_latest_blockhash().await?;
    
    // ========================================================================
    // SECTION 7: SEND TRANSACTION
    // ========================================================================
    
    if config.mock_buy {
        
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
                            current_price_sol: None,
                            current_value_sol: None,
                            pnl_sol: None,
                            pnl_percent: None,
                            last_pnl_update: None,
                            buy_fees_sol: Some(0.0),
                        };
                        
                        let _ = tracker.record_buy(buy);
                    }
                    None => {}
                }
            }
            Err(_e) => {}
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
        priority_fee,
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
    let (helius_res, jito_res, rpc_res) = tokio::join!(helius_task, jito_task, rpc_task);
    
    // Process results and find the first successful one
    let mut results = Vec::new();
    
    // Process Helius result
    let submission_time = submission_start.elapsed().as_millis() as u64;
    match helius_res {
        Ok(Ok(sig)) => {
            let mut m = metrics.write().unwrap();
            m.record_submission(SubmissionMethod::Helius, true, submission_time);
            results.push((Ok(()), SubmissionMethod::Helius, Some(sig)));
        }
        Ok(Err(e)) => {
            let mut m = metrics.write().unwrap();
            m.record_submission(SubmissionMethod::Helius, false, submission_time);
            m.record_error(ErrorType::Submission);
            results.push((Err(anyhow!("Helius failed: {}", e)), SubmissionMethod::Helius, None));
        }
        Err(e) => {
            let mut m = metrics.write().unwrap();
            m.record_submission(SubmissionMethod::Helius, false, submission_time);
            m.record_error(ErrorType::Submission);
            results.push((Err(anyhow!("Helius task error: {}", e)), SubmissionMethod::Helius, None));
        }
    }
    
    // Process Jito result
    match jito_res {
        Ok(Ok(bundle_id)) => {
            let mut m = metrics.write().unwrap();
            m.record_submission(SubmissionMethod::Jito, true, submission_time);
            results.push((Ok(()), SubmissionMethod::Jito, Some(bundle_id)));
        }
        Ok(Err(e)) => {
            let mut m = metrics.write().unwrap();
            m.record_submission(SubmissionMethod::Jito, false, submission_time);
            m.record_error(ErrorType::Submission);
            results.push((Err(anyhow!("Jito failed: {}", e)), SubmissionMethod::Jito, None));
        }
        Err(e) => {
            let mut m = metrics.write().unwrap();
            m.record_submission(SubmissionMethod::Jito, false, submission_time);
            m.record_error(ErrorType::Submission);
            results.push((Err(anyhow!("Jito task error: {}", e)), SubmissionMethod::Jito, None));
        }
    }
    
    // Process RPC result
    match rpc_res {
        Ok(Ok(sig)) => {
            let mut m = metrics.write().unwrap();
            m.record_submission(SubmissionMethod::Rpc, true, submission_time);
            results.push((Ok(()), SubmissionMethod::Rpc, Some(sig)));
        }
        Ok(Err(e)) => {
            let mut m = metrics.write().unwrap();
            m.record_submission(SubmissionMethod::Rpc, false, submission_time);
            m.record_error(ErrorType::Rpc);
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
    let (buy_succeeded, failure_reason) = if submission_result.is_ok() {
        if let Some(sig_str) = actual_signature.as_ref() {
            let sig_str: &String = sig_str;
            if sig_str.starts_with("Jito:") {
                eprintln!("  Verification: Jito bundle (cannot verify immediately)");
                (true, None)
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
                    Ok(None) => (true, None),
                    Ok(Some(reason)) => (false, Some(reason)),
                    Err(_e) => (true, None)
                }
            }
        } else {
            (false, Some("No signature returned".to_string()))
        }
    } else {
        (false, None)
    };
    
    if buy_succeeded {
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
        
        // Calculate total fees for PnL accuracy
        let priority_fee_sol = priority_fee as f64 / 1e9;
        let jito_tip_sol = config.jito_tip as f64 / 1e9;
        let base_fee_sol = 0.000005; // 5000 lamports base fee
        let total_buy_fees = priority_fee_sol + jito_tip_sol + base_fee_sol;

        // NEW: Fetch actual balance change for precise entry cost
        let mut invested_sol = config.buy_amount_sol;
        
        let buy_signature = actual_signature.as_ref()
            .map(|s| s.clone())
            .unwrap_or_else(|| init_signature.clone());
        
        // Only try to fetch if we have a real signature (not MOCK)
        if !buy_signature.starts_with("MOCK") {
            // Add small delay to ensure transaction is indexed
            tokio::time::sleep(Duration::from_millis(500)).await;
            
            match crate::utils::get_transaction_balance_change(rpc, &buy_signature, &user_wallet).await {
                    Ok(change_lamports) => {
                        let change_sol = change_lamports as f64 / 1e9;
                        // Balance change should be negative for a buy (spending SOL)
                        if change_sol < 0.0 {
                            let total_cost = -change_sol;
                            let actual_invested = total_cost - total_buy_fees;
                            
                            // Sanity check: if actual_invested is close to 0 or negative, something is wrong
                            if actual_invested > 0.001 {
                                invested_sol = actual_invested;
                            }
                        }
                    },
                    Err(_e) => {}
            }
        }
        
        if let Ok(mut tracker_opt) = tracker.write() {
            if let Some(tracker) = tracker_opt.as_mut() {
                
                let buy = TokenBuy {
                    token_number,
                    mint: mint.to_string(),
                    signature: buy_signature.clone(),
                    creator: accounts.creator.to_string(),
                    dev_buy_sol,
                    our_buy_sol: invested_sol,
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
                    current_price_sol: None,
                    current_value_sol: None,
                    pnl_sol: None,
                    pnl_percent: None,
                    last_pnl_update: None,
                    buy_fees_sol: Some(total_buy_fees),
                };
                
                let _ = tracker.record_buy(buy.clone());
            }
        }
        
        let final_signature = actual_signature.as_ref().map(|s| s.clone()).unwrap_or_else(|| init_signature.clone());
        Ok(Some(final_signature))
    } else {
        
        match submission_result {
            Ok(_) => {
                let reason = failure_reason.unwrap_or_else(|| "Unknown verification failure".to_string());
                Err(anyhow!("Transaction submission succeeded but buy verification failed: {}", reason))
            },
            Err(e) => Err(e),
        }
    }
}

/// Verify that a transaction was successfully executed and buy instruction succeeded
async fn verify_transaction_success(
    rpc: &RpcClient,
    signature: &solana_sdk::signature::Signature,
    expected_token_account: &Pubkey,
) -> Result<Option<String>> {
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
                    
                    // Provide helpful error messages for common errors
                    let error_msg = format!("{:?}", err);
                    let detailed_msg = if error_msg.contains("Custom(1)") {
                        format!("Transaction on-chain error: {:?} - This usually means:\n   • Token is complete (migrated) and cannot be bought on bonding curve\n   • Token is not ready for trading yet\n   • Slippage too high or insufficient SOL", err)
                    } else if error_msg.contains("Custom(0)") {
                        format!("Transaction on-chain error: {:?} - Insufficient SOL or token reserves", err)
                    } else {
                        format!("Transaction on-chain error: {:?}", err)
                    };
                    
                    // Log inner instructions if available for debugging
                    eprintln!("   Transaction failed, checking if ATA creation was the issue...");
                    // Note: inner_instructions is OptionSerializer type, skip detailed logging for now
                    
                    return Ok(Some(detailed_msg));
                }
                
                // Check if token account was created (indicates buy succeeded)
                // We can check if the expected token account exists
                if let Ok(account) = rpc.get_account(expected_token_account).await {
                    // Token account exists - buy likely succeeded
                    // But we should also check the token balance
                    if let Ok(token_account_data) = spl_token::state::Account::unpack(&account.data) {
                        if token_account_data.amount > 0 {
                            return Ok(None);
                        }
                    }
                }
                
                // Transaction succeeded but we can't verify token account
                // Assume success if transaction didn't error
                Ok(None)
            } else {
                // No metadata - can't verify
                Ok(Some("No transaction metadata found".to_string()))
            }
        }
        Err(e) => {
            // Transaction not found or error getting it
            Err(anyhow!("Could not get transaction: {}", e))
        }
    }
}

/// Monitor active positions and trigger sells when conditions are met
/// Helper function to get token balance using Helius API (faster and more reliable)
async fn get_token_balance_helius(
    helius_api_key: &str,
    token_account: &Pubkey,
) -> Result<u64> {
    let url = format!("https://mainnet.helius-rpc.com/?api-key={}", helius_api_key);
    let client = crate::utils::get_shared_http_client();
    
    let request_body = serde_json::json!({
        "jsonrpc": "2.0",
        "id": "1",
        "method": "getTokenAccountBalance",
        "params": [token_account.to_string()]
    });
    
    let response = client
        .post(&url)
        .json(&request_body)
        .send()
        .await
        .map_err(|e| anyhow!("Helius API request failed: {}", e))?;
    
    let result: serde_json::Value = response.json().await
        .map_err(|e| anyhow!("Failed to parse Helius response: {}", e))?;
    
    if let Some(balance_str) = result["result"]["value"]["amount"].as_str() {
        balance_str.parse::<u64>()
            .map_err(|e| anyhow!("Failed to parse balance: {}", e))
    } else {
        Err(anyhow!("Helius API returned no balance"))
    }
}

/// Get all token holdings for a wallet using Helius API
pub async fn get_wallet_token_holdings(
    helius_api_key: &str,
    wallet_address: &Pubkey,
) -> Result<Vec<(Pubkey, Pubkey, u64)>> {
    // Returns Vec<(mint, token_account, balance)>
    let url = format!("https://mainnet.helius-rpc.com/?api-key={}", helius_api_key);
    let client = crate::utils::get_shared_http_client();
    
    eprintln!("🔍 Fetching token holdings for wallet: {}", wallet_address);
    
    // Use getTokenAccountsByOwner to get all token accounts
    let request_body = serde_json::json!({
        "jsonrpc": "2.0",
        "id": "1",
        "method": "getTokenAccountsByOwner",
        "params": [
            wallet_address.to_string(),
            {
                "programId": "TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb" // Token 2022
            },
            {
                "encoding": "jsonParsed"
            }
        ]
    });
    
    let response = client
        .post(&url)
        .json(&request_body)
        .send()
        .await
        .map_err(|e| anyhow!("Helius API request failed: {}", e))?;
    
    let result: serde_json::Value = response.json().await
        .map_err(|e| anyhow!("Failed to parse Helius response: {}", e))?;
    
    // Also check standard Token Program
    let request_body_standard = serde_json::json!({
        "jsonrpc": "2.0",
        "id": "2",
        "method": "getTokenAccountsByOwner",
        "params": [
            wallet_address.to_string(),
            {
                "programId": "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA" // Standard Token Program
            },
            {
                "encoding": "jsonParsed"
            }
        ]
    });
    
    let response_standard = client
        .post(&url)
        .json(&request_body_standard)
        .send()
        .await
        .map_err(|e| anyhow!("Helius API request failed (standard): {}", e))?;
    
    let result_standard: serde_json::Value = response_standard.json().await
        .map_err(|e| anyhow!("Failed to parse Helius response (standard): {}", e))?;
    
    let mut holdings = Vec::new();
    
    // Parse Token 2022 accounts
    if let Some(accounts) = result["result"]["value"].as_array() {
        for account in accounts {
            if let (Some(account_data), Some(pubkey_str)) = (
                account["account"]["data"]["parsed"]["info"].as_object(),
                account["pubkey"].as_str()
            ) {
                if let (Some(mint_str), Some(token_amount)) = (
                    account_data["mint"].as_str(),
                    account_data["tokenAmount"]["amount"].as_str()
                ) {
                    if let (Ok(mint), Ok(token_account), Ok(balance)) = (
                        Pubkey::from_str(mint_str),
                        Pubkey::from_str(pubkey_str),
                        token_amount.parse::<u64>()
                    ) {
                        if balance > 0 {
                            holdings.push((mint, token_account, balance));
                        }
                    }
                }
            }
        }
    }
    
    // Parse standard Token Program accounts
    if let Some(accounts) = result_standard["result"]["value"].as_array() {
        for account in accounts {
            if let (Some(account_data), Some(pubkey_str)) = (
                account["account"]["data"]["parsed"]["info"].as_object(),
                account["pubkey"].as_str()
            ) {
                if let (Some(mint_str), Some(token_amount)) = (
                    account_data["mint"].as_str(),
                    account_data["tokenAmount"]["amount"].as_str()
                ) {
                    if let (Ok(mint), Ok(token_account), Ok(balance)) = (
                        Pubkey::from_str(mint_str),
                        Pubkey::from_str(pubkey_str),
                        token_amount.parse::<u64>()
                    ) {
                        if balance > 0 {
                            // Avoid duplicates
                            if !holdings.iter().any(|(m, _, _)| *m == mint) {
                                holdings.push((mint, token_account, balance));
                            }
                        }
                    }
                }
            }
        }
    }
    
    Ok(holdings)
}

async fn monitor_positions(
    config: Arc<std::sync::RwLock<Config>>,
    wallet: Keypair,
    rpc: RpcClient,
    tracker: Arc<std::sync::RwLock<Option<TokenTracker>>>,
    event_tx: mpsc::UnboundedSender<TokenEvent>,
) {
    eprintln!("🔍 Position monitor started");
    
    // Wrap RPC client in Arc for parallel access
    let rpc_arc = Arc::new(rpc);
    
    // Track last MC value and timestamp for each position (for dead coin detection)
    let mut position_mc_history: HashMap<String, (f64, Instant)> = HashMap::new();
    
    // Check wallet holdings on startup
    {
        let helius_api_key = {
            let cfg = config.read().unwrap();
            cfg.helius_api_key.clone()
        };
        let wallet_address = wallet.pubkey();
        eprintln!("\n╔═══════════════════════════════════════════════════════════════╗");
        eprintln!("║           CHECKING WALLET TOKEN HOLDINGS                      ║");
        eprintln!("╚═══════════════════════════════════════════════════════════════╝");
        eprintln!("🔍 Wallet: {}", wallet_address);
        
        match get_wallet_token_holdings(&helius_api_key, &wallet_address).await {
            Ok(holdings) => {
                if holdings.is_empty() {
                    eprintln!("   ℹ️  No token holdings found in wallet");
                } else {
                    eprintln!("   ✅ Found {} token position(s):", holdings.len());
                    eprintln!();
                    
                    // Get SOL price for MC calculation
                    let sol_price = {
                        let cfg = config.read().unwrap();
                        cfg.sol_price_usd
                    };
                    
                    // Fetch bonding curve and MC for each token
                    for (idx, (mint, token_account, balance)) in holdings.iter().enumerate() {
                        eprintln!("   [{}/{}] Token Position:", idx + 1, holdings.len());
                        eprintln!("      - Mint: {}", mint);
                        eprintln!("      - Token Account: {}", token_account);
                        eprintln!("      - Balance: {} tokens", balance);
                        
                        // Try to derive bonding curve and get MC
                        let (bonding_curve, _) = crate::pda_derivation::derive_bonding_curve_pda(mint);
                        eprintln!("      - Bonding Curve: {}", bonding_curve);
                        
                        // Try to fetch MC
                        match fetch_bonding_curve_mc(rpc_arc.as_ref(), &bonding_curve, sol_price).await {
                            Ok((curve, mc_sol, mc_usd)) => {
                                let token_price = curve.get_token_price_sol();
                                // Balance is in raw units (like lamports), need to check token decimals
                                // For most tokens, decimals are 6-9, but we'll use the raw balance for now
                                // Position value = (balance * token_price) where balance is in smallest units
                                // Token price is per token, so we need to know decimals
                                // For simplicity, assume 1e9 (like SOL) for calculation
                                let balance_tokens = *balance as f64 / 1e9; // Adjust if token has different decimals
                                let position_value_sol = balance_tokens * token_price;
                                let position_value_usd = position_value_sol * sol_price;
                                
                                eprintln!("      - 💰 Market Cap: ${:.2} ({:.4} SOL)", mc_usd, mc_sol);
                                eprintln!("      - 📈 Token Price: {:.8} SOL (${:.6})", token_price, token_price * sol_price);
                                eprintln!("      - 💵 Position Value: ${:.2} ({:.4} SOL)", position_value_usd, position_value_sol);
                                eprintln!("      - 📊 Bonding Curve State:");
                                eprintln!("         • Virtual: {:.4} SOL / {:.2} tokens", 
                                    curve.virtual_sol_reserves as f64 / 1e9,
                                    curve.virtual_token_reserves as f64 / 1e9);
                                eprintln!("         • Real: {:.4} SOL / {:.2} tokens",
                                    curve.real_sol_reserves as f64 / 1e9,
                                    curve.real_token_reserves as f64 / 1e9);
                                eprintln!("         • Total Supply: {} tokens", curve.token_total_supply);
                                eprintln!("         • Complete: {}", curve.complete);
                            }
                            Err(e) => {
                                eprintln!("      - ⚠️  Could not fetch market cap: {}", e);
                                eprintln!("         (Token might be migrated or bonding curve doesn't exist)");
                            }
                        }
                        eprintln!();
                    }
                }
            }
            Err(e) => {
                eprintln!("   ❌ Failed to fetch token holdings: {}", e);
                eprintln!("      - Error details: {:?}", e);
            }
        }
        eprintln!("╚═══════════════════════════════════════════════════════════════╝");
        eprintln!();
    }
    
    loop {
        // Wrap entire loop iteration in error handling to prevent crashes
        let loop_result = async {
            // Check if auto-sell is enabled
            let enabled = {
                let cfg = config.read().unwrap();
                cfg.enable_auto_sell
            };

            if !enabled {
                eprintln!("⏸️  Auto-sell is DISABLED - skipping position monitoring");
                tokio::time::sleep(Duration::from_secs(10)).await;
                return Ok::<(), anyhow::Error>(());
            }
            
            // Auto-sell monitoring (silent mode - no console spam)

            // Get config values including Helius API key and dead coin settings
            let (stop_loss_percent, take_profit_mc_usd, _monitor_interval, sol_price_usd, helius_api_key, _sell_percent, enable_dead_coin_sell, dead_coin_timeout_sec) = {
                let cfg = config.read().unwrap();
                (cfg.stop_loss_percent, cfg.take_profit_mc_usd, cfg.monitor_interval_sec, cfg.sol_price_usd, cfg.helius_api_key.clone(), cfg.sell_percent, cfg.enable_dead_coin_sell, cfg.dead_coin_timeout_sec)
            };

            // Clean up positions with zero balance - LIVE (every check, using Helius API for speed)
            let user_wallet = wallet.pubkey();
            let positions_to_check = {
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
            
            if !positions_to_check.is_empty() {
                use solana_sdk::pubkey::Pubkey;
                use spl_associated_token_account::get_associated_token_address_with_program_id;
                use std::str::FromStr;
                
                let token_program_2022 = Pubkey::from_str("TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb")
                    .unwrap_or_else(|_| spl_token::id());
                
                let mut cleaned_count = 0;
                // Check positions using Helius API (faster and more reliable)
                for position in positions_to_check {
                    let user_token_account = match &position.user_token_account {
                        Some(ata_str) => {
                            match Pubkey::from_str(ata_str) {
                                Ok(pubkey) => pubkey,
                                Err(_) => {
                                    let mint = match Pubkey::from_str(&position.mint) {
                                        Ok(m) => m,
                                        Err(_) => continue,
                                    };
                                    get_associated_token_address_with_program_id(
                                        &user_wallet,
                                        &mint,
                                        &token_program_2022,
                                    )
                                }
                            }
                        }
                        None => {
                            let mint = match Pubkey::from_str(&position.mint) {
                                Ok(m) => m,
                                Err(_) => continue,
                            };
                            get_associated_token_address_with_program_id(
                                &user_wallet,
                                &mint,
                                &token_program_2022,
                            )
                        }
                    };
                    
                    // Try Helius API first (faster), fallback to RPC
                    let balance = match get_token_balance_helius(&helius_api_key, &user_token_account).await {
                        Ok(bal) => bal,
                        Err(_) => {
                            // Fallback to RPC if Helius fails
                            match rpc_arc.get_token_account_balance(&user_token_account).await {
                                Ok(balance_info) => balance_info.amount.parse().unwrap_or(0),
                                Err(_) => 0, // Account doesn't exist
                            }
                        }
                    };
                    
                    if balance == 0 {
                        // Balance is zero - mark as sold, BUT only if:
                        // 1. Position is not brand new (transaction still processing)
                        // 2. Position doesn't have a sell_signature (not sold through auto-sell)
                        //    - If auto-sell is enabled, we should only mark as sold if sell_signature exists
                        //    - This prevents marking as sold if auto-sell transaction failed or is pending
                        let is_new_position = {
                            let now = Utc::now();
                            let buy_time = position.timestamp;
                            let time_since_buy = now.signed_duration_since(buy_time);
                            // Don't mark as sold if buy was less than 5 seconds ago (transaction still processing)
                            time_since_buy.num_seconds() < 5
                        };
                        
                        // Check if position has sell_signature (sold through auto-sell)
                        let has_sell_signature = position.sell_signature.is_some();
                        
                        // Only mark as sold if:
                        // - Position is not new AND
                        // - Either has sell_signature (sold through auto-sell) OR auto-sell is disabled
                        // This prevents marking as sold if auto-sell transaction failed or is pending
                        if !is_new_position {
                            let should_mark_as_sold = {
                                let cfg = config.read().unwrap();
                                if cfg.enable_auto_sell {
                                    // If auto-sell is enabled, only mark as sold if sell_signature exists
                                    // This means the sell transaction was confirmed
                                    has_sell_signature
                                } else {
                                    // If auto-sell is disabled, mark as sold if balance is zero
                                    true
                                }
                            };
                            
                            if should_mark_as_sold {
                                // Balance is zero and conditions are met - mark as sold
                                if let Ok(mut tracker_guard) = tracker.write() {
                                    if let Some(tracker) = tracker_guard.as_mut() {
                                        if tracker.mark_position_as_sold(&position.mint).is_ok() {
                                            cleaned_count += 1;
                                        }
                                    }
                                }
                            }
                        }
                    } else {
                        // Balance is non-zero - update token_amount in tracker so UI shows current balance
                        if let Ok(mut tracker_guard) = tracker.write() {
                            if let Some(tracker) = tracker_guard.as_mut() {
                                let _ = tracker.update_token_amount(&position.mint, balance);
                            }
                        }
                    }
                }
                
                if cleaned_count > 0 {
                    eprintln!("🧹 Cleaned up {} positions with zero balance (live, Helius API)", cleaned_count);
                }
            }

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
                // Still check every 50ms even when no positions - ensures instant detection when position is added
                tokio::time::sleep(Duration::from_millis(50)).await;
                return Ok(());
            }
            
            // 🚀 PARALLEL CHECK: Check all positions simultaneously for ultra-fast detection
            let mut check_tasks = Vec::new();
            
            for position in active_positions.iter() {
                // Skip if we don't have required data
                let bonding_curve_str = match &position.bonding_curve {
                    Some(bc) => bc.clone(),
                    None => continue,
                };

                let bonding_curve = match Pubkey::from_str(&bonding_curve_str) {
                    Ok(pk) => pk,
                    Err(_) => continue,
                };

                let entry_mc = match position.mc_at_entry_usd {
                    Some(mc) => mc,
                    None => continue,
                };

                // Validate entry_mc to avoid division by zero
                if entry_mc <= 0.0 {
                    continue;
                }
                
                let position_mint = position.mint.clone();
                let position_clone = position.clone();
                let rpc_task = Arc::clone(&rpc_arc);
                let tracker_clone = tracker.clone();
                let event_tx_clone = event_tx.clone();
                let config_clone = {
                    let cfg = config.read().unwrap();
                    (*cfg).clone()
                };
                let wallet_bytes = wallet.to_bytes();
                let wallet_clone = match Keypair::from_bytes(&wallet_bytes) {
                    Ok(kp) => kp,
                    Err(_) => continue,
                };
                
                // Spawn parallel task for each position
                let task = tokio::spawn(async move {
                    // 🚀 ULTRA FAST: Fetch current MC (critical for stop loss)
                    let current_mc_result = fetch_bonding_curve_mc(
                        rpc_task.as_ref(),
                        &bonding_curve,
                        sol_price_usd,
                    ).await;

                    let current_mc = match current_mc_result {
                        Ok((_, _, mc_usd)) => mc_usd,
                        Err(_) => return None,
                    };
                    
                    // 🚀 PRIORITY: Check stop loss FIRST (most critical - must be ultra fast)
                    // Check stop loss: current_mc < entry_mc * (1.0 - stop_loss_percent/100.0)
                    let stop_loss_threshold = entry_mc * (1.0 - stop_loss_percent / 100.0);
                    let should_sell_stop_loss = current_mc < stop_loss_threshold;
                    
                    // 🚀 ULTRA FAST: If stop loss triggered, sell IMMEDIATELY (skip other checks)
                    if should_sell_stop_loss {
                        // Execute sell IMMEDIATELY (no delays, no other checks)
                        let _ = execute_sell(
                            &config_clone,
                            &wallet_clone,
                            rpc_task.as_ref(),
                            &tracker_clone,
                            &position_clone,
                            "stop_loss",
                            &event_tx_clone,
                        ).await;
                        
                        return Some(("stop_loss", position_mint));
                    }
                    
                    // Check take profit: current_mc >= take_profit_mc_usd
                    let should_sell_take_profit = current_mc >= take_profit_mc_usd;
                    
                    // Check dead coin: no price movement for X seconds
                    let should_sell_dead_coin = if enable_dead_coin_sell {
                        // Note: position_mc_history is not accessible here, so we skip dead coin check in parallel mode
                        // Dead coin check will be done in sequential pass if needed
                        false
                    } else {
                        false
                    };
                    
                    if should_sell_take_profit || should_sell_dead_coin {
                        let reason = if should_sell_take_profit {
                            "take_profit"
                        } else {
                            "dead_coin"
                        };
                        
                        // Execute sell (non-blocking for take profit/dead coin)
                        let _ = execute_sell(
                            &config_clone,
                            &wallet_clone,
                            rpc_task.as_ref(),
                            &tracker_clone,
                            &position_clone,
                            reason,
                            &event_tx_clone,
                        ).await;
                        
                        return Some((reason, position_mint));
                    }
                    
                    None
                });
                
                check_tasks.push(task);
            }
            
            // Wait for all checks to complete in parallel
            let results = futures_util::future::join_all(check_tasks).await;
            
            // Process results and update dead coin tracking
            for result in results {
                if let Ok(Some((reason, mint))) = result {
                    if reason == "stop_loss" {
                        position_mc_history.remove(&mint);
                    }
                }
            }
            
            // Sequential pass for dead coin detection (requires shared state)
            for position in active_positions.iter() {
                // Skip if already sold (check balance)
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

                if entry_mc <= 0.0 {
                    continue;
                }
                
                // Quick MC check for dead coin detection
                let current_mc_result = fetch_bonding_curve_mc(
                    rpc_arc.as_ref(),
                    &bonding_curve,
                    sol_price_usd,
                ).await;

                let current_mc = match current_mc_result {
                    Ok((_, _, mc_usd)) => mc_usd,
                    Err(_) => continue,
                };
                
                // Check dead coin: no price movement for X seconds
                if enable_dead_coin_sell {
                    let now = Instant::now();
                    let mc_change_threshold = 0.01; // 1% change threshold
                    
                    if let Some((last_mc, last_update_time)) = position_mc_history.get(&position.mint) {
                        let time_since_update = now.duration_since(*last_update_time);
                        let mc_change = ((current_mc - last_mc).abs() / last_mc.max(1.0)) * 100.0;
                        
                        // If MC hasn't changed significantly and timeout has passed, it's dead
                        if mc_change < mc_change_threshold && time_since_update.as_secs() >= dead_coin_timeout_sec {
                            // Remove from dead coin tracking
                            position_mc_history.remove(&position.mint);
                            
                            // Clone wallet and config immediately
                            let wallet_bytes = wallet.to_bytes();
                            let wallet_clone = match Keypair::from_bytes(&wallet_bytes) {
                                Ok(kp) => kp,
                                Err(_) => continue,
                            };
                            
                            let config_clone = {
                                let cfg = config.read().unwrap();
                                (*cfg).clone()
                            };

                            // Execute sell for dead coin
                            let _ = execute_sell(
                                &config_clone,
                                &wallet_clone,
                                rpc_arc.as_ref(),
                                &tracker,
                                &position,
                                "dead_coin",
                                &event_tx,
                            ).await;
                        } else {
                            // Update history if MC changed significantly
                            if mc_change >= mc_change_threshold {
                                position_mc_history.insert(position.mint.clone(), (current_mc, now));
                            }
                        }
                    } else {
                        // First time seeing this position - initialize history
                        position_mc_history.insert(position.mint.clone(), (current_mc, now));
                    }
                }
            }

            // Wait before next check
            // 🚀 ULTRA FAST MONITORING: Check every 50ms for instant price drop detection
            // This ensures we catch -30% drops within 50ms
            tokio::time::sleep(Duration::from_millis(50)).await;
            Ok(())
        }.await;
        
        // Handle errors gracefully - log and continue
        if let Err(e) = loop_result {
            eprintln!("⚠️  Error in auto-sell monitoring loop: {}", e);
            eprintln!("   Continuing monitoring in 1 second...");
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    }
}

/// Execute manual buy transaction (simplified version without filtering)
pub async fn execute_manual_buy(
    config: &Config,
    wallet: &Keypair,
    rpc: &RpcClient,
    tracker: &Arc<std::sync::RwLock<Option<TokenTracker>>>,
    mut accounts: PumpBuyAccounts,
    sol_amount: u64,
    _metrics: &SharedMetrics,
    _event_tx: &mpsc::UnboundedSender<TokenEvent>,
) -> Result<String> {
    use crate::buy::build_buy_instruction;
    use crate::helius::send_helius_transaction;
    use crate::jito::send_jito_bundle;
    use crate::accounts::{fetch_bonding_curve_mc, TokenBuy};
    use solana_sdk::{
        instruction::Instruction,
        system_instruction,
        compute_budget::ComputeBudgetInstruction,
        transaction::VersionedTransaction,
        message::v0,
        message::VersionedMessage,
    };
    use solana_client::nonblocking::rpc_client::RpcClient as AsyncRpcClient;
    use spl_associated_token_account::instruction::{
        create_associated_token_account, create_associated_token_account_idempotent,
    };
    use std::str::FromStr;
    // use crate::blockhash_cache::get_cached_blockhash;
    use crate::constants::HELIUS_TIP_ACCOUNTS;
    use rand::seq::SliceRandom;
    
    let user_wallet = wallet.pubkey();
    let mint = accounts.mint;
    
    eprintln!("╔═══════════════════════════════════════════════════════════════╗");
    eprintln!("║                    MANUAL BUY                                ║");
    eprintln!("╚═══════════════════════════════════════════════════════════════╝");
    eprintln!("  Mint:           {}", mint);
    eprintln!("  SOL Amount:     {:.6} SOL", sol_amount as f64 / 1e9);
    eprintln!();
    
    // Get user token account
    let token_program_2022_id = Pubkey::from_str("TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb")
        .unwrap_or_else(|_| spl_token::id());
    
    // IMPORTANT: Derive ATA using Token Program 2022 for Pump.fun tokens
    use spl_associated_token_account::get_associated_token_address_with_program_id;
    let user_ata = get_associated_token_address_with_program_id(
        &user_wallet,
        &accounts.mint,
        &token_program_2022_id
    );
    
    // Set hardcoded Global Volume
    let hardcoded_global_volume = Pubkey::from_str("Hq2wp8uJ9jCPsYgNHex8RtqdvMPfVGoYwjvF1ATiwn2Y")
        .expect("Invalid hardcoded Global Volume address");
    accounts.global_volume = hardcoded_global_volume;
    
    // Fetch bonding curve for accurate token amount calculation
    let bonding_curve_opt = {
        // Get SOL price for MC calculation (we only need the curve, not MC)
        let sol_price_usd = 150.0; // Default fallback, actual value not critical for token amount calc
        match fetch_bonding_curve_mc(rpc, &accounts.bonding_curve, sol_price_usd).await {
            Ok((curve, _, _)) => Some(curve),
            Err(_) => None, // Fallback to initial price if fetch fails
        }
    };
    
    // Build buy instruction
    eprintln!("╔═══════════════════════════════════════════════════════════════╗");
    eprintln!("║              BUILDING BUY INSTRUCTION                         ║");
    eprintln!("╚═══════════════════════════════════════════════════════════════╝");
    
    let buy_ix = build_buy_instruction(
        rpc,
        &accounts,
        &user_wallet,
        &user_ata,
        sol_amount,
        config.slippage_percent,
        bonding_curve_opt.as_ref(), // Use current bonding curve price if available
    ).await?;
    
    eprintln!("  ✅ Buy instruction built ({} accounts)", buy_ix.accounts.len());
    eprintln!();
    
    let tip_account = {
        let mut rng = rand::thread_rng();
        Pubkey::from_str(
            HELIUS_TIP_ACCOUNTS.choose(&mut rng).unwrap()
        )?
    };
    
    // Check if ATA already exists
    let ata_exists = rpc.get_account(&user_ata).await.is_ok();
    
    // Calculate priority fee
    let priority_fee = if config.enable_dynamic_priority_fee {
        match config.calculate_dynamic_priority_fee(rpc).await {
            Ok(fee) => fee,
            Err(_) => config.priority_fee,
        }
    } else {
        config.priority_fee
    };
    
    let mut instructions = vec![
        ComputeBudgetInstruction::set_compute_unit_limit(config.compute_units),
        ComputeBudgetInstruction::set_compute_unit_price(priority_fee),
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
    
    instructions.push(buy_ix);
    instructions.push(system_instruction::transfer(
        &user_wallet,
        &tip_account,
        config.jito_tip,
    ));
    
    let recent_blockhash = rpc.get_latest_blockhash().await?;
    
    // Build and send transaction
    eprintln!("╔═══════════════════════════════════════════════════════════════╗");
    eprintln!("║                 SENDING TRANSACTION                           ║");
    eprintln!("╚═══════════════════════════════════════════════════════════════╝");
    
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
    
    let tx_helius = tx.clone();
    let tx_jito = tx.clone();
    let tx_rpc = tx.clone();
    
    let wallet_bytes = wallet.to_bytes();
    let wallet_clone = Keypair::from_bytes(&wallet_bytes)?;
    let jito_tip = config.jito_tip;
    let rpc_url = config.rpc_url.clone();
    
    let helius_task = tokio::spawn(async move {
        match send_helius_transaction(tx_helius).await {
            Ok(sig) => Ok(sig),
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
        let rpc_client = AsyncRpcClient::new(rpc_url);
        match rpc_client.send_transaction(&tx_rpc).await {
            Ok(sig) => Ok(sig.to_string()),
            Err(e) => Err(anyhow::anyhow!("RPC error: {}", e)),
        }
    });
    
    let (helius_res, jito_res, rpc_res) = tokio::join!(helius_task, jito_task, rpc_task);
    
    // Find first successful result
    let mut actual_signature = None;
    if let Ok(Ok(sig)) = helius_res {
        actual_signature = Some(sig);
    } else if let Ok(Ok(bundle_id)) = jito_res {
        actual_signature = Some(bundle_id);
    } else if let Ok(Ok(sig)) = rpc_res {
        actual_signature = Some(sig);
    }
    
    if let Some(sig) = actual_signature {
        eprintln!("  ✅ Transaction submitted: {}", sig);
        
        // Record in tracker
        let mc_result = fetch_bonding_curve_mc(
            rpc,
            &accounts.bonding_curve,
            config.sol_price_usd,
        ).await;
        
        let (mc_entry_usd, token_price_entry) = match mc_result {
            Ok((curve, _, mc)) => {
                let price = curve.get_token_price_sol();
                (Some(mc), if price > 0.0 { Some(price) } else { None })
            }
            Err(_) => (None, None)
        };

        if let Ok(mut tracker_opt) = tracker.write() {
            if let Some(tracker) = tracker_opt.as_mut() {
                let buy = TokenBuy {
                    token_number: 0, // Not relevant for manual buy
                    mint: mint.to_string(),
                    signature: sig.clone(),
                    creator: accounts.creator.to_string(),
                    dev_buy_sol: 0.0, // Not relevant for manual buy
                    our_buy_sol: sol_amount as f64 / 1e9,
                    timestamp: Utc::now(),
                    has_socials: false,
                    twitter: None,
                    website: None,
                    telegram: None,
                    creator_token_count: 0,
                    detection_method: "manual".to_string(),
                    mc_at_detection_usd: None,
                    mc_at_entry_usd: mc_entry_usd,
                    token_price_sol: token_price_entry,
                    token_amount: None,
                    user_token_account: Some(user_ata.to_string()),
                    bonding_curve: Some(accounts.bonding_curve.to_string()),
                    sold: false,
                    sell_signature: None,
                    current_price_sol: None,
                    current_value_sol: None,
                    pnl_sol: None,
                    pnl_percent: None,
                    last_pnl_update: None,
                    buy_fees_sol: Some(0.00002), // Estimate for manual buy
                };
                
                if let Err(e) = tracker.record_buy(buy) {
                    eprintln!("  ⚠️  Tracker error: {}", e);
                }
            }
        }
        
        Ok(sig)
    } else {
        Err(anyhow!("All submission methods failed"))
    }
}

/// Extract Creator Vault directly from buy transaction (Account 9 in buy instruction)
async fn extract_creator_vault_from_buy_tx(
    rpc: &RpcClient,
    buy_signature: &str,
) -> Result<Pubkey> {
    use solana_sdk::signature::Signature;
    use solana_transaction_status::UiTransactionEncoding;
    use base64::{engine::general_purpose, Engine as _};
    use solana_sdk::message::VersionedMessage;
    use solana_sdk::transaction::VersionedTransaction;
    
    let sig = Signature::from_str(buy_signature)?;
    
    let tx = rpc.get_transaction_with_config(
        &sig,
        solana_client::rpc_config::RpcTransactionConfig {
            encoding: Some(UiTransactionEncoding::Base64),
            max_supported_transaction_version: Some(0),
            commitment: Some(CommitmentConfig::confirmed()),
        }
    ).await?;

    if let solana_transaction_status::EncodedTransaction::Binary(encoded, _) = &tx.transaction.transaction {
        let tx_bytes = general_purpose::STANDARD.decode(encoded)?;
        let versioned_tx: VersionedTransaction = bincode::deserialize(&tx_bytes)?;
        
        let account_keys = match &versioned_tx.message {
            VersionedMessage::Legacy(msg) => &msg.account_keys,
            VersionedMessage::V0(msg) => &msg.account_keys,
        };

        let instructions = match &versioned_tx.message {
            VersionedMessage::Legacy(msg) => &msg.instructions,
            VersionedMessage::V0(msg) => &msg.instructions,
        };

        let pump_program = Pubkey::from_str(PUMP_PROGRAM_ID)?;
        let buy_discriminator = crate::constants::BUY_DISCRIMINATOR;

        // Find buy instruction
        for ix in instructions.iter() {
            let program_id_idx = ix.program_id_index as usize;
            if let Some(&program_id) = account_keys.get(program_id_idx) {
                if program_id == pump_program && ix.data.len() >= 8 {
                    let discriminator = &ix.data[0..8];
                    let is_buy = discriminator == &buy_discriminator
                        || discriminator == &[0x33, 0xE6, 0x85, 0x5A, 0x5B, 0x6B, 0xBD, 0x5B];
                    
                    if is_buy {
                        // Extract Creator Vault from Account 9 (index 9 in buy instruction)
                        if ix.accounts.len() > 9 {
                            let account_idx = ix.accounts[9] as usize;
                            if let Some(&creator_vault) = account_keys.get(account_idx) {
                                return Ok(creator_vault);
                            }
                        }
                        return Err(anyhow!("Buy instruction found but Account 9 (Creator Vault) not available"));
                    }
                }
            }
        }
        
        Err(anyhow!("Buy instruction not found in transaction"))
    } else {
        Err(anyhow!("Transaction encoding not supported"))
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
    // ⚡ ULTRA FAST SELL - Minimal logging, maximum speed
    eprintln!("🚨 ULTRA FAST SELL: {}", position.mint);
    let mint = Pubkey::from_str(&position.mint)?;
    let bonding_curve = Pubkey::from_str(
        position.bonding_curve.as_ref()
            .ok_or_else(|| anyhow!("Bonding curve not found"))?
    )?;
    let user_wallet = wallet.pubkey();
    
    // ⚡ ULTRA FAST: Derive accounts in parallel
    let token_program_2022 = Pubkey::from_str("TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb").unwrap();
    let user_token_account_from_tracker = position.user_token_account.as_ref()
        .and_then(|s| Pubkey::from_str(s).ok());
    
    let user_token_account_2022 = get_associated_token_address_with_program_id(
        &user_wallet, &mint, &token_program_2022
    );
    
    // ⚡ ULTRA FAST: Get balance - try tracker ATA first (fastest), then Token 2022 RPC (single attempt)
    let mut token_balance = 0u64;
    let mut user_token_account = user_token_account_2022;
    let token_program_used = token_program_2022;
    
    // Try tracker ATA first (if available) - fastest path
    if let Some(tracker_ata) = user_token_account_from_tracker {
        if let Ok(balance) = rpc.get_token_account_balance(&tracker_ata).await {
            token_balance = balance.amount.parse::<u64>().unwrap_or(0);
            if token_balance > 0 {
                user_token_account = tracker_ata;
            }
        }
    }
    
    // If tracker ATA failed, try Token 2022 directly (single attempt, no retries)
    if token_balance == 0 {
        if let Ok(balance) = rpc.get_token_account_balance(&user_token_account_2022).await {
            token_balance = balance.amount.parse::<u64>().unwrap_or(0);
        }
    }
    
    if token_balance == 0 {
        return Err(anyhow!("Token balance is 0"));
    }

    // ⚡ ULTRA FAST: Calculate sell amount
    let sell_amount = (token_balance as f64 * (config.sell_percent / 100.0)) as u64;
    if sell_amount == 0 {
        return Err(anyhow!("Sell amount is 0"));
    }

    // ⚡ ULTRA FAST: Reconstruct accounts in parallel with blockhash fetch
    let creator = Pubkey::from_str(&position.creator)?;
    let associated_bonding_curve = get_associated_token_address_with_program_id(
        &bonding_curve, &mint, &token_program_used
    );
    
    // ⚡ ULTRA FAST: Get blockhash and creator vault in parallel
    let creator_vault_fut: std::pin::Pin<Box<dyn std::future::Future<Output = Result<Pubkey>> + Send>> = Box::pin(async move {
        if position.signature.starts_with("MOCK_") {
            let (vault, _) = crate::pda_derivation::derive_creator_vault_pda(&creator);
            Ok(vault)
        } else {
            extract_creator_vault_from_buy_tx(rpc, &position.signature).await
                .or_else(|_| {
                    let (vault, _) = crate::pda_derivation::derive_creator_vault_pda(&creator);
                    Ok(vault)
                })
        }
    });
    let (recent_blockhash, creator_vault) = tokio::join!(
        rpc.get_latest_blockhash(),
        creator_vault_fut
    );
    let recent_blockhash = recent_blockhash?;
    let creator_vault = creator_vault?;
    
    let accounts = PumpBuyAccounts {
        mint, bonding_curve, associated_bonding_curve, creator_vault,
        event_authority: config.event_authority, global_volume: config.global_volume,
        global: config.global_account, fee_recipient: config.fee_recipient,
        fee_config: config.fee_config, fee_program: config.fee_program,
        dev_buy_sol: 0, creator, associated_bonding_curve_instruction: None,
    };

    // ⚡ ULTRA FAST: Build sell instruction and get priority fee in parallel
    let (sell_ix, priority_fee) = tokio::join!(
        build_sell_instruction(&accounts, &user_wallet, &user_token_account, sell_amount),
        async {
            if config.enable_dynamic_priority_fee {
                config.calculate_dynamic_priority_fee(rpc).await.unwrap_or(config.priority_fee)
            } else {
                config.priority_fee
            }
        }
    );
    let sell_ix = sell_ix?;
    
    // ⚡ ULTRA FAST: Build transaction
    let helius_tip_amount = 200_000u64;
    let helius_tip_account = crate::constants::random_helius_tip_account();
    let instructions = vec![
        ComputeBudgetInstruction::set_compute_unit_limit(config.compute_units),
        ComputeBudgetInstruction::set_compute_unit_price(priority_fee),
        sell_ix,
        system_instruction::transfer(&user_wallet, &helius_tip_account, helius_tip_amount),
    ];
    
    let msg = v0::Message::try_compile(&user_wallet, &instructions, &[], recent_blockhash)?;
    let tx = VersionedTransaction::try_new(VersionedMessage::V0(msg), &[wallet])?;

    // ⚡ ULTRA FAST: Send transaction (mock mode check)
    if config.mock_sell {
        use solana_sdk::signature::Signature;
        use rand::Rng;
        let mut rng = rand::thread_rng();
        let mut mock_sig_bytes = [0u8; 64];
        rng.fill(&mut mock_sig_bytes);
        let mock_signature = format!("MOCK_SELL_{}", Signature::from(mock_sig_bytes).to_string());
        if let Ok(mut tracker_opt) = tracker.write() {
            if let Some(tracker) = tracker_opt.as_mut() {
                let _ = tracker.mark_as_sold(&position.mint, mock_signature.clone());
            }
        }
        let _ = event_tx.send(TokenEvent::Sold {
            mint: position.mint.clone(),
            signature: mock_signature.clone(),
            reason: format!("{} (MOCK)", reason),
            pnl: None,
            timestamp: Utc::now(),
        });
        return Ok(mock_signature);
    }

    // ⚡ ULTRA FAST: Send transaction - use fastest method directly
    let tx_sig = match config.submission_mode {
        crate::config::SubmissionMode::Helius => send_helius_transaction(tx).await?,
        crate::config::SubmissionMode::Jito => {
            return Ok(format!("Jito: {}", send_jito_bundle(tx, wallet, recent_blockhash, config.jito_tip).await?));
        }
        crate::config::SubmissionMode::Rpc => rpc.send_transaction(&tx).await?.to_string(),
        crate::config::SubmissionMode::All => {
            // ⚡ ULTRA FAST: Try all in parallel, use first success
            let tx_helius = tx.clone();
            let tx_jito = tx.clone();
            let tx_rpc = tx.clone();
            let wallet_bytes = wallet.to_bytes();
            let wallet_clone = Keypair::from_bytes(&wallet_bytes)?;
            let jito_tip = config.jito_tip;
            let rpc_url = config.rpc_url.clone();
            
            tokio::select! {
                res = tokio::spawn(async move { send_helius_transaction(tx_helius).await }) => {
                    res??.to_string()
                }
                res = tokio::spawn(async move { send_jito_bundle(tx_jito, &wallet_clone, recent_blockhash, jito_tip).await }) => {
                    return Ok(format!("Jito: {}", res??));
                }
                res = tokio::spawn(async move {
                    RpcClient::new(rpc_url).send_transaction(&tx_rpc).await
                }) => {
                    res??.to_string()
                }
            }
        }
    };

    let signature = tx_sig.to_string();

    // ⚡ ULTRA FAST: Mark as sold IMMEDIATELY (no verification wait)
    if let Ok(mut tracker_opt) = tracker.write() {
        if let Some(tracker) = tracker_opt.as_mut() {
            let _ = tracker.mark_as_sold(&position.mint, signature.clone());
        }
    }

    // ⚡ ULTRA FAST: Send event immediately (non-blocking)
    let _ = event_tx.send(TokenEvent::Sold {
        mint: position.mint.clone(),
        signature: signature.clone(),
        reason: reason.to_string(),
        pnl: None, // Skip PnL calculation for speed
        timestamp: Utc::now(),
    });
    
    eprintln!("✅ SELL EXECUTED: {} -> {}", position.mint, signature);
    Ok(signature)
}

/// Ultra-fast PnL monitor - updates every 100ms for live display
async fn monitor_pnl_ultra_fast(
    config: Arc<std::sync::RwLock<Config>>,
    _rpc: RpcClient,
    tracker: Arc<std::sync::RwLock<Option<TokenTracker>>>,
) {
    let mut interval = tokio::time::interval(Duration::from_millis(100)); // Update every 100ms (10x/sec)
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    
    loop {
        interval.tick().await;
        
        // Get config values
        let (sol_price_usd, _) = {
            let cfg = config.read().unwrap();
            (cfg.sol_price_usd, cfg.helius_api_key.clone())
        };
        
        // Get active positions
        let positions_to_update = {
            if let Ok(tracker_guard) = tracker.read() {
                if let Some(tracker_ref) = tracker_guard.as_ref() {
                    tracker_ref.get_active_positions_for_pnl()
                } else {
                    Vec::new()
                }
            } else {
                Vec::new()
            }
        };
        
        if positions_to_update.is_empty() {
            continue;
        }
        
        // Batch update all positions (parallel for speed)
        use std::str::FromStr;
        use solana_sdk::pubkey::Pubkey;
        
        let mut update_tasks = Vec::new();
        
        for (mint, bonding_curve_str) in positions_to_update {
            let mint_clone = mint.clone();
            let bc_str = bonding_curve_str.clone();
            let config_clone = config.clone();
            let tracker_clone = tracker.clone();
            let sol_price = sol_price_usd;
            
            let task = tokio::spawn(async move {
                // Create RPC client for this task
                let task_rpc = {
                    let cfg = config_clone.read().unwrap();
                    cfg.create_rpc_client()
                };
                
                // Parse bonding curve
                let bonding_curve = match Pubkey::from_str(&bc_str) {
                    Ok(pk) => pk,
                    Err(_) => return,
                };
                
                // Fetch current price (fast, with retry)
                match fetch_bonding_curve_mc(
                    &task_rpc,
                    &bonding_curve,
                    sol_price,
                ).await {
                    Ok((curve, _, _)) => {
                        let current_price = curve.get_token_price_sol();
                        
                        // Update PnL in tracker (fast, no disk write)
                        if let Ok(mut tracker_guard) = tracker_clone.write() {
                            if let Some(tracker) = tracker_guard.as_mut() {
                                let _ = tracker.update_position_pnl_fast(&mint_clone, current_price);
                            }
                        }
                    }
                    Err(_) => {
                        // Silently fail - will retry on next cycle
                    }
                }
            });
            
            update_tasks.push(task);
        }
        
        // Wait for all updates to complete (but don't block too long)
        let timeout = tokio::time::timeout(Duration::from_secs(1), futures_util::future::join_all(update_tasks));
        let _ = timeout.await;
    }
}

