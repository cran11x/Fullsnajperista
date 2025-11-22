// helius.rs - SIMPLIFIED: Tip already in transaction with retry logic

use anyhow::{anyhow, Result};
use solana_sdk::transaction::VersionedTransaction;

const MAX_RETRIES: u32 = 3;
const RETRY_DELAY_MS: u64 = 100;

use crate::constants::SENDER_ENDPOINT;

pub struct HeliusSender {
    http_client: reqwest::Client,
    endpoint: String,
}

impl HeliusSender {
    /// Creates new Helius Sender client with proper TLS
    pub fn new() -> Result<Self> {
        // Use shared HTTP client for better performance
        let http_client = crate::utils::get_shared_http_client().clone();

        Ok(Self {
            http_client,
            endpoint: SENDER_ENDPOINT.to_string(),
        })
    }

    /// Send transaction via Helius Sender
    /// (Tip must already be included in the transaction!)
    pub async fn send_transaction(
        &self,
        tx: &VersionedTransaction,
    ) -> Result<String> {
        // Serialize transaction
        let tx_b64 = Self::serialize_transaction(tx)?;

        // Send via Helius Sender
        let response = self.http_client
            .post(&self.endpoint)
            .header("Content-Type", "application/json")
            .json(&serde_json::json!({
                "jsonrpc": "2.0",
                "id": chrono::Utc::now().timestamp_millis().to_string(),
                "method": "sendTransaction",
                "params": [
                    tx_b64,
                    {
                        "encoding": "base64",
                        "skipPreflight": true,  // Mandatory for Helius Sender
                        "maxRetries": 0
                    }
                ]
            }))
            .send()
            .await
            .map_err(|e| anyhow!("Network error: {}", e))?;

        let status = response.status();
        if !status.is_success() {
            let error_text = response.text().await
                .unwrap_or_else(|_| format!("HTTP {}", status));
            return Err(anyhow!("Helius Sender failed ({}): {}", status, error_text));
        }

        let result: serde_json::Value = response.json().await
            .map_err(|e| anyhow!("Failed to parse Helius response: {}", e))?;

        if let Some(signature) = result["result"].as_str() {
            Ok(signature.to_string())
        } else if let Some(error) = result["error"].as_object() {
            let error_code = error.get("code").and_then(|v| v.as_i64()).unwrap_or(0);
            let error_msg = error.get("message")
                .and_then(|v| v.as_str())
                .unwrap_or("Unknown error");
            Err(anyhow!("Helius error (code {}): {}", error_code, error_msg))
        } else {
            Err(anyhow!("Unexpected Helius response format: {:?}", result))
        }
    }

    /// Serialize transaction to base64
    fn serialize_transaction(tx: &VersionedTransaction) -> Result<String> {
        let serialized = bincode::serialize(tx)?;
        use base64::{engine::general_purpose, Engine as _};
        Ok(general_purpose::STANDARD.encode(serialized))
    }
}

