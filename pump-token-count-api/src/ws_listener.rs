// ws_listener.rs - Helius WebSocket listener for real-time CREATE event tracking
use anyhow::Result;
use futures_util::{SinkExt, StreamExt};
use redis::aio::ConnectionManager;
use serde_json::json;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::time::{sleep, Duration};
use tokio_tungstenite::{connect_async, tungstenite::Message};

const PUMP_PROGRAM_ID: &str = "6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P";
const WS_URL_TEMPLATE: &str = "wss://mainnet.helius-rpc.com/?api-key={}";
const REDIS_KEY_PREFIX: &str = "pump:creates:";
const REDIS_TTL_SECONDS: u64 = 30 * 24 * 60 * 60; // 30 days

// Track last update timestamp for health check
static LAST_UPDATE: AtomicU64 = AtomicU64::new(0);

/// Get last update timestamp (Unix seconds)
pub fn get_last_update() -> u64 {
    LAST_UPDATE.load(Ordering::Relaxed)
}

/// Start WebSocket listener with exponential backoff reconnection
pub async fn start_listener(
    api_key: String,
    redis_pool: Arc<redis::aio::ConnectionManager>,
) -> Result<()> {
    let mut backoff_seconds = 1u64;
    let max_backoff = 60u64;

    loop {
        let ws_url = WS_URL_TEMPLATE.replace("{}", &api_key);
        tracing::info!("Connecting to Helius WebSocket...");

        match connect_async(&ws_url).await {
            Ok((ws_stream, _)) => {
                tracing::info!("WebSocket connected");
                backoff_seconds = 1; // Reset backoff on successful connection

                let (mut write, mut read) = ws_stream.split();

                // Send programSubscribe request
                let subscribe_msg = json!({
                    "jsonrpc": "2.0",
                    "id": 1,
                    "method": "programSubscribe",
                    "params": [
                        PUMP_PROGRAM_ID,
                        {"commitment": "processed"}
                    ]
                });

                if let Err(e) = write.send(Message::Text(subscribe_msg.to_string())).await {
                    tracing::error!("Failed to send subscribe: {}", e);
                    sleep(Duration::from_secs(backoff_seconds)).await;
                    backoff_seconds = (backoff_seconds * 2).min(max_backoff);
                    continue;
                }

                // Listen for notifications
                while let Some(msg) = read.next().await {
                    match msg {
                        Ok(Message::Text(text)) => {
                            let mut redis_conn = (*redis_pool).clone();
                            if let Err(e) = handle_notification(&text, &mut redis_conn).await {
                                tracing::warn!("Error handling notification: {}", e);
                            }
                        }
                        Ok(Message::Close(_)) => {
                            tracing::warn!("WebSocket closed by server");
                            break;
                        }
                        Err(e) => {
                            tracing::error!("WebSocket error: {}", e);
                            break;
                        }
                        _ => {}
                    }
                }
            }
            Err(e) => {
                tracing::error!("Failed to connect: {}, retrying in {}s", e, backoff_seconds);
            }
        }

        sleep(Duration::from_secs(backoff_seconds)).await;
        backoff_seconds = (backoff_seconds * 2).min(max_backoff);
    }
}

/// Handle WebSocket notification
async fn handle_notification(
    text: &str,
    redis: &mut ConnectionManager,
) -> Result<()> {
    let notification: serde_json::Value = serde_json::from_str(text)?;

    // Check if this is a CREATE instruction
    if let Some(logs) = notification["params"]["result"]["value"]["logs"].as_array() {
        let has_create = logs.iter().any(|l| {
            l.as_str()
                .map_or(false, |s| s.contains("Program log: Instruction: Create"))
        });

        if !has_create {
            return Ok(()); // Not a CREATE instruction
        }

        // CRITICAL FIX #1: Extract creator correctly
        // Creator is NOT always account_keys[0] - fee-payer (Helius) might be first
        // Find first signer that is NOT pump.fun program and NOT ComputeBudget
        if let Some(account_keys) = notification["params"]["result"]["value"]["accountKeys"]
            .as_array()
        {
            let creator = account_keys
                .iter()
                .find_map(|key| {
                    let key_str = key.as_str()?;
                    if key_str != PUMP_PROGRAM_ID
                        && !key_str.starts_with("ComputeBudget")
                        && !key_str.contains("SystemProgram")
                        && !key_str.contains("11111111111111111111111111111111")
                    {
                        Some(key_str.to_string())
                    } else {
                        None
                    }
                });

            if let Some(creator) = creator {
                let redis_key = format!("{}{}", REDIS_KEY_PREFIX, creator);

                // INCR and set TTL (CRITICAL FIX #4: TTL on every INCR)
                let mut pipe = redis::pipe();
                pipe.incr(&redis_key, 1u64)
                    .expire(&redis_key, REDIS_TTL_SECONDS as i64);
                let _: () = pipe.query_async(redis).await?;

                // Update last update timestamp
                LAST_UPDATE.store(
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_secs(),
                    Ordering::Relaxed,
                );

                tracing::debug!("Incremented count for creator: {}", creator);
            } else {
                tracing::warn!("Could not find valid creator in account_keys");
            }
        }
    }

    Ok(())
}

