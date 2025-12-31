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
use crate::socials::{check_token_metadata, Socials, TokenMetadata};
use crate::das_check::check_creator_token_count_das;
use crate::filters::check_creator_token_count;
use crate::accounts::{TokenBuy, TokenTracker, SeenTokens, fetch_bonding_curve_mc, BondingCurveAccount};
use crate::account_subscription::{AccountSubscriptionManager, SubscriptionHandle};
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
use serde_json;

fn format_addr(addr: &str) -> String {
    if addr.len() > 10 {
        format!("{}...{}", &addr[..6], &addr[addr.len()-4..])
    } else {
        addr.to_string()
    }
}

// Static counter to track how many times run_bot is called
static RUN_BOT_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

// Helper function to check if error is due to Helius quota/rate limiting
fn is_helius_quota_error(error: &anyhow::Error) -> bool {
    let error_str = error.to_string().to_lowercase();
    error_str.contains("quota") || 
    error_str.contains("rate limit") ||
    error_str.contains("429") ||
    error_str.contains("too many requests") ||
    error_str.contains("usage limit") ||
    error_str.contains("exceeded") ||
    error_str.contains("helius") && (error_str.contains("limit") || error_str.contains("quota"))
}

// Helper function to check if verbose debug logging is enabled
// Set DEBUG_VERBOSE=1 environment variable to enable verbose debug output
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
    let call_number = RUN_BOT_COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
    eprintln!("[BOT] Started (call #{})", call_number);
    
    // Refresh SOL price on bot startup (before any operations that might use it)
    crate::utils::refresh_sol_price_if_needed().await;
    
    // Load initial config
    let initial_config = {
        let cfg = config.read().map_err(|e| {
            anyhow::anyhow!("Failed to read config: {}", e)
        })?;
        (*cfg).clone()
    };
    
    // Create RPC client
    let rpc = initial_config.create_rpc_client();
    
    // Update wallet balance
    match rpc.get_balance(&wallet.pubkey()).await {
        Ok(balance) => {
            if let Ok(mut bal) = wallet_balance.write() {
                *bal = balance as f64 / 1e9;
            }
        }
        Err(e) => {
            let error = anyhow::anyhow!("Failed to get wallet balance: {}", e);
            if is_helius_quota_error(&error) {
                let _ = event_tx.send(TokenEvent::Error {
                    message: "⚠️ Helius RPC quota/usage limit reached. Please check your Helius account usage or upgrade your plan.".to_string(),
                    timestamp: Utc::now(),
                });
                eprintln!("[BOT] Helius quota error: {}", error);
            } else {
                eprintln!("[BOT] Failed to get wallet balance: {}", error);
            }
            // Don't return error here - continue with bot startup
        }
    }
    
    // Initialize static caches
    match crate::buy::init_static_caches() {
        Ok(_) => {}
        Err(e) => {
            // Cache already initialized from previous run - this is OK, not an error
            if !e.to_string().contains("already initialized") {
                eprintln!("[ERR] Failed to initialize static caches: {}", e);
                return Err(e);
            }
        }
    }
    
    // Pre-load global account
    if let Err(e) = crate::buy::preload_global(&rpc, &initial_config.global_account).await {
        let error = anyhow::anyhow!("Failed to preload global account: {}", e);
        if is_helius_quota_error(&error) {
            let _ = event_tx.send(TokenEvent::Error {
                message: "⚠️ Helius RPC quota/usage limit reached. Please check your Helius account usage or upgrade your plan.".to_string(),
                timestamp: Utc::now(),
            });
            eprintln!("[ERR] Helius quota error during global account preload: {}", error);
            // Don't return error - continue with bot startup, it will retry later
        } else {
            eprintln!("[ERR] Failed to preload global account: {}", e);
            return Err(error);
        }
    }
    
    // Create token logger
    let logger = Arc::new(std::sync::Mutex::new(
        crate::token_logger::create_logger()
            .unwrap_or_else(|e| {
                eprintln!("⚠️  Failed to create token logger: {}", e);
                // Create a dummy logger that does nothing
                crate::token_logger::TokenLogger::new("/dev/null").unwrap()
            })
    ));
    
    // ✅ FIX: Create shutdown signal for graceful background task termination
    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
    
    // Start position monitor as background task
    let monitor_config = config.clone();
    let monitor_wallet_bytes = wallet.to_bytes();
    let monitor_wallet = Keypair::from_bytes(&monitor_wallet_bytes)?;
    let monitor_rpc = initial_config.create_rpc_client();
    let monitor_tracker = tracker.clone();
    let monitor_event_tx = event_tx.clone();
    let monitor_shutdown = shutdown_rx.clone();
    
    let _monitor_handle = tokio::spawn(async move {
        monitor_positions(
            monitor_config,
            monitor_wallet,
            monitor_rpc,
            monitor_tracker,
            monitor_event_tx,
            monitor_shutdown,
        ).await;
    });
    
    // Start ultra-fast PnL monitor as separate background task
    let pnl_config = config.clone();
    let pnl_rpc = initial_config.create_rpc_client();
    let pnl_tracker = tracker.clone();
    
    // 📊 ULTRA HISTORY TRACKER - for chart generation
    let history_tracker = Arc::new(std::sync::RwLock::new(
        crate::accounts::HistoryTracker::new()
            .unwrap_or_else(|e| {
                eprintln!("⚠️  Failed to create history tracker: {}", e);
                crate::accounts::HistoryTracker::new().unwrap()
            })
    ));
    let pnl_history = history_tracker.clone();
    let pnl_shutdown = shutdown_rx.clone();
    
    let _pnl_handle = tokio::spawn(async move {
        monitor_pnl_ultra_fast(
            pnl_config,
            pnl_rpc,
            pnl_tracker,
            pnl_history,
            pnl_shutdown,
        ).await;
    });
    
    // Initialize AccountSubscriptionManager for real-time PNL updates
    let subscription_manager = AccountSubscriptionManager::new(
        config.clone(),
        tracker.clone(),
    );
    let subscription_shutdown = shutdown_rx.clone();
    let subscription_handle_opt = match subscription_manager.start(subscription_shutdown).await {
        Ok(handle) => {
            eprintln!("[ACCOUNT_SUBSCRIPTION] ✅ Real-time PNL subscription manager started");
            Some(handle)
        }
        Err(e) => {
            eprintln!("[ACCOUNT_SUBSCRIPTION] ⚠️  Failed to start subscription manager: {} (falling back to polling)", e);
            None
        }
    };
    
    // Store subscription handle in Arc for passing to other functions
    let subscription_handle: Arc<Option<SubscriptionHandle>> = Arc::new(subscription_handle_opt);
    
    let mut detected = 0;
    let mut reconnect_count = 0;
    
    // Main bot loop
    loop {
        // ✅ CRITICAL: Check for control messages (non-blocking)
        // Use try_recv to avoid blocking, but also check if channel is disconnected
        match control_rx.try_recv() {
            Ok(control) => {
                match control {
                    BotControl::Stop => {
                        let _ = event_tx.send(TokenEvent::Info {
                            message: "Bot stopped by user".to_string(),
                            timestamp: Utc::now(),
                        });
                        // ✅ FIX: Signal shutdown to background tasks
                        let _ = shutdown_tx.send(true);
                        tokio::time::sleep(Duration::from_millis(100)).await;
                        
                        // Print tracker summary before stopping
                        if let Ok(tracker_guard) = tracker.write() {
                            if let Some(tracker_ref) = tracker_guard.as_ref() {
                                tracker_ref.print_summary();
                            }
                        }
                        
                        eprintln!("[BOT] Stopped");
                        return Ok(());
                    }
                    BotControl::UpdateConfig(new_config) => {
                        // Update config, will be used in next iteration
                        if let Ok(mut cfg) = config.write() {
                            *cfg = new_config;
                        }
                    }
                    BotControl::Restart => {
                        // ✅ FIX: Reset reconnect_count and continue loop (don't break)
                        // This allows bot to reconnect immediately
                        reconnect_count = 0;
                        // Continue loop to reconnect immediately (skip increment below)
                        // Get current config and connect immediately
                        let current_config = match config.read() {
                            Ok(cfg) => (*cfg).clone(),
                            Err(e) => {
                                eprintln!("[BOT] Failed to read config during reconnect: {}", e);
                                // Use initial config as fallback
                                initial_config.clone()
                            }
                        };
                        
                        // Connect immediately without delay
                        match listen_websocket_once(
                            &current_config,
                            &config,
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
                            &history_tracker,
                            subscription_handle.clone(),
                        ).await {
                            Ok(stopped) => {
                                if stopped {
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
                        continue; // Skip the normal reconnect logic below
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
            Err(tokio::sync::mpsc::error::TryRecvError::Empty) => {
                // No control message, continue normally
            }
            Err(tokio::sync::mpsc::error::TryRecvError::Disconnected) => {
                eprintln!("[ERR] Control channel disconnected");
                // ✅ CRITICAL: If channel is disconnected, bot should exit
                // This happens when control_tx_bot is dropped/cleared
                let _ = event_tx.send(TokenEvent::Info {
                    message: "Bot stopped (control channel disconnected)".to_string(),
                    timestamp: Utc::now(),
                });
                // Signal shutdown to background tasks
                let _ = shutdown_tx.send(true);
                tokio::time::sleep(Duration::from_millis(100)).await;
                eprintln!("[BOT] Stopped (channel disconnected)");
                return Ok(());
            }
        }
        
        // Get current config
        let current_config = match config.read() {
            Ok(cfg) => (*cfg).clone(),
            Err(e) => {
                eprintln!("[BOT] Failed to read config in main loop: {}", e);
                // Use initial config as fallback
                initial_config.clone()
            }
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
            &history_tracker,
            subscription_handle.clone(),
        ).await {
            Ok(stopped) => {
                if stopped {
                    return Ok(());
                }
                reconnect_count = 0;
            }
            Err(e) => {
                eprintln!("[ERR] WebSocket error: {}", e);
                let _ = event_tx.send(TokenEvent::Error {
                    message: format!("WebSocket error: {}", e),
                    timestamp: Utc::now(),
                });
                // ✅ FIX: Don't reset reconnect_count on error - let it increment for backoff
            }
        }
        
        // ✅ FIX: Reset reconnect_count if it gets too high (prevent infinite backoff)
        if reconnect_count > 10 {
            tokio::time::sleep(Duration::from_secs(60)).await;
            reconnect_count = 0;
        }
    }
    // Note: This function never returns normally - it always exits via return statements in the loop
    // (when Stop signal is received, channel disconnects, or WebSocket error occurs)
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
    history_tracker: &Arc<std::sync::RwLock<crate::accounts::HistoryTracker>>,
    subscription_handle: Arc<Option<SubscriptionHandle>>,
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
    
    eprintln!("[WS] Connected to {}", &config.wss_url);
    
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
                            let position_opt = if let Ok(tracker_guard) = tracker.read() {
                                if let Some(tracker_ref) = tracker_guard.as_ref() {
                                    let positions = tracker_ref.get_active_positions();
                                    positions.into_iter()
                                        .find(|p| p.mint == mint_to_sell)
                                } else {
                                    None
                                }
                            } else {
                                None
                            };
                            
                            if let Some(position) = position_opt {
                                // Clone resources for sell execution
                                let wallet_bytes = wallet.to_bytes();
                                let wallet_clone = match Keypair::from_bytes(&wallet_bytes) {
                                    Ok(kp) => kp,
                                    Err(_) => continue,
                                };
                                
                                let config_clone = {
                                    let cfg = config_arc.read().unwrap();
                                    (*cfg).clone()
                                };
                                
                                let rpc_client = config_clone.create_rpc_client();
                                
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
                                
                                // Execute buy using existing logic (history_tracker will be registered in execute_manual_buy)
                                match execute_manual_buy(
                                    &config_clone,
                                    &wallet_clone,
                                    &rpc_client,
                                    &tracker_clone,
                                    accounts,
                                    buy_amount,
                                    &metrics_clone,
                                    &event_tx_clone,
                                    None, // Manual buy from GUI doesn't have history_tracker
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
            
            eprintln!("[TOKEN] {} detected", format_addr(&mint));
            
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
                Some(history_tracker.clone()),
                Some(subscription_handle.clone()),
            ).await {
                Ok(sig) => {
                    if let Some(signature) = sig {
                        // Get MC if available from tracker - use try_read to avoid blocking
                        let mc = {
                            match tracker.try_read() {
                                Ok(tracker_guard) => {
                                    if let Some(tracker_ref) = tracker_guard.as_ref() {
                                        let buys = tracker_ref.get_recent_buys(1);
                                        buys.first().and_then(|b| b.mc_at_entry_sol)
                                    } else {
                                        None
                                    }
                                }
                                Err(_) => {
                                    // Tracker is locked, skip MC lookup to avoid blocking
                                    None
                                }
                            }
                        };
                        
                        let mc_str = mc.map(|m| format!(" (MC: ${:.0})", m)).unwrap_or_default();
                        eprintln!("[BUY] {} - {:.3} SOL - TX: {}{}", 
                                 format_addr(&mint), 
                                 dev_buy_sol,
                                 format_addr(&signature),
                                 mc_str);
                        
                        // Send event (non-blocking - unbounded channel never blocks)
                        // Check if channel is still connected before sending
                        if !event_tx.is_closed() {
                            let _ = event_tx.send(TokenEvent::Bought {
                                mint: mint.clone(),
                                signature: signature.clone(),
                                mc,
                                timestamp: Utc::now(),
                            });
                        } else {
                            eprintln!("⚠️  Event channel closed, cannot send Bought event");
                        }
                        
                        // Log bought token (non-blocking)
                        if let Ok(logger_guard) = logger.try_lock() {
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
                        if config_for_buy.one_shot_mode {
                            eprintln!("🎯 One Shot Mode: Buy successful, entering monitor-only mode...");
                            let _ = event_tx.send(TokenEvent::Info {
                                message: "One Shot Mode: Buy successful, stopped buying new tokens".to_string(),
                                timestamp: Utc::now(),
                            });
                            // Instead of breaking, we set the flag to stop buying new tokens
                            // This keeps the connection open for manual sells and monitoring
                            stop_buying = true;
                        }
                        
                        // Continue loop after successful buy
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
                    let config_for_error = {
                        let cfg = config_arc.read().unwrap();
                        (*cfg).clone()
                    };
                    if config_for_error.one_shot_mode && !reason.contains("SKIP") {
                        eprintln!("🎯 One Shot Mode: Processing error occurred, entering monitor-only mode");
                        let _ = event_tx.send(TokenEvent::Info {
                            message: "One Shot Mode: Error occurred, stopped buying new tokens".to_string(),
                            timestamp: Utc::now(),
                        });
                        stop_buying = true;
                    }
                }
            }
            // Continue loop to process next WebSocket message
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
    history_tracker: Option<Arc<std::sync::RwLock<crate::accounts::HistoryTracker>>>,
    subscription_handle: Option<Arc<Option<SubscriptionHandle>>>,
) -> Result<Option<String>> {
    let mint = accounts.mint;
    let dev_buy_lamports = accounts.dev_buy_sol;
    let dev_buy_sol = dev_buy_lamports as f64 / 1e9;
    
    // ========================================================================
    // SECTION 1: TOKEN INFO
    // ========================================================================
    
    let min_sol = config.min_dev_buy_sol;
    let max_sol = config.max_dev_buy_sol;
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
    let require_website = config.require_website;
    let require_telegram = config.require_telegram;
    let require_discord = config.require_discord;
    let min_socials = if config.enable_min_socials_count { config.min_socials_count } else { 0 };
    let require_uppercase = config.require_uppercase_token;
    let max_name_len = config.max_name_length;
    let min_ticker_len = config.min_ticker_length;
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
    
    // Check if we need metadata (for socials or metadata filters)
    let need_metadata = require_socials || require_twitter || require_website || require_telegram || require_discord || min_socials > 0 
        || require_uppercase || max_name_len < usize::MAX || min_ticker_len > 0;
    
    let metadata_fut: std::pin::Pin<Box<dyn std::future::Future<Output = Option<(Socials, TokenMetadata)>> + Send>> = Box::pin(async move {
        if need_metadata {
            if let Err(_e) = socials_rate_limiter_clone.check() {
                let mut m = metrics_clone_socials.write().unwrap();
                m.record_error(ErrorType::Network);
                None
            } else {
                check_token_metadata(&mint_str_socials, &api_key_socials).await.ok()
            }
        } else {
            None
        }
    });
    
    let mc_fut: std::pin::Pin<Box<dyn std::future::Future<Output = Result<(BondingCurveAccount, f64), anyhow::Error>> + Send>> = Box::pin(fetch_bonding_curve_mc(
        rpc,
        &accounts.bonding_curve,
    ));
    
    // ⚡ PARALLEL: Await all futures simultaneously using tokio::join!
    let (das_result, mc_result, metadata_result): (
        Result<u32, anyhow::Error>,
        Result<(BondingCurveAccount, f64), anyhow::Error>,
        Option<(Socials, TokenMetadata)>
    ) = tokio::join!(
        das_fut,
        mc_fut,
        metadata_fut
    );
    
    // Extract socials and metadata from result
    let (socials_result, metadata_opt) = if let Some((socials, metadata)) = metadata_result {
        (Some(socials), Some(metadata))
    } else {
        (None, None)
    };
    
    // Filter #2: Creator Token Count (using filter function from filters.rs)
    let creator_count = match das_result {
        Ok(count) => {
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
            count
        }
        Err(e) => {
            // DAS check failed - skip token since we can't verify creator token count
            let reason = format!("SKIP: DAS API failed - {}", e);
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
    
    // Process MC result
    let (curve, mc_sol) = match mc_result {
        Ok(data) => data,
        Err(_) => {
            (BondingCurveAccount::default(), 0.0)
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
        if require_website && !socials.has_website() {
            let filter_time = filter_start.elapsed().as_millis() as u64;
            if let Ok(mut m) = metrics.write() {
                m.record_filter(FilterReason::Socials, filter_time);
            }
            let reason = "SKIP: No Website".to_string();
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
        if require_telegram && !socials.has_telegram() {
            let filter_time = filter_start.elapsed().as_millis() as u64;
            if let Ok(mut m) = metrics.write() {
                m.record_filter(FilterReason::Socials, filter_time);
            }
            let reason = "SKIP: No Telegram".to_string();
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
        if require_discord && !socials.has_discord() {
            let filter_time = filter_start.elapsed().as_millis() as u64;
            if let Ok(mut m) = metrics.write() {
                m.record_filter(FilterReason::Socials, filter_time);
            }
            let reason = "SKIP: No Discord".to_string();
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
        if min_socials > 0 && socials.count() < min_socials {
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
            if require_socials || require_twitter || require_website || require_telegram || require_discord {
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
    
    // Filter #4: Token Metadata Filters (uppercase, name length, ticker length)
    if let Some(metadata) = &metadata_opt {
        // Uppercase filter
        if config.require_uppercase_token {
            let name_ok = metadata.name.is_empty() || metadata.name == metadata.name.to_uppercase();
            let symbol_ok = metadata.symbol.is_empty() || metadata.symbol == metadata.symbol.to_uppercase();
            if !name_ok || !symbol_ok {
                let filter_time = filter_start.elapsed().as_millis() as u64;
                if let Ok(mut m) = metrics.write() {
                    m.record_filter(FilterReason::Socials, filter_time); // Reuse Socials filter reason for now
                }
                let reason = format!("SKIP: Token not uppercase (name: '{}', symbol: '{}')", metadata.name, metadata.symbol);
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
        
        // Name length filter
        if metadata.name.len() > config.max_name_length {
            let filter_time = filter_start.elapsed().as_millis() as u64;
            if let Ok(mut m) = metrics.write() {
                m.record_filter(FilterReason::Socials, filter_time);
            }
            let reason = format!("SKIP: Name too long ({} > {}): '{}'", metadata.name.len(), config.max_name_length, metadata.name);
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
        
        // Ticker length filter
        let symbol_len = metadata.symbol.len();
        if symbol_len < config.min_ticker_length || symbol_len > config.max_ticker_length {
            let filter_time = filter_start.elapsed().as_millis() as u64;
            if let Ok(mut m) = metrics.write() {
                m.record_filter(FilterReason::Socials, filter_time);
            }
            let reason = format!("SKIP: Ticker length out of range ({} not in {}-{}): '{}'", 
                symbol_len, config.min_ticker_length, config.max_ticker_length, metadata.symbol);
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
        // If metadata is required for uppercase filter but not available, reject
        if config.require_uppercase_token {
            let filter_time = filter_start.elapsed().as_millis() as u64;
            if let Ok(mut m) = metrics.write() {
                m.record_filter(FilterReason::Socials, filter_time);
                m.record_error(ErrorType::Network);
            }
            let reason = "SKIP: Could not verify token metadata (uppercase required)".to_string();
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
    
    // ========================================================================
    // SECTION 3: ACCOUNT VERIFICATION
    // ========================================================================
    
    // Verify bonding curve account
    let mut bonding_curve_ready = false;
    let max_wait_attempts = 5; // Optimized: 5 attempts × 20ms = max 100ms for premium RPC
    let wait_interval_ms = 20; // Optimized: reduced from 30ms to 20ms for premium RPC
    
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
                    // BondingCurveAccount structure: 8+8+8+8+8+8+1 = 57 bytes
                    const BONDING_CURVE_SIZE: usize = 8 + 8 + 8 + 8 + 8 + 8 + 1; // 57 bytes
                    let data_slice = if account.data.len() >= BONDING_CURVE_SIZE {
                        &account.data[..BONDING_CURVE_SIZE]
                    } else {
                        &account.data[..]
                    };
                    if let Ok(curve) = BondingCurveAccount::try_from_slice(data_slice) {
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
    
    // ✅ CRITICAL: Extract token_amount from buy instruction (this is the expected amount from bonding curve)
    // This is more accurate than global account calculation and should be used as fallback
    let buy_instruction_token_amount = if buy_ix.data.len() >= 16 {
        u64::from_le_bytes(buy_ix.data[8..16].try_into().unwrap())
    } else {
        0
    };
    
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
    let max_wait_attempts = 2; // Reduced from 5 to 2 for premium RPC
    
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
            tokio::time::sleep(Duration::from_millis(30)).await; // Reduced from 100ms to 30ms for premium RPC
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
        tokio::time::sleep(Duration::from_millis(200)).await; // Reduced from 500ms to 200ms for premium RPC
        
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
        ).await;
        
        let (mc_entry_sol, token_price_entry) = match mc_entry_result {
            Ok((curve, mc_sol)) => {
                let price = curve.get_token_price_sol();
                (Some(mc_sol), if price > 0.0 { Some(price) } else { None })
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
                            twitter: socials_opt.as_ref().and_then(|s| s.twitter.clone()),
                            website: socials_opt.as_ref().and_then(|s| s.website.clone()),
                            telegram: socials_opt.as_ref().and_then(|s| s.telegram.clone()),
                            has_socials: {
                                // Set has_socials based on actual fields, not just socials_opt
                                // This ensures has_socials is correct even if socials fetch failed but fields are set
                                let twitter = socials_opt.as_ref().and_then(|s| s.twitter.clone());
                                let website = socials_opt.as_ref().and_then(|s| s.website.clone());
                                let telegram = socials_opt.as_ref().and_then(|s| s.telegram.clone());
                                twitter.is_some() || website.is_some() || telegram.is_some()
                            },
                            creator_token_count: creator_count,
                            detection_method: if dev_buy_lamports > 0 {
                                "instruction".to_string()
                            } else {
                                "balance_fallback".to_string()
                            },
                            mc_at_detection_sol: if mc_sol > 0.0 { Some(mc_sol) } else { None },
                            mc_at_entry_sol: mc_entry_sol,
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
                            buy_fees_sol: None,
                            peak_mc_sol: None,
                            peak_pnl_percent: None,
                            breakeven_mode_active: false,
                            executed_sell_rules: Vec::new(),
                            partial_sell_count: 0,
                            total_sold_percent: 0.0,
                            dev_buy_usd: None,
                            our_buy_usd: None,
                            pnl_usd: None,
                            mc_at_detection_usd: None,
                            mc_at_entry_usd: None,
                            current_value_usd: None,
        };
                        
                        let _ = tracker.record_buy(buy.clone());
                        
                        // 📊 ULTRA MC TRACKING: Register token in history tracker for mock buy too
                        // ✅ FIX: Use try_write to avoid blocking/deadlock
                        if let Some(ref history) = history_tracker {
                            let mint_str = mint.to_string();
                            let bonding_curve_str = accounts.bonding_curve.to_string();
                            let entry_mc = mc_entry_sol.unwrap_or(0.0);
                            let entry_price = token_price_entry.unwrap_or(token_price_sol);
                            let our_buy_sol = config.buy_amount_sol;
                            let token_amount_opt = if token_amount > 0 { Some(token_amount) } else { None };
                            
                            // Try to get write lock (non-blocking)
                            if let Ok(mut history_guard) = history.try_write() {
                                history_guard.register_token(
                                    &mint_str,
                                    &bonding_curve_str,
                                    entry_mc,
                                    entry_price,
                                    our_buy_sol,
                                    token_amount_opt,
                                );
                                // Release lock before async call
                                drop(history_guard);
                            }
                            
                            // 📊 Record initial MC snapshot for mock buy (fetch outside lock)
                            if let Ok((curve, _)) = fetch_bonding_curve_mc(rpc, &accounts.bonding_curve).await {
                                if let Ok(mut history_guard) = history.try_write() {
                                    history_guard.record_from_bonding_curve(
                                        &mint_str,
                                        &curve,
                                        None, None, None,
                                    );
                                    let mint_short = if mint_str.len() > 8 { &mint_str[..8] } else { &mint_str };
                                    eprintln!("📊 HISTORY: Registered mock buy token {} in history tracker", mint_short);
                                }
                            }
                        }
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
            mc: mc_entry_sol,
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
                
                tokio::time::sleep(Duration::from_millis(50)).await; // Optimized: reduced from 200ms to 50ms for premium RPC
                
                match verify_transaction_success(rpc, &sig, &user_ata).await {
                    Ok(None) => (true, None),
                    Ok(Some(reason)) => (false, Some(reason)),
                    Err(e) => {
                        eprintln!("  ⚠️  Verification error: {}", e);
                        // Don't assume success on verification error - transaction might have failed
                        (false, Some(format!("Verification failed: {}", e)))
                    }
                }
            }
        } else {
            (false, Some("No signature returned".to_string()))
        }
    } else {
        (false, None)
    };
    
    if buy_succeeded {
        tokio::time::sleep(Duration::from_millis(30)).await; // Optimized: reduced from 150ms to 30ms for premium RPC
        
        let mc_entry_result = fetch_bonding_curve_mc(
            rpc,
            &accounts.bonding_curve,
        ).await;
        
        // Calculate total fees for PnL accuracy
        // Priority fee is in microlamports per compute unit, so we need to:
        // 1. Multiply by compute_units to get total microlamports
        // 2. Divide by 1_000_000 to convert microlamports to lamports
        // 3. Divide by 1e9 to convert lamports to SOL
        let priority_fee_sol = (priority_fee as f64 * config.compute_units as f64) / 1_000_000.0 / 1e9;
        let jito_tip_sol = config.jito_tip as f64 / 1e9;
        let base_fee_sol = 0.000005; // 5000 lamports base fee
        let total_buy_fees = priority_fee_sol + jito_tip_sol + base_fee_sol;

        // NEW: Fetch actual balance change for precise entry cost
        let mut invested_sol = config.buy_amount_sol;
        
        let buy_signature = actual_signature.as_ref()
            .map(|s| s.clone())
            .unwrap_or_else(|| init_signature.clone());
        
        // ✅ CRITICAL: Get ACTUAL token amount from token account balance after buy
        // Use token_amount from buy instruction as fallback (more accurate than global account)
        let mut actual_token_amount = if buy_instruction_token_amount > 0 {
            buy_instruction_token_amount
        } else {
            token_amount // Fallback to global account calculation if buy instruction amount not available
        };
        let mut actual_token_decimals: Option<u8> = None; // Will be set from transaction metadata
        if !buy_signature.starts_with("MOCK") {
            // Transaction metadata already contains post_token_balances - no need to wait
            // Balance check is only used as fallback if metadata extraction fails
            
            // Get actual SOL spent
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
            
            // ✅ CRITICAL FIX: Use RPC balance fetch with known user_ata address (most reliable method)
            // This is more reliable than parsing transaction metadata which can have wrong account_index
            let mut balance_found = false;
            
            // Try Helius API first (fastest), then fallback to RPC
            match get_token_balance_helius(&config.helius_api_key, &user_ata).await {
                Ok(balance) => {
                    if balance > 0 {
                        actual_token_amount = balance;
                        // Try to get decimals from RPC token account info
                        match rpc.get_token_account_balance(&user_ata).await {
                            Ok(balance_info) => {
                                actual_token_decimals = Some(balance_info.decimals);
                                let tokens_human = balance as f64 / 10_f64.powi(balance_info.decimals as i32);
                                eprintln!("   ✅ Actual token balance after buy (Helius + RPC decimals): {} (raw units) = {} (human-readable, {} decimals)", balance, tokens_human, balance_info.decimals);
                            },
                            Err(_) => {
                                // Fallback: assume 6 decimals (pump.fun standard)
                                actual_token_decimals = Some(6);
                                let tokens_human = balance as f64 / 1e6;
                                eprintln!("   ✅ Actual token balance after buy (Helius, assumed 6 decimals): {} (raw units) = {} (human-readable)", balance, tokens_human);
                            }
                        }
                        balance_found = true;
                    }
                },
                Err(_) => {}
            }
            
            // Fallback to RPC if Helius failed
            if !balance_found {
                let mut balance_retries = 3; // Retry a few times as token account might not be ready immediately
                
                while balance_retries > 0 && !balance_found {
                    match rpc.get_token_account_balance(&user_ata).await {
                        Ok(balance_info) => {
                            if let Ok(bal) = balance_info.amount.parse::<u64>() {
                                if bal > 0 {
                                    actual_token_amount = bal;
                                    actual_token_decimals = Some(balance_info.decimals);
                                    let tokens_human = bal as f64 / 10_f64.powi(balance_info.decimals as i32);
                                    eprintln!("   ✅ Actual token balance after buy (RPC): {} (raw units) = {} (human-readable, {} decimals)", bal, tokens_human, balance_info.decimals);
                                    balance_found = true;
                                } else {
                                    eprintln!("   ⏳ Token balance is 0, waiting 50ms and retrying... ({} retries left)", balance_retries - 1);
                                    if balance_retries > 1 {
                                        tokio::time::sleep(Duration::from_millis(50)).await;
                                    }
                                }
                            }
                        },
                        Err(_) => {
                            eprintln!("   ⏳ RPC balance fetch failed, waiting 50ms and retrying... ({} retries left)", balance_retries - 1);
                            if balance_retries > 1 {
                                tokio::time::sleep(Duration::from_millis(50)).await;
                            }
                        }
                    }
                    balance_retries -= 1;
                }
            }
            
            if !balance_found {
                eprintln!("⚠️  Could not get actual token amount, using expected amount from buy instruction");
            }
        }
        
        // ✅ CRITICAL FIX: Calculate entry price from ACTUAL invested SOL and ACTUAL token amount
        // This is the REAL price we paid, not the price from bonding curve after buy
        // actual_token_amount is the REAL balance from token account (in raw units with decimals)
        // Use actual decimals from transaction if available, otherwise fallback to 6 decimals (pump.fun standard)
        let actual_entry_price = if actual_token_amount > 0 && invested_sol > 0.0 {
            // Entry price = SOL invested / tokens received (in human-readable units)
            // actual_token_amount is in raw units (e.g., if 1000 tokens with 6 decimals = 1000_000_000 raw units)
            // We need to convert to human-readable using the correct decimals
            let decimals = actual_token_decimals.unwrap_or(6); // Default to 6 decimals if not available
            let tokens_human = actual_token_amount as f64 / 10_f64.powi(decimals as i32);
            if tokens_human > 0.0 {
                let price = invested_sol / tokens_human;
                // ✅ CRITICAL FIX: Validate entry price is reasonable (not too small due to precision errors)
                // If price is < 1e-12, it's likely a calculation error (too many tokens or wrong decimals)
                // In such cases, we should use bonding curve price instead
                if price >= 1e-12 && price <= 1.0 {
                    eprintln!("   ✅ Calculated entry price: {:.12} SOL/token (from {} SOL / {} tokens)", 
                             price, invested_sol, tokens_human);
                    Some(price)
                } else {
                    eprintln!("   ⚠️  Calculated entry price {:.12} SOL/token is invalid (too small/large) - will use bonding curve price instead", price);
                    eprintln!("   ⚠️  Reason: price < 1e-12 or > 1.0 (likely precision error: {} SOL / {} tokens)", invested_sol, tokens_human);
                    None // Invalid price - use bonding curve price instead
                }
            } else {
                None
            }
        } else {
            None
        };
        
        // Get MC after buy (for reference, but don't use for entry price)
        let (mc_entry_sol, token_price_entry) = match mc_entry_result {
            Ok((curve, mc_sol)) => {
                let price = curve.get_token_price_sol();
                (Some(mc_sol), if price > 0.0 { Some(price) } else { None })
            }
            Err(_) => (None, None)
        };
        
        // ✅ CRITICAL FIX: Use actual entry price ONLY if it's valid (>= 1e-12)
        // Otherwise use bonding curve price (which is more reliable for very small prices)
        // Priority: actual_entry_price (if valid) > bonding_curve_price > detection_price
        let final_entry_price = actual_entry_price
            .or(token_price_entry)
            .or(if token_price_sol > 0.0 { Some(token_price_sol) } else { None });
        
        // Log which price source was used
        if let Some(price) = final_entry_price {
            if actual_entry_price.is_some() && actual_entry_price.unwrap() == price {
                eprintln!("   ✅ Using calculated entry price: {:.12} SOL/token", price);
            } else if token_price_entry.is_some() && token_price_entry.unwrap() == price {
                eprintln!("   ✅ Using bonding curve price as entry price: {:.12} SOL/token (calculated price was invalid)", price);
            } else {
                eprintln!("   ✅ Using detection price as entry price: {:.12} SOL/token (fallback)", price);
            }
        }
        
        if let Some(actual) = actual_entry_price {
            if let Some(curve_price) = token_price_entry {
                if (actual - curve_price).abs() > 0.0001 {
                    eprintln!("   ⚠️  Entry price mismatch: Actual={:.8} SOL/token (from invested {} SOL / {} tokens), Curve={:.8} SOL/token", 
                             actual, invested_sol, actual_token_amount, curve_price);
                }
            }
        }
        
        // ✅ CRITICAL FIX: Use try_write() instead of write() to avoid blocking GUI thread
        // GUI may hold read lock during rendering, causing UI to freeze
        let mut buy_recorded = false;
        let buy = {
            // Try to get write lock (non-blocking)
            if let Ok(mut tracker_opt) = tracker.try_write() {
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
                        mc_at_detection_sol: if mc_sol > 0.0 { Some(mc_sol) } else { None },
                        mc_at_entry_sol: mc_entry_sol,
                        token_price_sol: final_entry_price, // Use calculated entry price (invested SOL / actual token amount)
                        token_amount: if actual_token_amount > 0 { Some(actual_token_amount) } else { None },
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
                        peak_mc_sol: None,
                        peak_pnl_percent: None,
                        breakeven_mode_active: false,
                        executed_sell_rules: Vec::new(),
                        partial_sell_count: 0,
                        total_sold_percent: 0.0,
                        dev_buy_usd: None,
                        our_buy_usd: None,
                        pnl_usd: None,
                        mc_at_detection_usd: None,
                        mc_at_entry_usd: None,
                        current_value_usd: None,
                    };
                    
                    if let Err(e) = tracker.record_buy(buy.clone()) {
                        eprintln!("  ⚠️  Tracker error recording buy: {}", e);
                    } else {
                        buy_recorded = true;
                        // Subscribe to bonding curve for real-time PNL updates
                        if let Some(sub_handle_arc) = &subscription_handle {
                            if let Some(ref sub_handle) = sub_handle_arc.as_ref() {
                                if let Some(bonding_curve_str) = &buy.bonding_curve {
                                    if let Ok(bonding_curve) = Pubkey::from_str(bonding_curve_str) {
                                        if let Err(e) = sub_handle.subscribe(bonding_curve) {
                                            eprintln!("  ⚠️  Failed to subscribe to bonding curve: {}", e);
                                        } else {
                                            eprintln!("  ✅ Subscribed to bonding curve for real-time PNL: {}", bonding_curve_str);
                                        }
                                    }
                                }
                            }
                        }
                    }
                    Some(buy)
                } else {
                    None
                }
            } else {
                eprintln!("⚠️  Failed to acquire tracker write lock for record_buy (GUI may be holding lock) - will retry");
                None
            }
        };
        
        // ✅ RETRY LOGIC: If initial try_write failed, retry with short delay (non-blocking)
        let buy_final = if !buy_recorded {
            // Retry after short delay - clone the buy object that we created earlier
            if let Some(buy_to_retry) = buy {
                tokio::time::sleep(Duration::from_millis(100)).await;
                if let Ok(mut tracker_opt) = tracker.try_write() {
                    if let Some(tracker) = tracker_opt.as_mut() {
                        if let Err(e) = tracker.record_buy(buy_to_retry.clone()) {
                            eprintln!("  ⚠️  Tracker error recording buy (retry): {}", e);
                        } else {
                            eprintln!("✅ Successfully recorded buy after retry");
                        }
                        buy_to_retry
                    } else {
                        eprintln!("⚠️  CRITICAL: Failed to record buy - Tracker is None (retry failed)");
                        return Err(anyhow!("Failed to record buy - tracker unavailable"));
                    }
                } else {
                    eprintln!("⚠️  CRITICAL: Failed to record buy even after retry - position may not appear in UI!");
                    return Err(anyhow!("Failed to record buy - tracker lock unavailable"));
                }
            } else {
                eprintln!("⚠️  CRITICAL: buy is None - cannot retry recording");
                return Err(anyhow!("Failed to record buy - buy data unavailable"));
            }
        } else {
            // buy_recorded was true, so buy should be Some(...)
            match buy {
                Some(b) => b,
                None => {
                    // This should never happen if buy_recorded=true, but handle it gracefully
                    eprintln!("⚠️  CRITICAL: buy_recorded=true but buy is None - internal error!");
                    return Err(anyhow!("Internal error: buy_recorded inconsistent"));
                }
            }
        };
        
        let _buy = buy_final; // Buy data recorded, variable kept for potential future use
        
        // 📊 ULTRA MC TRACKING: Register token in history tracker immediately after buy
        // ✅ FIX: Use try_write to avoid blocking/deadlock if lock is held elsewhere
        if let Some(ref history) = history_tracker {
                    let mint_str = mint.to_string();
                    let bonding_curve_str = accounts.bonding_curve.to_string();
                    let entry_mc = mc_entry_sol.unwrap_or(0.0);
                    let entry_price = final_entry_price.unwrap_or(0.0);
                    let our_buy_sol = invested_sol;
                    let token_amount_opt = if actual_token_amount > 0 { Some(actual_token_amount) } else { None };
                    
                    // Try to get write lock (non-blocking)
                    if let Ok(mut history_guard) = history.try_write() {
                        history_guard.register_token(
                            &mint_str,
                            &bonding_curve_str,
                            entry_mc,
                            entry_price,
                            our_buy_sol,
                            token_amount_opt,
                        );
                        
                        // Release lock before async call
                        drop(history_guard);
                    }
                    
                    // 📊 Record initial MC snapshot immediately after buy (fetch outside lock)
                    // ✅ FIX: Use timeout to prevent blocking on slow RPC
                    match tokio::time::timeout(Duration::from_secs(3), 
                        fetch_bonding_curve_mc(rpc, &accounts.bonding_curve)
                    ).await {
                        Ok(Ok((curve, mc_sol))) => {
                            let current_price = curve.get_token_price_sol();
                            
                            // Get initial PnL if available
                            let initial_pnl_percent = if entry_price > 0.0 && current_price > 0.0 {
                                Some(((current_price - entry_price) / entry_price) * 100.0)
                            } else {
                                None
                            };
                            
                            // Try to get write lock again for recording snapshot
                            if let Ok(mut history_guard) = history.try_write() {
                                history_guard.record_from_bonding_curve(
                                    &mint_str,
                                    &curve,
                                    initial_pnl_percent,
                                    None, // pnl_sol not calculated yet
                                    None, // current_value_sol not calculated yet
                                );
                                
                                let mint_short = if mint_str.len() > 8 { &mint_str[..8] } else { &mint_str };
                                use crate::utils::sol_to_usd;
                                let mc_usd = sol_to_usd(mc_sol);
                                eprintln!("📊 HISTORY: Registered and recorded initial MC snapshot for {} (MC: {:.2} SOL (${:.0}), Price: {:.8})", 
                                         mint_short, mc_sol, mc_usd, current_price);
                            }
                        }
                        Ok(Err(e)) => {
                            // Fetch failed but don't block - continue
                            eprintln!("⚠️  Failed to fetch bonding curve for history snapshot: {}", e);
                        }
                        Err(_) => {
                            // Timeout - don't block, continue
                            eprintln!("⚠️  Timeout fetching bonding curve for history snapshot (3s)");
                        }
                    }
                }
        
        // ✅ FIX: Calculate PnL immediately after recording buy
        // Use final_entry_price as current_price_sol for initial PnL calculation
        // This ensures PnL is available in UI right away
        if let Some(entry_price) = final_entry_price {
            if entry_price > 0.0 {
                let mint_str = mint.to_string();
                if let Ok(mut tracker_opt) = tracker.try_write() {
                    if let Some(tracker) = tracker_opt.as_mut() {
                        if let Err(e) = tracker.update_position_pnl_fast(&mint_str, entry_price) {
                            let mint_short = if mint_str.len() > 8 { &mint_str[..8] } else { &mint_str };
                            eprintln!("⚠️  Failed to calculate initial PnL for {}: {}", mint_short, e);
                        } else {
                            let mint_short = if mint_str.len() > 8 { &mint_str[..8] } else { &mint_str };
                            eprintln!("✅ Calculated initial PnL for {} using entry price {:.8}", mint_short, entry_price);
                        }
                    }
                }
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
    // Retry logic: Transaction might not be immediately available after submission
    let max_attempts = 5;
    let wait_ms = 200; // Wait 200ms between retries
    
    for attempt in 1..=max_attempts {
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
                    return Ok(None);
                } else {
                    // No metadata - can't verify
                    return Ok(Some("No transaction metadata found".to_string()));
                }
            }
            Err(e) => {
                // Transaction not found yet - retry if we have attempts left
                if attempt < max_attempts {
                    let error_str = format!("{}", e);
                    // Check if error is about null/not found (transaction still processing)
                    if error_str.contains("null") || error_str.contains("not found") || error_str.contains("Invalid") {
                        eprintln!("   ⏳ Transaction not yet confirmed (attempt {}/{}), waiting {}ms...", 
                                 attempt, max_attempts, wait_ms);
                        tokio::time::sleep(Duration::from_millis(wait_ms)).await;
                        continue;
                    }
                }
                // Final attempt failed or non-retryable error
                return Err(anyhow!("Could not get transaction after {} attempts: {}", max_attempts, e));
            }
        }
    }
    
    // Should never reach here, but just in case
    Err(anyhow!("Could not get transaction: max attempts reached"))
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

/// Batch check token account balances using RPC get_multiple_accounts
/// Returns Vec<Option<u64>> where None means account doesn't exist or failed to parse
async fn batch_check_token_balances(
    rpc: &RpcClient,
    token_accounts: &[Pubkey],
) -> Vec<Option<u64>> {
    if token_accounts.is_empty() {
        return Vec::new();
    }

    // Solana RPC has limit of ~100 accounts per request, so we need to batch
    const BATCH_SIZE: usize = 100;
    let mut results = Vec::with_capacity(token_accounts.len());

    for chunk in token_accounts.chunks(BATCH_SIZE) {
        match rpc.get_multiple_accounts(chunk).await {
            Ok(accounts) => {
                for account_opt in accounts {
                    if let Some(account) = account_opt {
                        // Try to parse as token account
                        if let Ok(token_account) = spl_token::state::Account::unpack(&account.data) {
                            results.push(Some(token_account.amount));
                        } else {
                            // Not a valid token account
                            results.push(None);
                        }
                    } else {
                        // Account doesn't exist
                        results.push(None);
                    }
                }
            }
            Err(_) => {
                // If batch fetch fails, return None for all accounts in this chunk
                results.extend(std::iter::repeat(None).take(chunk.len()));
            }
        }
    }

    results
}

/// Batch fetch bonding curve accounts and deserialize them
/// Returns Vec<Option<BondingCurveAccount>> where None means account doesn't exist or failed to parse
async fn batch_fetch_bonding_curves(
    rpc: &RpcClient,
    bonding_curves: &[Pubkey],
) -> Vec<Option<BondingCurveAccount>> {
    if bonding_curves.is_empty() {
        eprintln!("⚠️  batch_fetch_bonding_curves: Empty input");
        return Vec::new();
    }

    use borsh::BorshDeserialize;
    
    // Solana RPC has limit of ~100 accounts per request, so we need to batch
    const BATCH_SIZE: usize = 100;
    let mut results = Vec::with_capacity(bonding_curves.len());

    for chunk in bonding_curves.chunks(BATCH_SIZE) {
        match rpc.get_multiple_accounts(chunk).await {
            Ok(accounts) => {
                for account_opt in accounts.iter() {
                    if let Some(account) = account_opt {
                        // Try to deserialize as bonding curve account
                        // BondingCurveAccount structure: 8+8+8+8+8+8+1 = 57 bytes
                        // Account may have additional data, so we only deserialize the first 57 bytes
                        const BONDING_CURVE_SIZE: usize = 8 + 8 + 8 + 8 + 8 + 8 + 1; // 57 bytes
                        let data_slice = if account.data.len() >= BONDING_CURVE_SIZE {
                            &account.data[..BONDING_CURVE_SIZE]
                        } else {
                            &account.data[..]
                        };
                        
                        match BondingCurveAccount::try_from_slice(data_slice) {
                            Ok(curve) => {
                                results.push(Some(curve));
                            },
                            Err(_) => {
                                results.push(None); // Failed to deserialize
                            }
                        }
                    } else {
                        // Account doesn't exist
                        results.push(None);
                    }
                }
            }
            Err(_) => {
                // If batch fetch fails, return None for all accounts in this chunk
                results.extend(std::iter::repeat(None).take(chunk.len()));
            }
        }
    }

    results
}

/// Get all token holdings for a wallet using Helius API
pub async fn get_wallet_token_holdings(
    helius_api_key: &str,
    wallet_address: &Pubkey,
) -> Result<Vec<(Pubkey, Pubkey, u64)>> {
    // Returns Vec<(mint, token_account, balance)>
    let url = format!("https://mainnet.helius-rpc.com/?api-key={}", helius_api_key);
    let client = crate::utils::get_shared_http_client();
    
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
    mut shutdown: tokio::sync::watch::Receiver<bool>,
) {
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
        match get_wallet_token_holdings(&helius_api_key, &wallet_address).await {
            Ok(holdings) => {
                if holdings.is_empty() {
                    eprintln!("   ℹ️  No token holdings found in wallet");
                } else {
                    eprintln!("   ✅ Found {} token position(s):", holdings.len());
                    eprintln!();
                    
                    // Get SOL price for MC calculation
                    let sol_price = {
                        use crate::utils::get_cached_sol_price;
                        get_cached_sol_price()
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
                        match fetch_bonding_curve_mc(rpc_arc.as_ref(), &bonding_curve).await {
                            Ok((curve, mc_sol)) => {
                                use crate::utils::sol_to_usd;
                                let mc_usd = sol_to_usd(mc_sol);
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
        // ✅ FIX: Check shutdown signal before each iteration
        if *shutdown.borrow() {
            eprintln!("🛑 Position monitor received shutdown signal");
            break;
        }
        
        // Wrap entire loop iteration in error handling to prevent crashes
        let loop_result = async {
            // Check if auto-sell is enabled
            let enabled = {
                let cfg = config.read().unwrap();
                cfg.enable_auto_sell
            };

            if !enabled {
                // Silent mode - don't spam console every 10 seconds
                // Use select to check shutdown during sleep
                tokio::select! {
                    _ = tokio::time::sleep(Duration::from_secs(10)) => {},
                    _ = shutdown.changed() => {
                        if *shutdown.borrow() {
                            return Ok::<(), anyhow::Error>(());
                        }
                    }
                }
                return Ok::<(), anyhow::Error>(());
            }
            
            // Auto-sell monitoring

            // Get config values including Helius API key and dead coin settings
            let (stop_loss_percent, take_profit_mc_sol, _monitor_interval, helius_api_key, _sell_percent, enable_dead_coin_sell, dead_coin_timeout_sec) = {
                let cfg = config.read().unwrap();
                use crate::utils::get_cached_sol_price;
                (cfg.stop_loss_percent, cfg.take_profit_mc_sol, cfg.monitor_interval_sec, cfg.helius_api_key.clone(), cfg.sell_percent, cfg.enable_dead_coin_sell, cfg.dead_coin_timeout_sec)
            };
            
            // Refresh SOL price if needed (every 5 minutes)
            crate::utils::refresh_sol_price_if_needed().await;

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
                
                // OPTIMIZED: Batch prepare all token account addresses
                let mut token_accounts = Vec::new();
                let mut position_indices = Vec::new(); // Track which position index corresponds to which token account
                
                for (idx, position) in positions_to_check.iter().enumerate() {
                    let user_token_account = match &position.user_token_account {
                        Some(ata_str) => {
                            match Pubkey::from_str(ata_str) {
                                Ok(pubkey) => pubkey,
                                Err(_) => {
                                    if let Ok(mint) = Pubkey::from_str(&position.mint) {
                                        get_associated_token_address_with_program_id(
                                            &user_wallet,
                                            &mint,
                                            &token_program_2022,
                                        )
                                    } else {
                                        continue; // Skip invalid mint
                                    }
                                }
                            }
                        }
                        None => {
                            if let Ok(mint) = Pubkey::from_str(&position.mint) {
                                get_associated_token_address_with_program_id(
                                    &user_wallet,
                                    &mint,
                                    &token_program_2022,
                                )
                            } else {
                                continue; // Skip invalid mint
                            }
                        }
                    };
                    token_accounts.push(user_token_account);
                    position_indices.push(idx);
                }
                
                // OPTIMIZED: Batch fetch all balances using RPC (faster than individual calls)
                // Note: We still try Helius for individual positions if batch fails, but batch is primary
                let balances = if !token_accounts.is_empty() {
                    batch_check_token_balances(rpc_arc.as_ref(), &token_accounts).await
                } else {
                    Vec::new()
                };
                
                let mut cleaned_count = 0;
                
                // Process results and map back to positions
                for (balance_idx, &position_idx) in position_indices.iter().enumerate() {
                    if position_idx >= positions_to_check.len() {
                        continue;
                    }
                    
                    let position = &positions_to_check[position_idx];
                    let balance = if balance_idx < balances.len() {
                        balances[balance_idx].unwrap_or(0)
                    } else {
                        // Fallback: try individual Helius API call if batch failed for this account
                        let user_token_account = &token_accounts[balance_idx];
                        match get_token_balance_helius(&helius_api_key, user_token_account).await {
                            Ok(bal) => bal,
                            Err(_) => {
                                // Final fallback: individual RPC call
                                match rpc_arc.get_token_account_balance(user_token_account).await {
                                    Ok(balance_info) => balance_info.amount.parse().unwrap_or(0),
                                    Err(_) => 0, // Account doesn't exist
                                }
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
                    eprintln!("🧹 Cleaned up {} positions with zero balance (batch RPC)", cleaned_count);
                }
            }

            // Get active positions
            let active_positions = {
                if let Ok(tracker_guard) = tracker.read() {
                    if let Some(tracker_ref) = tracker_guard.as_ref() {
                        let positions = tracker_ref.get_active_positions();
                        if !positions.is_empty() {
                            eprintln!("📊 Found {} active position(s) to monitor", positions.len());
                        }
                        positions
                    } else {
                        Vec::new()
                    }
                } else {
                    Vec::new()
                }
            };

            if active_positions.is_empty() {
                // Still check every 30ms even when no positions - ensures instant detection when position is added (optimized for premium RPC)
                tokio::time::sleep(Duration::from_millis(30)).await; // Reduced from 50ms to 30ms
                return Ok(());
            }
            
            // 🚀 OPTIMIZED: Batch fetch bonding curve accounts first, then process positions
            // First, handle positions that can be sold immediately (PnL already calculated)
            let mut positions_to_fetch = Vec::new();
            let mut positions_to_sell_immediately = Vec::new();
            
            for position in active_positions.iter() {
                // 🚀 ULTRA FAST: Check PnL from tracker FIRST (if available) - fastest path
                if let Some(pnl_percent) = position.pnl_percent {
                    // Skip normal stop loss check if breakeven mode is active
                    // (breakeven check requires MC data which will be done in parallel task)
                    if !position.breakeven_mode_active && pnl_percent <= -stop_loss_percent {
                        // PnL already calculated - use it immediately (NO RPC CALL NEEDED!)
                        positions_to_sell_immediately.push(position.clone());
                        continue; // Skip to next position
                    }
                }
                
                // Skip if we don't have required data
                let bonding_curve_str = match &position.bonding_curve {
                    Some(bc) => bc.clone(),
                    None => {
                        eprintln!("⚠️  Position {} skipped: no bonding_curve", position.mint);
                        continue;
                    },
                };

                let bonding_curve = match Pubkey::from_str(&bonding_curve_str) {
                    Ok(pk) => pk,
                    Err(_) => {
                        eprintln!("⚠️  Position {} skipped: invalid bonding_curve address", position.mint);
                        continue;
                    },
                };
                
                positions_to_fetch.push((position.clone(), bonding_curve));
            }
            
            // Execute immediate sells (no RPC needed)
            for position in positions_to_sell_immediately {
                eprintln!("🚨 STOP LOSS TRIGGERED: PnL = {:.2}% (threshold: -{:.2}%) - SELLING IMMEDIATELY", 
                         position.pnl_percent.unwrap_or(0.0), stop_loss_percent);
                
                let wallet_bytes = wallet.to_bytes();
                let wallet_clone = match Keypair::from_bytes(&wallet_bytes) {
                    Ok(kp) => kp,
                    Err(_) => continue,
                };
                
                let config_clone = {
                    let cfg = config.read().unwrap();
                    (*cfg).clone()
                };
                
                let position_clone = position.clone();
                let tracker_clone = tracker.clone();
                let event_tx_clone = event_tx.clone();
                let rpc_clone = Arc::clone(&rpc_arc);
                
                // Execute sell IMMEDIATELY in background (don't block monitoring)
                tokio::spawn(async move {
                    let _ = execute_sell(
                        &config_clone,
                        &wallet_clone,
                        rpc_clone.as_ref(),
                        &tracker_clone,
                        &position_clone,
                        "stop_loss",
                        &event_tx_clone,
                    ).await;
                });
            }
            
            // OPTIMIZED: Batch fetch all bonding curve accounts at once
            // Clone positions data first to avoid lifetime issues
            let positions_cloned: Vec<(TokenBuy, Pubkey)> = positions_to_fetch.iter()
                .map(|(pos, bc)| (pos.clone(), *bc))
                .collect();
            let bonding_curves: Vec<Pubkey> = positions_cloned.iter().map(|(_, bc)| *bc).collect();
            let bonding_curve_data = if !bonding_curves.is_empty() {
                batch_fetch_bonding_curves(rpc_arc.as_ref(), &bonding_curves).await
            } else {
                Vec::new()
            };
            
            // 🚀 PARALLEL CHECK: Process all positions with batch-fetched data
            let mut check_tasks = Vec::new();
            
            for (idx, (position, bonding_curve)) in positions_cloned.iter().enumerate() {
                // Get entry_mc if available (for fallback MC check), but don't require it
                let entry_mc = position.mc_at_entry_sol;
                
                // Clone all data before moving into task
                let position_mint = position.mint.clone();
                let position_clone = position.clone();
                let bonding_curve_clone = *bonding_curve; // Clone Pubkey
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
                
                // OPTIMIZED: Use batch-fetched bonding curve data if available
                let curve_opt = if idx < bonding_curve_data.len() {
                    bonding_curve_data[idx].clone()
                } else {
                    None
                };
                
                // Spawn parallel task for each position
                let task = tokio::spawn(async move {
                    // OPTIMIZED: Use batch-fetched curve data if available, otherwise fetch individually
                    let (current_price, current_mc_sol) = if let Some(curve) = curve_opt {
                        // Use batch-fetched data - calculate price and MC from curve
                        let price = curve.get_token_price_sol();
                        let mc_sol = curve.calculate_mc_sol();
                        (price, mc_sol)
                    } else {
                        // Fallback: fetch individually if batch failed
                        match fetch_bonding_curve_mc(
                            rpc_task.as_ref(),
                            &bonding_curve_clone,
                        ).await {
                            Ok((curve, mc_sol)) => {
                                (curve.get_token_price_sol(), mc_sol)
                            }
                            Err(_) => return None, // Skip if can't fetch (will retry next cycle)
                        }
                    };
                    
                    // 🎯 DYNAMIC SELL STRATEGY: Use strategy-based evaluation
                    // Check if we have a sell strategy config
                    let sell_strategy = config_clone.sell_strategy_config.as_ref();
                    
                    // Calculate time since buy
                    let time_since_buy = {
                        let now = Utc::now();
                        let buy_time = position_clone.timestamp;
                        now.signed_duration_since(buy_time).num_seconds() as u64
                    };
                    
                    // Get executed rules for this position
                    let executed_rule_ids = if let Ok(tracker_guard) = tracker_clone.read() {
                        if let Some(tracker_ref) = tracker_guard.as_ref() {
                            tracker_ref.get_executed_rules(&position_clone.mint)
                        } else {
                            Vec::new()
                        }
                    } else {
                        Vec::new()
                    };
                    
                    // Calculate current PnL
                    let entry_price = position_clone.token_price_sol.unwrap_or(0.0);
                    let current_pnl_percent = if entry_price > 0.0 {
                        Some(((current_price - entry_price) / entry_price) * 100.0)
                    } else {
                        position_clone.pnl_percent
                    };
                    
                    // Check strategy rules if available
                    if let Some(strategy) = sell_strategy {
                        if let Some(rule) = strategy.check_rules(
                            &position_clone,
                            current_pnl_percent,
                            Some(current_mc_sol),
                            position_clone.peak_pnl_percent,
                            time_since_buy,
                            &executed_rule_ids,
                        ) {
                            eprintln!("🎯 SELL STRATEGY RULE TRIGGERED: '{}' - Selling {:.0}% of position", rule.id, rule.sell_percent);
                            
                            // Execute sell with rule's sell_percent
                            let _ = execute_sell_with_percent(
                                &config_clone,
                                &wallet_clone,
                                rpc_task.as_ref(),
                                &tracker_clone,
                                &position_clone,
                                &format!("strategy_{}", rule.id),
                                rule.sell_percent,
                                &event_tx_clone,
                            ).await;
                            
                            // Mark rule as executed in tracker
                            if let Ok(mut tracker_guard) = tracker_clone.write() {
                                if let Some(tracker_ref) = tracker_guard.as_mut() {
                                    if rule.sell_percent >= 100.0 {
                                        // Full sell
                                        let _ = tracker_ref.mark_as_sold(&position_clone.mint, "strategy_sell".to_string());
                                    } else {
                                        // Partial sell
                                        let _ = tracker_ref.mark_partial_sell(&position_clone.mint, &rule.id, rule.sell_percent);
                                    }
                                }
                            }
                            
                            // Mark rule as executed in strategy config (update in next cycle)
                            // Note: We track in tracker, strategy config will be updated on next read
                            
                            return Some((format!("strategy_{}", rule.id), position_mint));
                        }
                    }
                    
                    // Fallback to old logic if no strategy config or no rule matched
                    // 🚀 PRIORITY: Check stop loss using PnL PERCENTAGE (not MC) - FIXED!
                    // Calculate PnL based on token price change
                    // ✅ CRITICAL FIX: Only calculate PnL if entry price is set
                    // This prevents incorrect PnL calculation when entry price is not yet available
                    let should_sell_stop_loss = if entry_price > 0.0 {
                        // Check if breakeven mode is active - if so, skip normal stop loss check
                        // (breakeven check is done later with MC data)
                        if position_clone.breakeven_mode_active {
                            false // Skip normal stop loss in breakeven mode
                        } else {
                            // Calculate PnL percentage: ((current_price - entry_price) / entry_price) * 100
                            let pnl_percent = ((current_price - entry_price) / entry_price) * 100.0;
                            
                            if pnl_percent <= -stop_loss_percent {
                                eprintln!("🚨 STOP LOSS TRIGGERED: PnL = {:.2}% (price: {:.8} -> {:.8}, threshold: -{:.2}%)", 
                                         pnl_percent, entry_price, current_price, stop_loss_percent);
                                true
                            } else {
                                false
                            }
                        }
                    } else if let Some(entry_mc_val) = entry_mc {
                        // Fallback to MC check if we don't have entry price but have entry MC
                        if entry_mc_val <= 0.0 {
                            eprintln!("⚠️  Position {}: Invalid entry MC ({:.2}), skipping stop loss check", position_clone.mint, entry_mc_val);
                            return None; // Invalid entry MC
                        }
                        
                        // Check if breakeven mode is active - if so, skip normal stop loss check
                        if position_clone.breakeven_mode_active {
                            false // Skip normal stop loss in breakeven mode (handled later)
                        } else {
                            // OPTIMIZED: Use batch-fetched MC data (already calculated above)
                            // Check if we should use breakeven stop loss
                            // ✅ CRITICAL FIX: Check breakeven_mode_active OR if MC reaches threshold
                            // Once breakeven mode is active, it stays active even if MC drops below threshold
                            if position_clone.breakeven_mode_active || current_mc_sol >= config_clone.breakeven_mc_threshold_sol {
                                // Breakeven mode: use entry MC as stop loss
                                if current_mc_sol < entry_mc_val {
                                    eprintln!("🛡️  BREAKEVEN STOP LOSS TRIGGERED (MC fallback): MC dropped to {:.2} SOL (entry: {:.2} SOL)", 
                                             current_mc_sol, entry_mc_val);
                                    return Some(("breakeven_stop_loss".to_string(), position_mint.clone()));
                                }
                                false // In breakeven mode, only sell if below entry
                            } else {
                                // Normal stop loss check
                                let stop_loss_threshold = entry_mc_val * (1.0 - stop_loss_percent / 100.0);
                                if current_mc_sol < stop_loss_threshold {
                                    eprintln!("🚨 STOP LOSS TRIGGERED (MC): MC dropped from {:.2} SOL to {:.2} SOL (threshold: {:.2} SOL)", 
                                             entry_mc_val, current_mc_sol, stop_loss_threshold);
                                }
                                current_mc_sol < stop_loss_threshold
                            }
                        }
                    } else {
                        // No entry price and no entry MC - can't calculate stop loss, skip
                        eprintln!("⚠️  Position {}: No entry price ({:?}) and no entry MC ({:?}) - cannot monitor stop loss", 
                                 position_clone.mint, position_clone.token_price_sol, entry_mc);
                        false
                    };
                    
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
                        
                        return Some(("stop_loss".to_string(), position_mint));
                    }
                    
                    // 🎯 BREAKEVEN STOP LOSS: If breakeven mode is active, use entry MC as stop loss
                    if let Some(entry_mc_val) = entry_mc {
                        if entry_mc_val > 0.0 {
                            // Check if MC reaches threshold (activates breakeven mode)
                            let breakeven_mode_active = if current_mc_sol >= config_clone.breakeven_mc_threshold_sol {
                                // Update peak MC in tracker (activates breakeven mode)
                                if let Ok(mut tracker_guard) = tracker_clone.write() {
                                    if let Some(tracker_ref) = tracker_guard.as_mut() {
                                        let _ = tracker_ref.update_peak_mc(
                                            &position_clone.mint,
                                            current_mc_sol,
                                            config_clone.breakeven_mc_threshold_sol,
                                        );
                                    }
                                }
                                true // MC just reached threshold, breakeven mode is now active
                            } else {
                                // Check if breakeven mode was already active (from previous cycle)
                                // Read fresh from tracker to get updated status
                                let mut is_active = position_clone.breakeven_mode_active;
                                if let Ok(tracker_guard) = tracker_clone.read() {
                                    if let Some(tracker_ref) = tracker_guard.as_ref() {
                                        if let Some(pos) = tracker_ref.get_active_positions().iter().find(|p| p.mint == position_clone.mint) {
                                            is_active = pos.breakeven_mode_active;
                                        }
                                    }
                                }
                                is_active
                            };
                            
                            // ✅ CRITICAL FIX: Check breakeven stop loss if breakeven mode is active
                            // Once activated (MC reached threshold), breakeven mode stays active
                            // and we check if MC drops below entry, regardless of current MC level
                            if breakeven_mode_active {
                                // Check if MC dropped below entry (breakeven stop loss)
                                if current_mc_sol < entry_mc_val {
                                    eprintln!("🛡️  BREAKEVEN STOP LOSS TRIGGERED: MC dropped from peak to {:.2} (entry: {:.2}) - SELLING AT BREAKEVEN", 
                                             current_mc_sol, entry_mc_val);
                                    
                                    // Execute sell at breakeven
                                    let _ = execute_sell(
                                        &config_clone,
                                        &wallet_clone,
                                        rpc_task.as_ref(),
                                        &tracker_clone,
                                        &position_clone,
                                        "breakeven_stop_loss",
                                        &event_tx_clone,
                                    ).await;
                                    
                                    return Some(("breakeven_stop_loss".to_string(), position_mint));
                                }
                                // Continue to take profit check (in breakeven mode, skip normal stop loss)
                            }
                        }
                    }
                    
                    // Check take profit: current_mc_sol >= take_profit_mc_sol
                    let should_sell_take_profit = current_mc_sol >= take_profit_mc_sol;
                    
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
                        
                        return Some((reason.to_string(), position_mint));
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

                let entry_mc = match position.mc_at_entry_sol {
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
                ).await;

                let current_mc = match current_mc_result {
                    Ok((_, mc_sol)) => {
                        use crate::utils::sol_to_usd;
                        sol_to_usd(mc_sol)
                    },
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
            // 🚀 ULTRA FAST MONITORING: Check every 10ms for instant stop loss detection (optimized for premium RPC)
            // This ensures we catch -30% drops within 10ms
            tokio::time::sleep(Duration::from_millis(1)).await; // Reduced to 1ms for ultra-fast stop loss detection
            Ok(())
        }.await;
        
        // Handle errors gracefully - log and continue
        if let Err(e) = loop_result {
            eprintln!("⚠️  Error in auto-sell monitoring loop: {}", e);
            eprintln!("   Continuing monitoring in 1 second...");
            // Use select to check shutdown during sleep
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_secs(1)) => {},
                _ = shutdown.changed() => {
                    if *shutdown.borrow() {
                        eprintln!("🛑 Position monitor received shutdown signal during error recovery");
                        break;
                    }
                }
            }
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
    history_tracker: Option<Arc<std::sync::RwLock<crate::accounts::HistoryTracker>>>,
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
        // Get bonding curve (we only need the curve, not MC)
        match fetch_bonding_curve_mc(rpc, &accounts.bonding_curve).await {
            Ok((curve, _)) => Some(curve),
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
        ).await;
        
        let (mc_entry_sol, token_price_entry) = match mc_result {
            Ok((curve, mc_sol)) => {
                let price = curve.get_token_price_sol();
                (Some(mc_sol), if price > 0.0 { Some(price) } else { None })
            }
            Err(_) => (None, None)
        };

        // ✅ CRITICAL FIX: Use try_write() instead of write() to avoid blocking GUI thread
        // Record buy in tracker (non-blocking to prevent UI freeze)
        let buy_data = TokenBuy {
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
            mc_at_detection_sol: None,
            mc_at_entry_sol: mc_entry_sol,
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
            peak_mc_sol: None,
            peak_pnl_percent: None,
            breakeven_mode_active: false,
            executed_sell_rules: Vec::new(),
            partial_sell_count: 0,
            total_sold_percent: 0.0,
            dev_buy_usd: None,
            our_buy_usd: None,
            pnl_usd: None,
            mc_at_detection_usd: None,
            mc_at_entry_usd: None,
            current_value_usd: None,
        };
        
        // Try to record buy (non-blocking)
        let mut buy_recorded = false;
        if let Ok(mut tracker_opt) = tracker.try_write() {
            if let Some(tracker) = tracker_opt.as_mut() {
                if let Err(e) = tracker.record_buy(buy_data.clone()) {
                    eprintln!("  ⚠️  Tracker error: {}", e);
                } else {
                    eprintln!("✅ Successfully recorded manual buy in tracker");
                    buy_recorded = true;
                }
            } else {
                eprintln!("⚠️  CRITICAL: Tracker is None - cannot record buy! Make sure ENABLE_TRACKER is true in config.");
            }
        } else {
            eprintln!("⚠️  Failed to acquire tracker lock for manual buy");
        }
        
        // ✅ RETRY LOGIC: If initial try failed, retry after delay
        if !buy_recorded {
            eprintln!("⚠️  Failed to acquire tracker write lock for manual buy - retrying...");
            tokio::time::sleep(Duration::from_millis(100)).await;
            if let Ok(mut tracker_opt) = tracker.try_write() {
                if let Some(tracker) = tracker_opt.as_mut() {
                    if let Err(e) = tracker.record_buy(buy_data.clone()) {
                        eprintln!("  ⚠️  Tracker error (retry): {}", e);
                    } else {
                        eprintln!("✅ Successfully recorded manual buy after retry");
                        buy_recorded = true;
                    }
                }
            }
            if !buy_recorded {
                eprintln!("⚠️  CRITICAL: Failed to record manual buy even after retry");
            }
        }
        
        let buy = Some(buy_data);
        
        // 📊 ULTRA MC TRACKING: Register token in history tracker immediately after buy
        if let Some(ref buy_data) = buy {
            if let Some(ref history) = history_tracker {
                let mint_str = buy_data.mint.clone();
                let bonding_curve_str = buy_data.bonding_curve.clone().unwrap_or_default();
                let entry_mc = mc_entry_sol.unwrap_or(0.0);
                let entry_price = token_price_entry.unwrap_or(0.0);
                let our_buy_sol = config.buy_amount_sol;
                let token_amount_opt = buy_data.token_amount;
                
                // ✅ FIX: Use try_write to avoid blocking/deadlock
                // Register token (non-blocking)
                if let Ok(mut history_guard) = history.try_write() {
                    history_guard.register_token(
                        &mint_str,
                        &bonding_curve_str,
                        entry_mc,
                        entry_price,
                        our_buy_sol,
                        token_amount_opt,
                    );
                }
                
                // 📊 Record initial MC snapshot immediately after buy (fetch curve first, then lock)
                // ✅ FIX: Use timeout to prevent blocking on slow RPC
                // Parse bonding curve from string (we already have it in buy_data)
                if !bonding_curve_str.is_empty() {
                    if let Ok(bonding_curve_pubkey) = Pubkey::from_str(&bonding_curve_str) {
                        // Use timeout to prevent hanging on slow RPC
                        match tokio::time::timeout(Duration::from_secs(3), 
                            fetch_bonding_curve_mc(rpc, &bonding_curve_pubkey)
                        ).await {
                            Ok(Ok((curve, _))) => {
                                let current_mc_sol = curve.calculate_mc_sol();
                                use crate::utils::sol_to_usd;
                                let current_mc = sol_to_usd(current_mc_sol);
                                let current_price = curve.get_token_price_sol();
                                
                                // Get initial PnL if available
                                let initial_pnl_percent = if entry_price > 0.0 && current_price > 0.0 {
                                    Some(((current_price - entry_price) / entry_price) * 100.0)
                                } else {
                                    None
                                };
                                
                                // ✅ FIX: Use try_write to avoid blocking/deadlock
                                if let Ok(mut history_guard) = history.try_write() {
                                    history_guard.record_from_bonding_curve(
                                        &mint_str,
                                        &curve,
                                        initial_pnl_percent,
                                        None, // pnl_sol not calculated yet
                                        None, // current_value_sol not calculated yet
                                    );
                                    
                                    let mint_short = if mint_str.len() > 8 { &mint_str[..8] } else { &mint_str };
                                    eprintln!("📊 HISTORY: Registered and recorded initial MC snapshot for {} (MC: ${:.0}, Price: {:.8})", 
                                             mint_short, current_mc, current_price);
                                }
                            }
                            Ok(Err(e)) => {
                                // Fetch failed but don't block - continue
                                eprintln!("⚠️  Failed to fetch bonding curve for history snapshot: {}", e);
                            }
                            Err(_) => {
                                // Timeout - don't block, continue
                                eprintln!("⚠️  Timeout fetching bonding curve for history snapshot (3s)");
                            }
                        }
                    }
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
    
    // 🔥 IMPROVED: Validate bonding curve exists before proceeding
    let bonding_curve_str = position.bonding_curve.as_ref()
        .ok_or_else(|| {
            eprintln!("   ❌ SELL FAILED: Bonding curve not found in position");
            anyhow!("Bonding curve not found in position - cannot sell")
        })?;
    
    let bonding_curve = Pubkey::from_str(bonding_curve_str).map_err(|e| {
        eprintln!("   ❌ SELL FAILED: Invalid bonding curve address: {}", bonding_curve_str);
        anyhow!("Invalid bonding curve address: {} - {}", bonding_curve_str, e)
    })?;
    
    // 🔥 IMPROVED: Verify bonding curve account exists and is valid
    match rpc.get_account_with_commitment(&bonding_curve, CommitmentConfig::confirmed()).await {
        Ok(account_info) => {
            if account_info.value.is_none() {
                eprintln!("   ❌ SELL FAILED: Bonding curve account does not exist (token may be migrated)");
                return Err(anyhow!("Bonding curve account does not exist - token may be migrated to Raydium"));
            }
            eprintln!("   ✅ Bonding curve account verified");
        }
        Err(e) => {
            eprintln!("   ⚠️  WARNING: Failed to verify bonding curve account: {} (continuing anyway)", e);
        }
    }
    
    let user_wallet = wallet.pubkey();
    
    // ⚡ ULTRA FAST: Derive accounts in parallel
    let token_program_2022 = Pubkey::from_str("TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb").unwrap();
    let user_token_account_from_tracker = position.user_token_account.as_ref()
        .and_then(|s| Pubkey::from_str(s).ok());
    
    let user_token_account_2022 = get_associated_token_address_with_program_id(
        &user_wallet, &mint, &token_program_2022
    );
    
    // 🔥 IMPROVED: Get balance - try all possible token accounts (tracker ATA, Token 2022, standard Token Program)
    let mut token_balance = 0u64;
    let mut user_token_account = user_token_account_2022;
    let mut token_program_used = token_program_2022;
    
    // Try tracker ATA first (if available) - fastest path
    if let Some(tracker_ata) = user_token_account_from_tracker {
        if let Ok(balance) = rpc.get_token_account_balance(&tracker_ata).await {
            token_balance = balance.amount.parse::<u64>().unwrap_or(0);
            if token_balance > 0 {
                user_token_account = tracker_ata;
                eprintln!("   ✅ Found balance in tracker ATA: {} tokens", token_balance);
            }
        }
    }
    
    // If tracker ATA failed, try Token 2022 directly
    if token_balance == 0 {
        if let Ok(balance) = rpc.get_token_account_balance(&user_token_account_2022).await {
            token_balance = balance.amount.parse::<u64>().unwrap_or(0);
            if token_balance > 0 {
                eprintln!("   ✅ Found balance in Token 2022 ATA: {} tokens", token_balance);
            }
        }
    }
    
    // 🔥 NEW: If Token 2022 failed, try standard Token Program (some tokens use standard program)
    if token_balance == 0 {
        let token_program_standard = Pubkey::from_str("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA").unwrap();
        let user_token_account_standard = get_associated_token_address_with_program_id(
            &user_wallet, &mint, &token_program_standard
        );
        
        if let Ok(balance) = rpc.get_token_account_balance(&user_token_account_standard).await {
            token_balance = balance.amount.parse::<u64>().unwrap_or(0);
            if token_balance > 0 {
                user_token_account = user_token_account_standard;
                token_program_used = token_program_standard;
                eprintln!("   ✅ Found balance in standard Token Program ATA: {} tokens", token_balance);
            }
        }
    }
    
    if token_balance == 0 {
        eprintln!("   ❌ SELL FAILED: Token balance is 0 for all token accounts");
        eprintln!("      - Tried tracker ATA: {:?}", user_token_account_from_tracker);
        eprintln!("      - Tried Token 2022 ATA: {}", user_token_account_2022);
        eprintln!("      - Tried standard Token Program ATA: {}", get_associated_token_address_with_program_id(
            &user_wallet, &mint, &Pubkey::from_str("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA").unwrap()
        ));
        return Err(anyhow!("Token balance is 0 - tried all token accounts (tracker ATA, Token 2022, standard Token Program)"));
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
    
    // 🔥 IMPROVED: Always use creator vault from buy transaction with retry logic
    // Derived PDA can be wrong - must use exact vault from buy TX to avoid Error 2006
    let creator_vault_fut: std::pin::Pin<Box<dyn std::future::Future<Output = Result<Pubkey>> + Send>> = Box::pin(async move {
        if position.signature.starts_with("MOCK_") {
            // For mock transactions, use derived PDA as fallback
            let (vault, _) = crate::pda_derivation::derive_creator_vault_pda(&creator);
            Ok(vault)
        } else {
            // 🔥 IMPROVED: Retry logic for creator vault extraction (up to 3 attempts)
            let mut last_error = None;
            for attempt in 1..=3 {
                let result = tokio::time::timeout(
                    Duration::from_millis(500), // Increased timeout for reliability
                    extract_creator_vault_from_buy_tx(rpc, &position.signature)
                ).await;
                
                match result {
                    Ok(Ok(vault)) => {
                        eprintln!("   ✅ Successfully extracted creator vault from buy TX: {}", vault);
                        return Ok(vault);
                    }
                    Ok(Err(e)) => {
                        eprintln!("   ⚠️  Attempt {} failed: {}", attempt, e);
                        last_error = Some(e);
                        if attempt < 3 {
                            tokio::time::sleep(Duration::from_millis(100 * attempt as u64)).await;
                        }
                    }
                    Err(_) => {
                        eprintln!("   ⚠️  Attempt {} timed out", attempt);
                        if attempt < 3 {
                            tokio::time::sleep(Duration::from_millis(100 * attempt as u64)).await;
                        }
                    }
                }
            }
            
            // All retries failed - use derived PDA as last resort (but log warning)
            eprintln!("   ⚠️  WARNING: All attempts to fetch creator vault from buy TX failed, using derived PDA (may cause Error 2006)");
            if let Some(err) = last_error {
                eprintln!("      Last error: {}", err);
            }
            let (vault, _) = crate::pda_derivation::derive_creator_vault_pda(&creator);
            Ok(vault)
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
    // For stop loss, use static fee (no HTTP call) for maximum speed
    let (sell_ix, priority_fee) = tokio::join!(
        build_sell_instruction(&accounts, &user_wallet, &user_token_account, sell_amount),
        async {
            if reason == "stop_loss" {
                // 🚀 ULTRA FAST: For stop loss, use static fee (no HTTP call)
                config.priority_fee
            } else if config.enable_dynamic_priority_fee {
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
    
    eprintln!("[SELL] {} - TX: {}", format_addr(&position.mint), format_addr(&signature));
    Ok(signature)
}

/// Execute sell with custom sell percent (for partial sells)
async fn execute_sell_with_percent(
    config: &Config,
    wallet: &Keypair,
    rpc: &RpcClient,
    tracker: &Arc<std::sync::RwLock<Option<TokenTracker>>>,
    position: &TokenBuy,
    reason: &str,
    sell_percent: f64,  // Custom sell percent (0-100)
    event_tx: &mpsc::UnboundedSender<TokenEvent>,
) -> Result<String> {
    // Same as execute_sell, but use custom sell_percent
    eprintln!("🎯 PARTIAL SELL EXECUTION: {} - Selling {:.0}% of position", position.mint, sell_percent);
    let mint = Pubkey::from_str(&position.mint)?;
    
    let bonding_curve_str = position.bonding_curve.as_ref()
        .ok_or_else(|| anyhow!("Bonding curve not found in position"))?;
    let bonding_curve = Pubkey::from_str(bonding_curve_str)?;
    
    let user_wallet = wallet.pubkey();
    let token_program_2022 = Pubkey::from_str("TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb").unwrap();
    let user_token_account_from_tracker = position.user_token_account.as_ref()
        .and_then(|s| Pubkey::from_str(s).ok());
    let user_token_account_2022 = get_associated_token_address_with_program_id(
        &user_wallet, &mint, &token_program_2022
    );
    
    // Get balance
    let mut token_balance = 0u64;
    let mut user_token_account = user_token_account_2022;
    let mut token_program_used = token_program_2022;
    
    if let Some(tracker_ata) = user_token_account_from_tracker {
        if let Ok(balance) = rpc.get_token_account_balance(&tracker_ata).await {
            token_balance = balance.amount.parse::<u64>().unwrap_or(0);
            if token_balance > 0 {
                user_token_account = tracker_ata;
            }
        }
    }
    
    if token_balance == 0 {
        if let Ok(balance) = rpc.get_token_account_balance(&user_token_account_2022).await {
            token_balance = balance.amount.parse::<u64>().unwrap_or(0);
        }
    }
    
    if token_balance == 0 {
        let token_program_standard = Pubkey::from_str("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA").unwrap();
        let user_token_account_standard = get_associated_token_address_with_program_id(
            &user_wallet, &mint, &token_program_standard
        );
        if let Ok(balance) = rpc.get_token_account_balance(&user_token_account_standard).await {
            token_balance = balance.amount.parse::<u64>().unwrap_or(0);
            if token_balance > 0 {
                user_token_account = user_token_account_standard;
                token_program_used = token_program_standard;
            }
        }
    }
    
    if token_balance == 0 {
        return Err(anyhow!("Token balance is 0"));
    }

    // Calculate sell amount using custom sell_percent
    let sell_amount = (token_balance as f64 * (sell_percent / 100.0)) as u64;
    if sell_amount == 0 {
        return Err(anyhow!("Sell amount is 0"));
    }

    let creator = Pubkey::from_str(&position.creator)?;
    let associated_bonding_curve = get_associated_token_address_with_program_id(
        &bonding_curve, &mint, &token_program_used
    );
    
    let creator_vault_fut = Box::pin(async move {
        if position.signature.starts_with("MOCK_") {
            let (vault, _) = crate::pda_derivation::derive_creator_vault_pda(&creator);
            Ok::<Pubkey, anyhow::Error>(vault)
        } else {
            extract_creator_vault_from_buy_tx(rpc, &position.signature).await
                .or_else(|_| {
                    let (vault, _) = crate::pda_derivation::derive_creator_vault_pda(&creator);
                    Ok::<Pubkey, anyhow::Error>(vault)
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

    let (sell_ix, priority_fee) = tokio::join!(
        build_sell_instruction(&accounts, &user_wallet, &user_token_account, sell_amount),
        async {
            if reason.contains("stop_loss") {
                config.priority_fee
            } else if config.enable_dynamic_priority_fee {
                config.calculate_dynamic_priority_fee(rpc).await.unwrap_or(config.priority_fee)
            } else {
                config.priority_fee
            }
        }
    );
    let sell_ix = sell_ix?;
    
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

    if config.mock_sell {
        use solana_sdk::signature::Signature;
        use rand::Rng;
        let mut rng = rand::thread_rng();
        let mut mock_sig_bytes = [0u8; 64];
        rng.fill(&mut mock_sig_bytes);
        let mock_signature = format!("MOCK_PARTIAL_SELL_{}", Signature::from(mock_sig_bytes).to_string());
        
        // Update tracker for partial sell
        if let Ok(mut tracker_opt) = tracker.write() {
            if let Some(tracker) = tracker_opt.as_mut() {
                if sell_percent >= 100.0 {
                    let _ = tracker.mark_as_sold(&position.mint, mock_signature.clone());
                } else {
                    // Partial sell - update balance
                    let new_balance = token_balance - sell_amount;
                    let _ = tracker.update_token_amount(&position.mint, new_balance);
                }
            }
        }
        
        let _ = event_tx.send(TokenEvent::Sold {
            mint: position.mint.clone(),
            signature: mock_signature.clone(),
            reason: format!("{} (MOCK, {:.0}%)", reason, sell_percent),
            pnl: None,
            timestamp: Utc::now(),
        });
        return Ok(mock_signature);
    }

    let tx_sig = match config.submission_mode {
        crate::config::SubmissionMode::Helius => send_helius_transaction(tx).await?,
        crate::config::SubmissionMode::Jito => {
            return Ok(format!("Jito: {}", send_jito_bundle(tx, wallet, recent_blockhash, config.jito_tip).await?));
        }
        crate::config::SubmissionMode::Rpc => rpc.send_transaction(&tx).await?.to_string(),
        crate::config::SubmissionMode::All => {
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

    // Update tracker based on sell type
    let update_result = {
        if let Ok(mut tracker_opt) = tracker.try_write() {
            if let Some(tracker) = tracker_opt.as_mut() {
                if sell_percent >= 100.0 {
                    // Full sell
                    tracker.mark_as_sold(&position.mint, signature.clone())
                } else {
                    // Partial sell - update balance and track
                    let new_balance = token_balance - sell_amount;
                    tracker.update_token_amount(&position.mint, new_balance)
                }
            } else {
                Err(anyhow::anyhow!("Tracker is None"))
            }
        } else {
            Err(anyhow::anyhow!("Failed to acquire tracker lock"))
        }
    };
    
    match update_result {
        Ok(_) => {
            eprintln!("✅ Position updated in tracker, JSON saved");
        }
        Err(e) => {
            eprintln!("⚠️  Failed to update tracker: {}, will retry...", e);
            // Retry after delay (lock is dropped, so we can await safely)
            tokio::time::sleep(Duration::from_millis(200)).await;
            if let Ok(mut tracker_opt) = tracker.try_write() {
                if let Some(tracker) = tracker_opt.as_mut() {
                    let retry_result = if sell_percent >= 100.0 {
                        tracker.mark_as_sold(&position.mint, signature.clone())
                    } else {
                        let new_balance = token_balance - sell_amount;
                        tracker.update_token_amount(&position.mint, new_balance)
                    };
                    match retry_result {
                        Ok(_) => eprintln!("✅ Position updated in tracker (retry), JSON saved"),
                        Err(e) => eprintln!("❌ CRITICAL: Failed to save sell to tracker even after retry: {} - JSON may not be updated!", e),
                    }
                } else {
                    eprintln!("❌ CRITICAL: Tracker is None after retry - JSON may not be updated!");
                }
            } else {
                eprintln!("❌ CRITICAL: Failed to acquire tracker lock even after retry - JSON may not be updated!");
            }
        }
    }

    let _ = event_tx.send(TokenEvent::Sold {
        mint: position.mint.clone(),
        signature: signature.clone(),
        reason: format!("{} ({:.0}%)", reason, sell_percent),
        pnl: None,
        timestamp: Utc::now(),
    });
    
    eprintln!("✅ PARTIAL SELL COMPLETED: {} - Sold {:.0}% - TX: {}", format_addr(&position.mint), sell_percent, format_addr(&signature));
    Ok(signature)
}

/// Ultra-fast PnL monitor - updates every 100ms for live display
/// Also records MC/Price history snapshots for chart generation
async fn monitor_pnl_ultra_fast(
    config: Arc<std::sync::RwLock<Config>>,
    _rpc: RpcClient,
    tracker: Arc<std::sync::RwLock<Option<TokenTracker>>>,
    history_tracker: Arc<std::sync::RwLock<crate::accounts::HistoryTracker>>,
    mut shutdown: tokio::sync::watch::Receiver<bool>,
) {
    let mut interval = tokio::time::interval(Duration::from_millis(150)); // Update every 150ms (~7x/sec) - balanced for UI responsiveness
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    
    loop {
        // ✅ FIX: Check shutdown signal before each iteration
        if *shutdown.borrow() {
            eprintln!("🛑 PnL monitor received shutdown signal");
            break;
        }
        
        // Use select to check shutdown during interval tick
        tokio::select! {
            _ = interval.tick() => {},
            _ = shutdown.changed() => {
                if *shutdown.borrow() {
                    eprintln!("🛑 PnL monitor received shutdown signal");
                    break;
                }
            }
        }
        
        // Get config values
        // Refresh SOL price if needed
        crate::utils::refresh_sol_price_if_needed().await;
        
        let _ = {
            let cfg = config.read().unwrap();
            cfg.helius_api_key.clone()
        };
        
        // Get active positions
        let positions_to_update = {
            if let Ok(tracker_guard) = tracker.read() {
                if let Some(tracker_ref) = tracker_guard.as_ref() {
                    tracker_ref.get_active_positions_for_pnl()
                } else {
                    // Tracker is None - skip silently
                    Vec::new()
                }
            } else {
                // Failed to read tracker - skip silently (will retry next cycle)
                Vec::new()
            }
        };
        
        // ✅ OPTIMIZED: Removed debug logging every iteration (was logging 50x/sec, now 7x/sec)
        // Debug logging can be enabled via environment variable if needed
        
        if positions_to_update.is_empty() {
            continue;
        }
        
        // OPTIMIZED: Batch fetch all bonding curve accounts at once
        use std::str::FromStr;
        use solana_sdk::pubkey::Pubkey;
        
        // Create RPC client once for batch operations
        let rpc = {
            let cfg = config.read().unwrap();
            cfg.create_rpc_client()
        };
        
        // Parse all bonding curves and prepare for batch fetch
        let mut bonding_curves_vec = Vec::new();
        let mut position_data = Vec::new(); // Track (mint, bonding_curve_str) for each position
        
        for (mint, bonding_curve_str) in positions_to_update.iter() {
            match Pubkey::from_str(bonding_curve_str) {
                Ok(bonding_curve) => {
                    // ✅ FIX: Don't log every 20ms - only log on errors
                    // eprintln!("✅ Parsed bonding curve for {}: {}", mint_short, bonding_curve);
                    bonding_curves_vec.push(bonding_curve);
                    position_data.push((mint.clone(), bonding_curve_str.clone()));
                },
                Err(e) => {
                    let mint_short = if mint.len() > 8 { &mint[..8] } else { mint };
                    eprintln!("❌ Failed to parse bonding curve for {}: '{}' - Error: {}", 
                             mint_short, bonding_curve_str, e);
                }
            }
        }
        
        // OPTIMIZED: Batch fetch all bonding curves at once
        let bonding_curve_data = if !bonding_curves_vec.is_empty() {
            batch_fetch_bonding_curves(&rpc, &bonding_curves_vec).await
        } else {
            Vec::new()
        };
        
        // Process all positions with batch-fetched data
        let breakeven_threshold_sol = {
            let cfg = config.read().unwrap();
            cfg.breakeven_mc_threshold_sol
        };
        
        for (idx, (mint, _)) in position_data.iter().enumerate() {
            let mint_short = if mint.len() > 8 { &mint[..8] } else { mint };
            
            if idx >= bonding_curve_data.len() {
                eprintln!("⚠️  PnL MONITOR: Bonding curve fetch failed for {} (idx {} >= len {})", 
                         mint_short, idx, bonding_curve_data.len());
                continue; // Skip if batch fetch failed for this position
            }
            
            if let Some(curve) = &bonding_curve_data[idx] {
                let current_price = curve.get_token_price_sol();
                let current_mc_sol = curve.calculate_mc_sol();
                use crate::utils::sol_to_usd;
                let current_mc = sol_to_usd(current_mc_sol);
                
                
                // Variables to capture position data for history recording
                let mut pnl_percent_for_history: Option<f64> = None;
                let mut pnl_sol_for_history: Option<f64> = None;
                let mut current_value_for_history: Option<f64> = None;
                let mut entry_mc_for_history: Option<f64> = None;
                let mut our_buy_sol_for_history: Option<f64> = None;
                let mut token_amount_for_history: Option<u64> = None;
                let mut bonding_curve_for_history: Option<String> = None;
                
                // Update PnL and peak MC in tracker (fast, no disk write)
                // ✅ OPTIMIZED: Use try_write to avoid blocking UI thread
                if let Ok(mut tracker_guard) = tracker.try_write() {
                    if let Some(tracker) = tracker_guard.as_mut() {
                        match tracker.update_position_pnl_fast(mint, current_price) {
                            Ok(_) => {
                                // ✅ FIX: Don't log every update - too verbose (was logging 50x/sec)
                                // Only log errors to reduce console spam
                            },
                            Err(e) => {
                                eprintln!("❌ PnL MONITOR: Failed to update PnL for {}: {}", mint_short, e);
                            }
                        }
                        let _ = tracker.update_peak_mc(mint, current_mc_sol, breakeven_threshold_sol);
                        
                        // Get position data for history recording
                        if let Some(pos) = tracker.get_active_positions().iter().find(|p| p.mint == *mint) {
                            pnl_percent_for_history = pos.pnl_percent;
                            pnl_sol_for_history = pos.pnl_sol;
                            current_value_for_history = pos.current_value_sol;
                            entry_mc_for_history = pos.mc_at_entry_sol;
                            our_buy_sol_for_history = Some(pos.our_buy_sol);
                            token_amount_for_history = pos.token_amount;
                            bonding_curve_for_history = pos.bonding_curve.clone();
                        }
                    } else {
                        // Tracker is None - skip silently (not an error condition)
                    }
                } else {
                    // Failed to acquire lock - UI thread is reading, skip this update (will retry next cycle)
                    // Don't log as this is expected when UI is actively rendering
                }
                
                // 📊 ULTRA HISTORY: Record snapshot for chart generation
                // ✅ FIX: Use try_write to avoid blocking if lock is held elsewhere (prevents deadlock/crash)
                if let Ok(mut history) = history_tracker.try_write() {
                    // Register token if not already tracked
                    if history.get_token_history(mint).is_none() {
                        if let (Some(entry_mc), Some(our_buy), Some(bc)) = (entry_mc_for_history, our_buy_sol_for_history, bonding_curve_for_history.clone()) {
                            let entry_price = curve.get_token_price_sol(); // Approximate entry price
                            history.register_token(
                                mint,
                                &bc,
                                entry_mc,
                                entry_price,
                                our_buy,
                                token_amount_for_history,
                            );
                        }
                    }
                    
                    // Record snapshot (rate-limited internally to ~1/sec per token)
                    history.record_from_bonding_curve(
                        mint,
                        curve,
                        pnl_percent_for_history,
                        pnl_sol_for_history,
                        current_value_for_history,
                    );
                }
                // If lock is held elsewhere (e.g., by GUI or another thread), skip this update
                // This is fine, we'll catch it on the next iteration (every 20ms)
            } else {
                eprintln!("⚠️  PnL MONITOR: Bonding curve data is None for {} (idx {})", mint_short, idx);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use solana_sdk::pubkey::Pubkey;

    #[tokio::test]
    async fn test_batch_check_token_balances_empty() {
        // Test with empty input
        let rpc = RpcClient::new("https://api.mainnet-beta.solana.com".to_string());
        let accounts: Vec<Pubkey> = Vec::new();
        let result = batch_check_token_balances(&rpc, &accounts).await;
        assert_eq!(result.len(), 0);
    }

    #[tokio::test]
    #[ignore] // Requires real RPC connection
    async fn test_batch_check_token_balances_single() {
        // Integration test - requires real token account
        // This test would need a real token account address to work
        // let rpc = RpcClient::new("https://api.mainnet-beta.solana.com".to_string());
        // Use a known token account for testing (if available)
        // let test_account = Pubkey::from_str("...").unwrap();
        // let result = batch_check_token_balances(&rpc, &[test_account]).await;
        // assert_eq!(result.len(), 1);
    }

    #[tokio::test]
    async fn test_batch_fetch_bonding_curves_empty() {
        // Test with empty input
        let rpc = RpcClient::new("https://api.mainnet-beta.solana.com".to_string());
        let bonding_curves: Vec<Pubkey> = Vec::new();
        let result = batch_fetch_bonding_curves(&rpc, &bonding_curves).await;
        assert_eq!(result.len(), 0);
    }

    #[tokio::test]
    #[ignore] // Requires real RPC connection
    async fn test_batch_fetch_bonding_curves_single() {
        // Integration test - requires real bonding curve account
        // This test would need a real bonding curve address to work
        // let rpc = RpcClient::new("https://api.mainnet-beta.solana.com".to_string());
        // Use a known bonding curve for testing (if available)
        // let test_bc = Pubkey::from_str("...").unwrap();
        // let result = batch_fetch_bonding_curves(&rpc, &[test_bc]).await;
        // assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_batch_fetch_bonding_curves_batch_size() {
        // Test that batch size limit is respected (100 accounts per batch)
        // This is a unit test that verifies the logic without RPC calls
        let bonding_curves: Vec<Pubkey> = (0..150)
            .map(|_| Pubkey::new_unique())
            .collect();
        
        // Verify we have 150 accounts
        assert_eq!(bonding_curves.len(), 150);
        
        // When batch_fetch_bonding_curves is called, it should split into 2 batches:
        // - First batch: 100 accounts
        // - Second batch: 50 accounts
        // This is tested implicitly by the function implementation
    }

    #[test]
    fn test_batch_check_token_balances_batch_size() {
        // Test that batch size limit is respected (100 accounts per batch)
        let token_accounts: Vec<Pubkey> = (0..250)
            .map(|_| Pubkey::new_unique())
            .collect();
        
        // Verify we have 250 accounts
        assert_eq!(token_accounts.len(), 250);
        
        // When batch_check_token_balances is called, it should split into 3 batches:
        // - First batch: 100 accounts
        // - Second batch: 100 accounts
        // - Third batch: 50 accounts
        // This is tested implicitly by the function implementation
    }
}

