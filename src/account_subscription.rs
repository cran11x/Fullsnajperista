// account_subscription.rs - WebSocket Account Subscription for Real-Time PNL Updates

use anyhow::{anyhow, Result};
use solana_sdk::pubkey::Pubkey;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{RwLock, mpsc};
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message as WsMessage};
use futures_util::{StreamExt, SinkExt};
use serde_json::Value;
use borsh::BorshDeserialize;
use crate::accounts::{TokenTracker, BondingCurveAccount};
use crate::config::Config;

/// Subscription command
pub enum SubscriptionCommand {
    Subscribe(Pubkey),
    Unsubscribe(Pubkey),
}

/// Account subscription manager for real-time PNL updates
pub struct AccountSubscriptionManager {
    wss_url: String,
    subscriptions: Arc<RwLock<HashMap<String, u64>>>, // bonding_curve -> subscription_id
    subscription_to_bonding_curve: Arc<RwLock<HashMap<u64, String>>>, // subscription_id -> bonding_curve
    tracker: Arc<std::sync::RwLock<Option<TokenTracker>>>,
    _config: Arc<std::sync::RwLock<Config>>,
    next_request_id: Arc<std::sync::atomic::AtomicU64>,
}

/// Subscription manager handle for sending commands
pub struct SubscriptionHandle {
    command_tx: mpsc::UnboundedSender<SubscriptionCommand>,
}

impl AccountSubscriptionManager {
    /// Create new subscription manager
    pub fn new(
        config: Arc<std::sync::RwLock<Config>>,
        tracker: Arc<std::sync::RwLock<Option<TokenTracker>>>,
    ) -> Self {
        let wss_url = {
            let cfg = config.read().unwrap();
            cfg.wss_url.clone()
        };
        
        Self {
            wss_url,
            subscriptions: Arc::new(RwLock::new(HashMap::new())),
            subscription_to_bonding_curve: Arc::new(RwLock::new(HashMap::new())),
            tracker,
            _config: config,
            next_request_id: Arc::new(std::sync::atomic::AtomicU64::new(1)),
        }
    }

