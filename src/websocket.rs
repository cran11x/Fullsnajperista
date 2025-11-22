// websocket.rs - WEBSOCKET CONNECTION HANDLING
#![allow(unused_imports, dead_code)]

use anyhow::Result;
use solana_sdk::pubkey::Pubkey;
use std::str::FromStr;
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message as WsMessage};
use futures_util::{StreamExt, SinkExt};

use crate::constants::PUMP_PROGRAM_ID;

/// Check if notification is an initialize bonding curve event
pub fn is_initialize_bonding_curve(notification: &serde_json::Value) -> bool {
    if let Some(logs) = notification["params"]["result"]["value"]["logs"].as_array() {
        let has_pump = logs.iter().any(|l|
            l.as_str().map_or(false, |s|
                s.contains("Program 6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P invoke [1]")
            )
        );

        let has_create = logs.iter().any(|l|
            l.as_str().map_or(false, |s|
                s.contains("Program log: Instruction: Create")
            )
        );

        return has_pump && has_create;
    }
    false
}

/// Extract signature from notification
pub fn extract_signature(notification: &serde_json::Value) -> Option<String> {
    notification["params"]["result"]["value"]["signature"]
        .as_str()
        .map(|s| s.to_string())
}

/// Create WebSocket subscription message
pub fn create_subscribe_message() -> Result<serde_json::Value> {
    let pump_program = Pubkey::from_str(PUMP_PROGRAM_ID)?;
    Ok(serde_json::json!({
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
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_initialize_bonding_curve() {
        // Valid notification
        let notification = serde_json::json!({
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

        assert!(is_initialize_bonding_curve(&notification));

        // Missing pump program
        let notification2 = serde_json::json!({
            "params": {
                "result": {
                    "value": {
                        "logs": [
                            "Program log: Instruction: Create"
                        ]
                    }
                }
            }
        });

        assert!(!is_initialize_bonding_curve(&notification2));

        // Missing create
        let notification3 = serde_json::json!({
            "params": {
                "result": {
                    "value": {
                        "logs": [
                            "Program 6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P invoke [1]"
                        ]
                    }
                }
            }
        });

        assert!(!is_initialize_bonding_curve(&notification3));
    }

    #[test]
    fn test_extract_signature() {
        let notification = serde_json::json!({
            "params": {
                "result": {
                    "value": {
                        "signature": "5VERv8NMvzbJMEkV8xnrLkEaWRtSz9CosKDYjCJjBRnbJLgp8uirBgmQpjKhoR4tjF3ZpRzrFmBV6UjKdiSZkQUW"
                    }
                }
            }
        });

        let sig = extract_signature(&notification);
        assert!(sig.is_some());
        assert_eq!(sig.unwrap(), "5VERv8NMvzbJMEkV8xnrLkEaWRtSz9CosKDYjCJjBRnbJLgp8uirBgmQpjKhoR4tjF3ZpRzrFmBV6UjKdiSZkQUW");

        // Missing signature
        let notification2 = serde_json::json!({
            "params": {
                "result": {
                    "value": {}
                }
            }
        });

        assert!(extract_signature(&notification2).is_none());
    }

    #[test]
    fn test_create_subscribe_message() {
        let msg = create_subscribe_message().unwrap();
        
        assert_eq!(msg["method"], "logsSubscribe");
        assert_eq!(msg["jsonrpc"], "2.0");
        assert!(msg["params"].is_array());
    }
}

