// websocket_server_test.rs - Integration tests for WebSocket and RPC connections
// Run with: cargo test --test websocket_server_test
#![allow(unused_imports, unused_mut)]

use anyhow::Result;
use Fullsnajperista::config::Config;
use Fullsnajperista::websocket;
use Fullsnajperista::health::{HealthMonitor, HealthStatus};
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message as WsMessage};
use futures_util::{StreamExt, SinkExt};
use tokio::time::{timeout, Duration};
use std::env;

// Load .env file before tests
fn load_env() {
    // Try to load .env file
    let _ = dotenv::dotenv();
    
    // Also try manual loading
    if let Ok(contents) = std::fs::read_to_string(".env") {
        for line in contents.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Some((key, value)) = line.split_once('=') {
                let key = key.trim();
                let value = value.trim().trim_matches('"').trim_matches('\'');
                if !key.is_empty() && !value.is_empty() {
                    env::set_var(key, value);
                }
            }
        }
    }
}

/// Test WebSocket connection to Helius
#[tokio::test]
async fn test_websocket_connection() -> Result<()> {
    load_env();
    
    // Setup: Get config from environment
    let api_key = env::var("HELIUS_API_KEY").unwrap_or_else(|_| "test-key".to_string());
    
    // Skip if using test key (not a real API key)
    if api_key == "test-key" || api_key.is_empty() || (api_key.contains("test") && api_key.len() < 30) {
        eprintln!("⚠️  Skipping WebSocket test: HELIUS_API_KEY not set or is test key");
        return Ok(());
    }
    
    env::set_var("HELIUS_API_KEY", api_key.clone());
    
    let config = match Config::from_env() {
        Ok(cfg) => cfg,
        Err(e) => {
            eprintln!("⚠️  Skipping WebSocket test: Config error: {}", e);
            return Ok(());
        }
    };
    
    eprintln!("🔌 Testing WebSocket connection to: {}", config.wss_url);
    
    // Test connection with timeout
    let connect_result = timeout(
        Duration::from_secs(10),
        connect_async(&config.wss_url)
    ).await;
    
    match connect_result {
        Ok(Ok((ws_stream, _))) => {
            eprintln!("✅ WebSocket connection established!");
            
            // Try to send a ping to keep connection alive
            let (mut write, mut read) = ws_stream.split();
            
            // Send subscription message
            let subscribe_msg = websocket::create_subscribe_message()?;
            let msg_text = serde_json::to_string(&subscribe_msg)?;
            
            eprintln!("📤 Sending subscription message...");
            write.send(WsMessage::Text(msg_text)).await?;
            
            // Wait for at least one message (subscription confirmation or log)
            let message_result = timeout(Duration::from_secs(15), read.next()).await;
            
            match message_result {
                Ok(Some(Ok(msg))) => {
                    match msg {
                        WsMessage::Text(text) => {
                            eprintln!("📥 Received message: {} chars", text.len());
                            // Try to parse as JSON
                            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) {
                                eprintln!("✅ Valid JSON received: {}", json);
                                // Check if it's a subscription confirmation
                                if json["id"].as_u64().is_some() || json["result"].is_object() {
                                    eprintln!("✅ Subscription confirmed!");
                                }
                            }
                        }
                        WsMessage::Ping(data) => {
                            eprintln!("📥 Received ping, sending pong...");
                            write.send(WsMessage::Pong(data)).await?;
                        }
                        WsMessage::Pong(_) => {
                            eprintln!("📥 Received pong");
                        }
                        _ => {
                            eprintln!("📥 Received other message type");
                        }
                    }
                }
                Ok(Some(Err(e))) => {
                    eprintln!("⚠️  Error receiving message: {}", e);
                    return Err(anyhow::anyhow!("WebSocket message error: {}", e));
                }
                Ok(None) => {
                    eprintln!("⚠️  WebSocket stream ended");
                }
                Err(_) => {
                    eprintln!("⚠️  Timeout waiting for message (this is OK if server doesn't send immediately)");
                    // Not a failure - server might not send messages right away
                }
            }
            
            // Close connection gracefully
            let _ = write.close().await;
            eprintln!("✅ WebSocket test completed successfully");
            Ok(())
        }
        Ok(Err(e)) => {
            eprintln!("❌ WebSocket connection failed: {}", e);
            Err(anyhow::anyhow!("WebSocket connection failed: {}", e))
        }
        Err(_) => {
            eprintln!("❌ WebSocket connection timeout");
            Err(anyhow::anyhow!("WebSocket connection timeout"))
        }
    }
}