    /// Start subscription manager as background task
    pub async fn start(
        self,
        mut shutdown: tokio::sync::watch::Receiver<bool>,
    ) -> Result<SubscriptionHandle> {
        let (command_tx, mut command_rx) = mpsc::unbounded_channel();
        let (ws_msg_tx, mut ws_msg_rx) = mpsc::unbounded_channel::<String>();
        
        let (ws_stream, _) = connect_async(&self.wss_url).await
            .map_err(|e| anyhow!("WebSocket connection failed: {}", e))?;

        eprintln!("[ACCOUNT_SUBSCRIPTION] Connected to {}", &self.wss_url);

        let (mut write, mut read) = ws_stream.split();
        let pending_subscriptions: Arc<RwLock<HashMap<u64, String>>> = Arc::new(RwLock::new(HashMap::new())); // request_id -> bonding_curve

        let manager = Arc::new(self);
        let manager_clone1 = manager.clone();
        let manager_clone2 = manager.clone();
        let pending_clone1 = pending_subscriptions.clone();
        let pending_clone2 = pending_subscriptions.clone();

        // Spawn task to handle subscription commands and send WebSocket messages
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    cmd = command_rx.recv() => {
                        match cmd {
                            Some(cmd) => {
                                match cmd {
                                    SubscriptionCommand::Subscribe(bonding_curve) => {
                                        if let Err(e) = manager_clone1.subscribe_to_bonding_curve_internal(&bonding_curve, &ws_msg_tx, &pending_clone1).await {
                                            eprintln!("[ACCOUNT_SUBSCRIPTION] Failed to subscribe: {}", e);
                                        }
                                    }
                                    SubscriptionCommand::Unsubscribe(bonding_curve) => {
                                        if let Err(e) = manager_clone1.unsubscribe_internal(&bonding_curve, &ws_msg_tx).await {
                                            eprintln!("[ACCOUNT_SUBSCRIPTION] Failed to unsubscribe: {}", e);
                                        }
                                    }
                                }
                            }
                            None => break, // Channel closed
                        }
                    }
                    msg = ws_msg_rx.recv() => {
                        match msg {
                            Some(msg_text) => {
                                if let Err(e) = write.send(WsMessage::Text(msg_text)).await {
                                    eprintln!("[ACCOUNT_SUBSCRIPTION] Failed to send WebSocket message: {}", e);
                                    break;
                                }
                            }
                            None => break, // Channel closed
                        }
                    }
                }
            }
        });

        // Spawn task to handle incoming WebSocket messages
        tokio::spawn(async move {
            loop {
                if *shutdown.borrow() {
                    eprintln!("[ACCOUNT_SUBSCRIPTION] Shutdown signal received");
                    break;
                }

                tokio::select! {
                    msg_result = read.next() => {
                        match msg_result {
                            Some(Ok(WsMessage::Text(text))) => {
                                if let Err(e) = manager_clone2.handle_message(&text, &pending_clone2).await {
                                    eprintln!("[ACCOUNT_SUBSCRIPTION] Error handling message: {}", e);
                                }
                            }
                            Some(Ok(WsMessage::Close(_))) => {
                                eprintln!("[ACCOUNT_SUBSCRIPTION] WebSocket closed by server");
                                break;
                            }
                            Some(Err(e)) => {
                                eprintln!("[ACCOUNT_SUBSCRIPTION] WebSocket error: {}", e);
                                break;
                            }
                            None => {
                                eprintln!("[ACCOUNT_SUBSCRIPTION] WebSocket stream ended");
                                break;
                            }
                            _ => {} // Ignore other message types
                        }
                    }
                    _ = shutdown.changed() => {
                        if *shutdown.borrow() {
                            eprintln!("[ACCOUNT_SUBSCRIPTION] Shutdown signal received");
                            break;
                        }
                    }
                }
            }
        });

        Ok(SubscriptionHandle { command_tx })
    }

    /// Internal method to subscribe (called from spawned task)
    async fn subscribe_to_bonding_curve_internal(
        &self,
        bonding_curve: &Pubkey,
        ws_msg_tx: &mpsc::UnboundedSender<String>,
        pending_subscriptions: &Arc<RwLock<HashMap<u64, String>>>,
    ) -> Result<u64> {
        let bonding_curve_str = bonding_curve.to_string();
        
        // Check if already subscribed
        let subscriptions = self.subscriptions.read().await;
        if subscriptions.contains_key(&bonding_curve_str) {
            drop(subscriptions);
            eprintln!("[ACCOUNT_SUBSCRIPTION] Already subscribed to: {}", bonding_curve);
            return Ok(0); // Already subscribed
        }
        drop(subscriptions);

        // Generate request ID (different from subscription ID which comes in response)
        let request_id = self.next_request_id.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        
        // Create subscription request
        let subscribe_msg = serde_json::json!({
            "jsonrpc": "2.0",
            "id": request_id,
            "method": "accountSubscribe",
            "params": [
                bonding_curve_str,
                {
                    "encoding": "base64",
                    "commitment": "processed"
                }
            ]
        });

        // Store pending subscription
        {
            let mut pending = pending_subscriptions.write().await;
            pending.insert(request_id, bonding_curve_str.clone());
        }

        // Send subscription request via channel
        ws_msg_tx.send(subscribe_msg.to_string())
            .map_err(|e| anyhow!("Failed to send subscription via channel: {}", e))?;

        eprintln!("[ACCOUNT_SUBSCRIPTION] Subscribed to bonding curve: {} (request_id: {})", bonding_curve, request_id);
        
        Ok(request_id)
    }

    /// Internal method to unsubscribe (called from spawned task)
    async fn unsubscribe_internal(
        &self,
        bonding_curve: &Pubkey,
        ws_msg_tx: &mpsc::UnboundedSender<String>,
    ) -> Result<()> {
        let bonding_curve_str = bonding_curve.to_string();
        
        let subscription_id_opt = {
            let mut subscriptions = self.subscriptions.write().await;
            subscriptions.remove(&bonding_curve_str)
        };

        if let Some(subscription_id) = subscription_id_opt {
            // Remove from subscription_to_bonding_curve mapping
            {
                let mut sub_to_bc = self.subscription_to_bonding_curve.write().await;
                sub_to_bc.remove(&subscription_id);
            }
            
            // Send unsubscribe request via channel
            let unsubscribe_msg = serde_json::json!({
                "jsonrpc": "2.0",
                "id": subscription_id,
                "method": "accountUnsubscribe",
                "params": [subscription_id]
            });

            ws_msg_tx.send(unsubscribe_msg.to_string())
                .map_err(|e| anyhow!("Failed to send unsubscribe via channel: {}", e))?;

            eprintln!("[ACCOUNT_SUBSCRIPTION] Unsubscribed from bonding curve: {}", bonding_curve);
        }
        
        Ok(())
    }

    /// Handle incoming WebSocket message
    async fn handle_message(
        &self,
        text: &str,
        pending_subscriptions: &Arc<RwLock<HashMap<u64, String>>>,
    ) -> Result<()> {
        let json: Value = serde_json::from_str(text)
            .map_err(|e| anyhow!("Failed to parse JSON: {}", e))?;

        // Check if this is an account notification
        if let Some(method) = json.get("method").and_then(|v| v.as_str()) {
            if method == "accountNotification" {
                return self.handle_account_notification(&json).await;
            }
        }

        // Check if this is a subscription response
        if let Some(request_id) = json.get("id").and_then(|v| v.as_u64()) {
            // This is a response to our subscription request
            if let Some(result) = json.get("result") {
                if let Some(subscription_id) = result.as_u64() {
                    // Map request_id to bonding_curve, then store subscription_id -> bonding_curve
                    let bonding_curve_opt = {
                        let pending = pending_subscriptions.read().await;
                        pending.get(&request_id).cloned()
                    };

                    if let Some(bonding_curve_str) = bonding_curve_opt {
                        // Remove from pending
                        {
                            let mut pending = pending_subscriptions.write().await;
                            pending.remove(&request_id);
                        }

                        // Store subscription_id -> bonding_curve mapping
                        {
                            let mut sub_to_bc = self.subscription_to_bonding_curve.write().await;
                            sub_to_bc.insert(subscription_id, bonding_curve_str.clone());
                        }

                        // Store bonding_curve -> subscription_id mapping
                        {
                            let mut subscriptions = self.subscriptions.write().await;
                            subscriptions.insert(bonding_curve_str.clone(), subscription_id);
                        }

                        eprintln!("[ACCOUNT_SUBSCRIPTION] Subscription confirmed: {} -> sub_id {}", bonding_curve_str, subscription_id);
                    }
                }
            }
            if let Some(error) = json.get("error") {
                // Remove from pending on error
                {
                    let mut pending = pending_subscriptions.write().await;
                    pending.remove(&request_id);
                }
                eprintln!("[ACCOUNT_SUBSCRIPTION] Subscription error (request_id {}): {:?}", request_id, error);
            }
        }

        Ok(())
    }

    /// Handle account notification and update PNL
    async fn handle_account_notification(&self, notification: &Value) -> Result<()> {
        let params = notification.get("params")
            .ok_or_else(|| anyhow!("Missing params in notification"))?;

        // Get subscription ID from notification
        let subscription_id = params.get("subscription")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| anyhow!("Missing subscription ID in notification"))?;

        // Find bonding curve address from subscription ID
        let bonding_curve_str = {
            let sub_to_bc = self.subscription_to_bonding_curve.read().await;
            sub_to_bc.get(&subscription_id).cloned()
        };

        let bonding_curve_str = match bonding_curve_str {
            Some(bc) => bc,
            None => {
                eprintln!("[ACCOUNT_SUBSCRIPTION] Unknown subscription ID: {}", subscription_id);
                return Ok(()); // Not our subscription, ignore
            }
        };

        let result = params.get("result")
            .ok_or_else(|| anyhow!("Missing result in params"))?;

        let value = result.get("value")
            .ok_or_else(|| anyhow!("Missing value in result"))?;

        // Extract account data
        let data = value.get("data")
            .and_then(|d| d.as_array())
            .and_then(|arr| arr.get(0))
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("Missing or invalid account data"))?;

        // Decode base64 data
        use base64::{engine::general_purpose, Engine as _};
        let account_data = general_purpose::STANDARD.decode(data)
            .map_err(|e| anyhow!("Failed to decode base64: {}", e))?;

        // Deserialize bonding curve account
        const BONDING_CURVE_SIZE: usize = 8 + 8 + 8 + 8 + 8 + 8 + 1; // 57 bytes
        let data_slice = if account_data.len() >= BONDING_CURVE_SIZE {
            &account_data[..BONDING_CURVE_SIZE]
        } else {
            &account_data[..]
        };

        let curve = BondingCurveAccount::try_from_slice(data_slice)
            .map_err(|e| anyhow!("Failed to deserialize bonding curve: {}", e))?;

        // Get current price
        let current_price = curve.get_token_price_sol();

        // Find mint that matches this bonding curve
        let mint_opt = {
            if let Ok(tracker_guard) = self.tracker.read() {
                if let Some(tracker_ref) = tracker_guard.as_ref() {
                    tracker_ref.get_active_positions()
                        .into_iter()
                        .find(|p| p.bonding_curve.as_ref() == Some(&bonding_curve_str))
                        .map(|p| p.mint)
                } else {
                    None
                }
            } else {
                None
            }
        };

        if let Some(mint) = mint_opt {
            // Update PNL for this position
            let mint_short = if mint.len() > 8 { format!("{}...{}", &mint[..6], &mint[mint.len()-4..]) } else { mint.clone() };
            eprintln!("[ACCOUNT_SUBSCRIPTION] 🔄 Real-time price update for {}: {:.10} SOL/token", mint_short, current_price);
            if let Ok(mut tracker_guard) = self.tracker.try_write() {
                if let Some(tracker) = tracker_guard.as_mut() {
                    if let Err(e) = tracker.update_position_pnl_fast(&mint, current_price) {
                        eprintln!("[ACCOUNT_SUBSCRIPTION] ❌ Failed to update PNL for {}: {}", mint_short, e);
                    } else {
                        eprintln!("[ACCOUNT_SUBSCRIPTION] ✅ PNL updated for {}", mint_short);
                    }
                }
            }
        } else {
            eprintln!("[ACCOUNT_SUBSCRIPTION] ⚠️  No active position found for bonding curve: {}", bonding_curve_str);
        }

        Ok(())
    }
}

impl SubscriptionHandle {
    /// Subscribe to bonding curve account changes
    pub fn subscribe(&self, bonding_curve: Pubkey) -> Result<()> {
        self.command_tx.send(SubscriptionCommand::Subscribe(bonding_curve))
            .map_err(|e| anyhow!("Failed to send subscribe command: {}", e))
    }

    /// Unsubscribe from bonding curve account
    pub fn unsubscribe(&self, bonding_curve: Pubkey) -> Result<()> {
        self.command_tx.send(SubscriptionCommand::Unsubscribe(bonding_curve))
            .map_err(|e| anyhow!("Failed to send unsubscribe command: {}", e))
    }
}
