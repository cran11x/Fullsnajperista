// helius.rs - ULTRA-LOW LATENCY VIA HELIUS SENDER + Official SDK

use anyhow::{anyhow, Result};
use solana_sdk::{
    pubkey::Pubkey,
    signature::{Keypair, Signer},
    transaction::VersionedTransaction,
    system_instruction,
    message::{v0, VersionedMessage},
};
use std::str::FromStr;
use rand::seq::SliceRandom;

// ⚡ Option 1: Use built-in reqwest (if helius SDK not available)
// ⚡ Option 2: Use Helius SDK client (recommended!)

/// Helius Sender tip accounts (from official docs)
const HELIUS_TIP_ACCOUNTS: [&str; 10] = [
    "4ACfpUFoaSD9bfPdeu6DBt89gB6ENTeHBXCAi87NhDEE",
    "D2L6yPZ2FmmmTKPgzaMKdhu6EWZcTpLy1Vhx8uvZe7NZ",
    "9bnz4RShgq1hAnLnZbP8kbgBg1kEmcJBYQq3gQbmnSta",
    "5VY91ws6B2hMmBFRsXkoAAdsPHBJwRfBht4DXox3xkwn",
    "2nyhqdwKcJZR2vcqCyrYsaPVdAnFoJjiksCXJ7hfEYgD",
    "2q5pghRs6arqVjRvT5gfgWfWcHWmw1ZuCzphgd5KfWGJ",
    "wyvPkWjVZz1M8fHQnMMCDTQDbkManefNNhweYk5WkcF",
    "3KCKozbAaF75qEU33jtzozcJ29yJuaLJTy2jFdzUY8bT",
    "4vieeGHPYPG2MmyPRcYjdiDmmhN3ww7hsFNap8pVN3Ey",
    "4TQLFNWK8AovT1gFvda5jfw2oJeRMKEmw7aH6MGBJ3or",
];

/// Helius Sender endpoint (global HTTPS)
const SENDER_ENDPOINT: &str = "https://sender.helius-rpc.com/fast";

/// Regional endpoints for backend optimization (HTTP)
#[allow(dead_code)]
const SENDER_ENDPOINTS: [&str; 7] = [
    "http://slc-sender.helius-rpc.com/fast",  // Salt Lake City
    "http://ewr-sender.helius-rpc.com/fast",  // Newark
    "http://lon-sender.helius-rpc.com/fast",  // London
    "http://fra-sender.helius-rpc.com/fast",  // Frankfurt
    "http://ams-sender.helius-rpc.com/fast",  // Amsterdam
    "http://sg-sender.helius-rpc.com/fast",   // Singapore
    "http://tyo-sender.helius-rpc.com/fast",  // Tokyo
];

pub struct HeliusSender {
    http_client: reqwest::Client,
    endpoint: String,
}

impl HeliusSender {
    /// Creates new Helius Sender client with proper TLS
    pub fn new() -> Result<Self> {
        // ⚡ CRITICAL: Use rustls-tls for Windows compatibility!
        let http_client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(5))
            .build()
            .map_err(|e| {
                if e.is_builder() {
                    anyhow!("TLS Error: {}. Make sure Cargo.toml has rustls-tls feature!", e)
                } else {
                    anyhow!("HTTP Client Error: {}", e)
                }
            })?;

        Ok(Self {
            http_client,
            endpoint: SENDER_ENDPOINT.to_string(),
        })
    }

    /// Send transaction via Helius Sender (dual routing to validators + Jito)
    pub async fn send_transaction(
        &self,
        buy_tx: &VersionedTransaction,
        tip_lamports: u64,
        wallet: &Keypair,
        recent_blockhash: solana_sdk::hash::Hash,
    ) -> Result<String> {
        // 1. Create tip transaction
        let tip_account = self.get_random_tip_account()?;
        let tip_tx = self.create_tip_transaction(wallet, &tip_account, tip_lamports, recent_blockhash)?;

        // 2. Serialize both transactions
        let buy_tx_b64 = Self::serialize_transaction(buy_tx)?;
        let tip_tx_b64 = Self::serialize_transaction(&tip_tx)?;

        // 3. Send buy TX via Helius Sender
        let response = self.http_client
            .post(&self.endpoint)
            .header("Content-Type", "application/json")
            .json(&serde_json::json!({
                "jsonrpc": "2.0",
                "id": chrono::Utc::now().timestamp_millis().to_string(),
                "method": "sendTransaction",
                "params": [
                    buy_tx_b64,
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
            // Also send tip TX (fire and forget - best effort)
            let _ = self.send_tip_transaction(&tip_tx_b64).await;
            Ok(signature.to_string())
        } else if let Some(error) = result["error"].as_object() {
            Err(anyhow!("Helius error: {:?}", error))
        } else {
            Err(anyhow!("Unexpected response: {:?}", result))
        }
    }

    /// Send tip transaction (fire and forget)
    async fn send_tip_transaction(&self, tip_tx_b64: &str) -> Result<()> {
        let _ = self.http_client
            .post(&self.endpoint)
            .header("Content-Type", "application/json")
            .json(&serde_json::json!({
                "jsonrpc": "2.0",
                "id": chrono::Utc::now().timestamp_millis().to_string(),
                "method": "sendTransaction",
                "params": [
                    tip_tx_b64,
                    {
                        "encoding": "base64",
                        "skipPreflight": true,
                        "maxRetries": 0
                    }
                ]
            }))
            .send()
            .await;

        Ok(())
    }

    /// Creates tip transaction
    fn create_tip_transaction(
        &self,
        wallet: &Keypair,
        tip_account: &Pubkey,
        tip_lamports: u64,
        recent_blockhash: solana_sdk::hash::Hash,
    ) -> Result<VersionedTransaction> {
        let tip_ix = system_instruction::transfer(
            &wallet.pubkey(),
            tip_account,
            tip_lamports,
        );

        let msg = v0::Message::try_compile(
            &wallet.pubkey(),
            &[tip_ix],
            &[],
            recent_blockhash,
        )?;

        Ok(VersionedTransaction::try_new(
            VersionedMessage::V0(msg),
            &[wallet],
        )?)
    }

    /// Random tip account selection
    fn get_random_tip_account(&self) -> Result<Pubkey> {
        let mut rng = rand::thread_rng();
        let account_str = HELIUS_TIP_ACCOUNTS.choose(&mut rng).unwrap();
        Ok(Pubkey::from_str(account_str)?)
    }

    /// Serialize transaction to base64
    fn serialize_transaction(tx: &VersionedTransaction) -> Result<String> {
        let serialized = bincode::serialize(tx)?;
        Ok(base64::encode(serialized))
    }
}

/// Quick send function (same interface as jito.rs)
pub async fn send_helius_transaction(
    buy_tx: VersionedTransaction,
    wallet: &Keypair,
    recent_blockhash: solana_sdk::hash::Hash,
    tip_lamports: u64,
) -> Result<String> {
    let sender = HeliusSender::new()?;
    sender.send_transaction(&buy_tx, tip_lamports, wallet, recent_blockhash).await
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
    fn test_random_tip_account() {
        let sender = HeliusSender::new().unwrap();
        let tip = sender.get_random_tip_account().unwrap();
        assert!(HELIUS_TIP_ACCOUNTS.iter().any(|&acc| {
            Pubkey::from_str(acc).unwrap() == tip
        }));
    }
}