/// Quick send function with retry - Tip must already be in transaction!
pub async fn send_helius_transaction(
    tx: VersionedTransaction,
) -> Result<String> {
    let sender = HeliusSender::new()?;
    let mut last_error = None;
    
    for attempt in 1..=MAX_RETRIES {
        match sender.send_transaction(&tx).await {
            Ok(result) => return Ok(result),
            Err(e) => {
                last_error = Some(e);
                if attempt < MAX_RETRIES {
                    tokio::time::sleep(std::time::Duration::from_millis(
                        RETRY_DELAY_MS * (1 << (attempt - 1))
                    )).await;
                }
            }
        }
    }
    
    Err(last_error.unwrap_or_else(|| anyhow::anyhow!("Failed after {} attempts", MAX_RETRIES)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_helius_sender_creation() {
        let sender = HeliusSender::new();
        assert!(sender.is_ok(), "Failed to create Helius sender: {:?}", sender.err());
    }

    #[test]
    fn test_serialize_transaction() {
        use solana_sdk::{
            signature::{Keypair, Signer},
            pubkey::Pubkey,
            system_instruction,
            message::v0,
            hash::Hash,
        };

        let wallet = Keypair::new();
        let recipient = Pubkey::new_unique();
        let recent_blockhash = Hash::default();

        let ix = system_instruction::transfer(&wallet.pubkey(), &recipient, 1000);
        let msg = v0::Message::try_compile(
            &wallet.pubkey(),
            &[ix],
            &[],
            recent_blockhash,
        ).unwrap();

        let tx = VersionedTransaction::try_new(
            solana_sdk::message::VersionedMessage::V0(msg),
            &[&wallet],
        ).unwrap();

        let serialized = HeliusSender::serialize_transaction(&tx);
        assert!(serialized.is_ok());
        
        let serialized_str = serialized.unwrap();
        assert!(!serialized_str.is_empty());
        // Base64 encoded string
        assert!(serialized_str.len() > 100);
    }

    #[tokio::test]
    async fn test_send_transaction_mock() {
        use mockito::Server;
        use solana_sdk::{
            signature::{Keypair, Signer},
            pubkey::Pubkey,
            system_instruction,
            message::v0,
            hash::Hash,
        };

        let mut server = Server::new_async().await;
        let wallet = Keypair::new();
        let recipient = Pubkey::new_unique();
        let recent_blockhash = Hash::default();

        let ix = system_instruction::transfer(&wallet.pubkey(), &recipient, 1000);
        let msg = v0::Message::try_compile(
            &wallet.pubkey(),
            &[ix],
            &[],
            recent_blockhash,
        ).unwrap();

        let tx = VersionedTransaction::try_new(
            solana_sdk::message::VersionedMessage::V0(msg),
            &[&wallet],
        ).unwrap();

        // Mock successful response
        let mock = server.mock("POST", "/fast")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"jsonrpc":"2.0","id":"123","result":"5VERv8NMvzbJMEkV8xnrLkEaWRtSz9CosKDYjCJjBRnbJLgp8uirBgmQpjKhoR4tjF3ZpRzrFmBV6UjKdiSZkQUW"}"#)
            .create();

        let mut sender = HeliusSender::new().unwrap();
        sender.endpoint = format!("{}/fast", server.url());

        let result = sender.send_transaction(&tx).await;

        assert!(result.is_ok());
        assert!(result.unwrap().starts_with("5VER"));
        mock.assert();
    }

    #[tokio::test]
    async fn test_send_transaction_error_handling() {
        use mockito::Server;
        use solana_sdk::{
            signature::{Keypair, Signer},
            pubkey::Pubkey,
            system_instruction,
            message::v0,
            hash::Hash,
        };

        let mut server = Server::new_async().await;
        let wallet = Keypair::new();
        let recipient = Pubkey::new_unique();
        let recent_blockhash = Hash::default();

        let ix = system_instruction::transfer(&wallet.pubkey(), &recipient, 1000);
        let msg = v0::Message::try_compile(
            &wallet.pubkey(),
            &[ix],
            &[],
            recent_blockhash,
        ).unwrap();

        let tx = VersionedTransaction::try_new(
            solana_sdk::message::VersionedMessage::V0(msg),
            &[&wallet],
        ).unwrap();

        // Mock error response
        let mock = server.mock("POST", "/fast")
            .with_status(400)
            .with_header("content-type", "application/json")
            .with_body(r#"{"jsonrpc":"2.0","id":"123","error":{"code":-32602,"message":"Invalid params"}}"#)
            .create();

        let mut sender = HeliusSender::new().unwrap();
        sender.endpoint = format!("{}/fast", server.url());

        let result = sender.send_transaction(&tx).await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Helius Sender failed"));
        mock.assert();
    }

    #[tokio::test]
    #[ignore]
    async fn test_helius_transaction_submission() {
        // Integration test - requires real Helius endpoint and valid transaction
        // This should be run manually with proper setup
    }
}