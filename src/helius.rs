// helius.rs - SIMPLIFIED: Tip already in transaction

use anyhow::{anyhow, Result};
use solana_sdk::transaction::VersionedTransaction;

/// Helius Sender endpoint (global HTTPS)
const SENDER_ENDPOINT: &str = "https://sender.helius-rpc.com/fast";

pub struct HeliusSender {
    http_client: reqwest::Client,
    endpoint: String,
}

impl HeliusSender {
    /// Creates new Helius Sender client with proper TLS
    pub fn new() -> Result<Self> {
        let http_client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(5))
            .build()
            .map_err(|e| {
                if e.is_builder() {
                    anyhow!("TLS Error: {}. Check Cargo.toml has rustls-tls feature!", e)
                } else {
                    anyhow!("HTTP Client Error: {}", e)
                }
            })?;

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

        if !response.status().is_success() {
            let error_text = response.text().await?;
            return Err(anyhow!("Helius Sender failed: {}", error_text));
        }

        let result: serde_json::Value = response.json().await?;

        if let Some(signature) = result["result"].as_str() {
            Ok(signature.to_string())
        } else if let Some(error) = result["error"].as_object() {
            Err(anyhow!("Helius error: {:?}", error))
        } else {
            Err(anyhow!("Unexpected response: {:?}", result))
        }
    }

    /// Serialize transaction to base64
    fn serialize_transaction(tx: &VersionedTransaction) -> Result<String> {
        let serialized = bincode::serialize(tx)?;
        Ok(base64::encode(serialized))
    }
}

/// Quick send function - Tip must already be in transaction!
pub async fn send_helius_transaction(
    tx: VersionedTransaction,
) -> Result<String> {
    let sender = HeliusSender::new()?;
    sender.send_transaction(&tx).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_helius_sender_creation() {
        let sender = HeliusSender::new();
        assert!(sender.is_ok(), "Failed to create Helius sender: {:?}", sender.err());
    }
}