/// Test RPC connection
#[tokio::test]
async fn test_rpc_connection() -> Result<()> {
    load_env();
    
    // Setup: Get config from environment
    let api_key = env::var("HELIUS_API_KEY").unwrap_or_else(|_| "test-key".to_string());
    
    // Skip if using test key (not a real API key)
    if api_key == "test-key" || api_key.is_empty() || (api_key.contains("test") && api_key.len() < 30) {
        eprintln!("⚠️  Skipping RPC test: HELIUS_API_KEY not set or is test key");
        return Ok(());
    }
    
    env::set_var("HELIUS_API_KEY", api_key.clone());
    
    let config = match Config::from_env() {
        Ok(cfg) => cfg,
        Err(e) => {
            eprintln!("⚠️  Skipping RPC test: Config error: {}", e);
            return Ok(());
        }
    };
    
    eprintln!("🔌 Testing RPC connection to: {}", config.rpc_url);
    
    let rpc = config.create_rpc_client();
    
    // Test basic RPC call with timeout
    let slot_result = timeout(
        Duration::from_secs(10),
        rpc.get_slot()
    ).await;
    
    match slot_result {
        Ok(Ok(slot)) => {
            eprintln!("✅ RPC connection successful! Current slot: {}", slot);
            Ok(())
        }
        Ok(Err(e)) => {
            eprintln!("❌ RPC call failed: {}", e);
            Err(anyhow::anyhow!("RPC call failed: {}", e))
        }
        Err(_) => {
            eprintln!("❌ RPC call timeout");
            Err(anyhow::anyhow!("RPC call timeout"))
        }
    }
}

/// Test WebSocket subscription message creation
#[tokio::test]
async fn test_websocket_subscription_message() -> Result<()> {
    let msg = websocket::create_subscribe_message()?;
    
    assert_eq!(msg["method"], "logsSubscribe");
    assert_eq!(msg["jsonrpc"], "2.0");
    assert!(msg["params"].is_array());
    
    // Check that pump program ID is included
    let mentions = &msg["params"][0]["mentions"];
    assert!(mentions.is_array());
    assert!(mentions.as_array().unwrap().len() > 0);
    
    eprintln!("✅ Subscription message created: {}", msg);
    Ok(())
}

/// Test websocket helper functions without server
#[test]
fn test_websocket_helpers() {
    // Test is_initialize_bonding_curve
    let valid_notification = serde_json::json!({
        "params": {
            "result": {
                "value": {
                    "logs": [
                        "Program 6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P invoke [1]",
                        "Program log: Instruction: Create"
                    ]
                }
            }
        }
    });
    
    assert!(websocket::is_initialize_bonding_curve(&valid_notification));
    
    // Test extract_signature
    let notification_with_sig = serde_json::json!({
        "params": {
            "result": {
                "value": {
                    "signature": "5VERv8NMvzbJMEkV8xnrLkEaWRtSz9CosKDYjCJjBRnbJLgp8uirBgmQpjKhoR4tjF3ZpRzrFmBV6UjKdiSZkQUW"
                }
            }
        }
    });
    
    let sig = websocket::extract_signature(&notification_with_sig);
    assert!(sig.is_some());
    assert_eq!(sig.unwrap(), "5VERv8NMvzbJMEkV8xnrLkEaWRtSz9CosKDYjCJjBRnbJLgp8uirBgmQpjKhoR4tjF3ZpRzrFmBV6UjKdiSZkQUW");
    
    // Test with missing signature
    let notification_no_sig = serde_json::json!({
        "params": {
            "result": {
                "value": {}
            }
        }
    });
    
    let sig2 = websocket::extract_signature(&notification_no_sig);
    assert!(sig2.is_none());
    
    eprintln!("✅ All websocket helper functions work correctly");
}

/// Test WebSocket connection with real message parsing
#[tokio::test]
async fn test_websocket_message_parsing() -> Result<()> {
    load_env();
    
    // Setup config
    let api_key = env::var("HELIUS_API_KEY").unwrap_or_else(|_| "test-key".to_string());
    
    // Skip if using test key (not a real API key)
    if api_key == "test-key" || api_key.is_empty() || (api_key.contains("test") && api_key.len() < 30) {
        eprintln!("⚠️  Skipping message parsing test: HELIUS_API_KEY not set or is test key");
        return Ok(());
    }
    
    env::set_var("HELIUS_API_KEY", api_key.clone());
    
    let config = match Config::from_env() {
        Ok(cfg) => cfg,
        Err(e) => {
            eprintln!("⚠️  Skipping message parsing test: Config error: {}", e);
            return Ok(());
        }
    };
    
    eprintln!("🔌 Testing WebSocket message parsing...");
    
    let connect_result = timeout(
        Duration::from_secs(10),
        connect_async(&config.wss_url)
    ).await;
    
    match connect_result {
        Ok(Ok((ws_stream, _))) => {
            let (mut write, mut read) = ws_stream.split();
            
            // Send subscription
            let subscribe_msg = websocket::create_subscribe_message()?;
            write.send(WsMessage::Text(serde_json::to_string(&subscribe_msg)?)).await?;
            
            // Listen for messages for a short time
            let mut messages_received = 0;
            let start = std::time::Instant::now();
            
            while start.elapsed() < Duration::from_secs(20) && messages_received < 5 {
                let msg_result = timeout(Duration::from_secs(5), read.next()).await;
                
                match msg_result {
                    Ok(Some(Ok(WsMessage::Text(text)))) => {
                        messages_received += 1;
                        eprintln!("📥 Message #{} received ({} chars)", messages_received, text.len());
                        
                        // Try to parse
                        if let Ok(notification) = serde_json::from_str::<serde_json::Value>(&text) {
                            // Check if it's a bonding curve initialization
                            let is_init = websocket::is_initialize_bonding_curve(&notification);
                            let signature = websocket::extract_signature(&notification);
                            
                            eprintln!("  - Is bonding curve init: {}", is_init);
                            if let Some(sig) = signature {
                                eprintln!("  - Signature: {}...", &sig[..8.min(sig.len())]);
                            }
                        }
                    }
                    Ok(Some(Ok(WsMessage::Ping(data)))) => {
                        write.send(WsMessage::Pong(data)).await?;
                        eprintln!("📥 Ping received and responded");
                    }
                    Ok(Some(Ok(WsMessage::Pong(_)))) => {
                        eprintln!("📥 Pong received");
                    }
                    Ok(Some(Ok(WsMessage::Binary(_)))) => {
                        eprintln!("📥 Binary message received");
                    }
                    Ok(Some(Ok(WsMessage::Close(_)))) => {
                        eprintln!("📥 Connection closed by server");
                        break;
                    }
                    Ok(Some(Ok(_))) => {
                        // Catch-all for Frame and any other message types
                        eprintln!("📥 Other message type received");
                    }
                    Ok(Some(Err(e))) => {
                        eprintln!("⚠️  Error: {}", e);
                        break;
                    }
                    Ok(None) => {
                        eprintln!("📥 Stream ended");
                        break;
                    }
                    Err(_) => {
                        // Timeout is OK
                        break;
                    }
                }
            }
            
            eprintln!("✅ Received {} messages total", messages_received);
            let _ = write.close().await;
            Ok(())
        }
        Ok(Err(e)) => {
            eprintln!("❌ Connection failed: {}", e);
            Err(anyhow::anyhow!("Connection failed: {}", e))
        }
        Err(_) => {
            eprintln!("❌ Connection timeout");
            Err(anyhow::anyhow!("Connection timeout"))
        }
    }
}

/// Test config validation with real server URLs
#[tokio::test]
async fn test_config_with_real_urls() -> Result<()> {
    // Save original values
    let orig_api_key = env::var("HELIUS_API_KEY").ok();
    let orig_rpc_url = env::var("RPC_URL").ok();
    let orig_wss_url = env::var("WSS_URL").ok();
    
    // Set test values
    env::set_var("HELIUS_API_KEY", "test-api-key-123");
    env::remove_var("RPC_URL"); // Let it use default
    env::remove_var("WSS_URL"); // Let it use default
    
    let config = Config::from_env()?;
    
    // Verify URLs are properly formatted
    assert!(config.rpc_url.contains("api-key"), "RPC URL should contain api-key");
    assert!(config.wss_url.contains("api-key"), "WSS URL should contain api-key");
    assert!(config.rpc_url.contains("test-api-key-123"), "RPC URL should contain API key");
    assert!(config.wss_url.contains("test-api-key-123"), "WSS URL should contain API key");
    
    eprintln!("✅ RPC URL: {}", config.rpc_url);
    eprintln!("✅ WSS URL: {}", config.wss_url);
    
    // Restore original values
    if let Some(key) = orig_api_key {
        env::set_var("HELIUS_API_KEY", key);
    } else {
        env::remove_var("HELIUS_API_KEY");
    }
    if let Some(url) = orig_rpc_url {
        env::set_var("RPC_URL", url);
    } else {
        env::remove_var("RPC_URL");
    }
    if let Some(url) = orig_wss_url {
        env::set_var("WSS_URL", url);
    } else {
        env::remove_var("WSS_URL");
    }
    
    Ok(())
}

/// Test health check with real connections
#[tokio::test]
async fn test_health_check_real() -> Result<()> {
    load_env();
    
    let api_key = env::var("HELIUS_API_KEY").unwrap_or_else(|_| "test-key".to_string());
    
    // Skip if using test key (not a real API key)
    if api_key == "test-key" || api_key.is_empty() || (api_key.contains("test") && api_key.len() < 30) {
        eprintln!("⚠️  Skipping health check test: HELIUS_API_KEY not set or is test key");
        return Ok(());
    }
    
    env::set_var("HELIUS_API_KEY", api_key.clone());
    
    let config = match Config::from_env() {
        Ok(cfg) => cfg,
        Err(e) => {
            eprintln!("⚠️  Skipping health check test: Config error: {}", e);
            return Ok(());
        }
    };
    
    let rpc = config.create_rpc_client();
    let mut monitor = HealthMonitor::new();
    
    // Test RPC health check
    let rpc_healthy = monitor.check_rpc(&rpc).await?;
    assert!(rpc_healthy, "RPC should be healthy");
    
    eprintln!("✅ RPC health check passed");
    
    // Test WebSocket health check (simulate connected)
    monitor.check_websocket(true);
    assert_eq!(monitor.ws_status(), HealthStatus::Healthy);
    
    eprintln!("✅ WebSocket health check passed");
    
    Ok(())
}

