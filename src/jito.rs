// jito.rs - ULTRA FAST BUNDLE SUBMISSION
#![allow(unused_imports)]

use anyhow::{anyhow, Result};
use solana_sdk::{
    pubkey::Pubkey,
    signature::{Keypair, Signature, Signer},
    transaction::VersionedTransaction,
    system_instruction,
    message::{v0, VersionedMessage},
};
use std::str::FromStr;
use rand::seq::SliceRandom;
use crate::constants::{JITO_TIP_ACCOUNTS, JITO_ENDPOINTS};

pub struct JitoClient {
    http_client: reqwest::Client,
    endpoint: String,
}

impl JitoClient {
    pub fn new() -> Self {
        // Random endpoint za load balancing
        let mut rng = rand::thread_rng();
        let endpoint = JITO_ENDPOINTS.choose(&mut rng).unwrap().to_string();

        Self {
            http_client: crate::utils::get_shared_http_client().clone(),
            endpoint,
        }
    }

    /// Šalje bundle sa tip transakcijom
    pub async fn send_bundle(
        &self,
        buy_tx: &VersionedTransaction,  // ⚡ Borrowed
        tip_lamports: u64,
        wallet: &Keypair,
        recent_blockhash: solana_sdk::hash::Hash,
    ) -> Result<String> {
        // 1. Kreiraj tip transaction
        let tip_account = self.get_random_tip_account()?;
        let tip_tx = self.create_tip_transaction(wallet, &tip_account, tip_lamports, recent_blockhash)?;

        // 2. Bundle = [buy_tx, tip_tx]
        let bundle = vec![
            Self::serialize_transaction(buy_tx)?,
            Self::serialize_transaction(&tip_tx)?,
        ];

        // 3. Šalji na Jito
        let url = format!("{}/api/v1/bundles", self.endpoint);
        let response = self.http_client
            .post(&url)
            .header("Content-Type", "application/json")
            .json(&serde_json::json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "sendBundle",
                "params": [bundle]
            }))
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("Network error sending bundle to {}: {}", url, e))?;

        let status = response.status();
        if !status.is_success() {
            let error_text = response.text().await
                .unwrap_or_else(|_| format!("HTTP {}", status));
            return Err(anyhow::anyhow!("Jito bundle failed ({}): {}", status, error_text));
        }

        let result: serde_json::Value = response.json().await
            .map_err(|e| anyhow::anyhow!("Failed to parse Jito response: {}", e))?;

        if let Some(bundle_id) = result["result"].as_str() {
            Ok(bundle_id.to_string())
        } else if let Some(error) = result["error"].as_object() {
            let error_code = error.get("code").and_then(|v| v.as_i64()).unwrap_or(0);
            let error_msg = error.get("message")
                .and_then(|v| v.as_str())
                .unwrap_or("Unknown error");
            Err(anyhow::anyhow!("Jito error (code {}): {}", error_code, error_msg))
        } else {
            Err(anyhow::anyhow!("Unexpected Jito response format: {:?}", result))
        }
    }

    /// Kreira tip transakciju
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

    /// Random tip account
    fn get_random_tip_account(&self) -> Result<Pubkey> {
        let mut rng = rand::thread_rng();
        let account_str = JITO_TIP_ACCOUNTS.choose(&mut rng).unwrap();
        Ok(Pubkey::from_str(account_str)?)
    }

    /// Serialize TX za bundle
    fn serialize_transaction(tx: &VersionedTransaction) -> Result<String> {
        let serialized = bincode::serialize(tx)?;
        Ok(bs58::encode(serialized).into_string())
    }
}

// Quick send funkcija
pub async fn send_jito_bundle(
    buy_tx: VersionedTransaction,  // ⚡ Takes ownership (no &)
    wallet: &Keypair,
    recent_blockhash: solana_sdk::hash::Hash,
    tip_lamports: u64,
) -> Result<String> {
    let jito = JitoClient::new();
    jito.send_bundle(&buy_tx, tip_lamports, wallet, recent_blockhash).await
    //              ^^^^^^^ pass reference to send_bundle
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_jito_client_creation() {
        let client = JitoClient::new();
        assert!(JITO_ENDPOINTS.contains(&client.endpoint.as_str()));
    }

    #[test]
    fn test_random_tip_account() {
        let client = JitoClient::new();
        let tip = client.get_random_tip_account().unwrap();
        assert!(JITO_TIP_ACCOUNTS.iter().any(|&acc| {
            Pubkey::from_str(acc).unwrap() == tip
        }));
    }

    #[test]
    fn test_create_tip_transaction() {
        use solana_sdk::signature::Keypair;
        use solana_sdk::hash::Hash;

        let wallet = Keypair::new();
        let tip_account = Pubkey::from_str(JITO_TIP_ACCOUNTS[0]).unwrap();
        let recent_blockhash = Hash::default();
        let tip_lamports = 1_500_000;

        let client = JitoClient::new();
        let tip_tx = client.create_tip_transaction(&wallet, &tip_account, tip_lamports, recent_blockhash);

        assert!(tip_tx.is_ok());
        let tx = tip_tx.unwrap();
        assert_eq!(tx.signatures.len(), 1);
    }

    #[test]
    fn test_serialize_transaction() {
        use solana_sdk::{
            signature::{Keypair, Signer},
            pubkey::Pubkey,
            system_instruction,
            message::v0,
            transaction::VersionedTransaction,
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

        let serialized = JitoClient::serialize_transaction(&tx);
        assert!(serialized.is_ok());
        
        let serialized_str = serialized.unwrap();
        assert!(!serialized_str.is_empty());
        // Base58 encoded string should be longer than raw bytes
        assert!(serialized_str.len() > 100);
    }

    #[tokio::test]
    async fn test_send_bundle_mock() {
        use mockito::Server;
        let mut server = Server::new_async().await;
        use solana_sdk::{
            signature::Keypair,
            system_instruction,
            message::v0,
            transaction::VersionedTransaction,
            hash::Hash,
        };

        let _server = Server::new_async().await;
        let wallet = Keypair::new();
        let recipient = Pubkey::new_unique();
        let recent_blockhash = Hash::default();

        // Create a test transaction
        let ix = system_instruction::transfer(&wallet.pubkey(), &recipient, 1000);
        let msg = v0::Message::try_compile(
            &wallet.pubkey(),
            &[ix],
            &[],
            recent_blockhash,
        ).unwrap();

        let buy_tx = VersionedTransaction::try_new(
            solana_sdk::message::VersionedMessage::V0(msg),
            &[&wallet],
        ).unwrap();

        // Mock successful bundle submission
        let mock = server.mock("POST", "/api/v1/bundles")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"jsonrpc":"2.0","id":1,"result":"test-bundle-id-123"}"#)
            .create();

        // Create client with mock endpoint (base URL only, send_bundle adds /api/v1/bundles)
        let mut client = JitoClient::new();
        client.endpoint = server.url();

        let result = client.send_bundle(
            &buy_tx,
            1_500_000,
            &wallet,
            recent_blockhash,
        ).await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "test-bundle-id-123");
        mock.assert();
    }

    #[tokio::test]
    async fn test_send_bundle_error_handling() {
        use mockito::Server;
        use solana_sdk::{
            signature::{Keypair, Signer},
            pubkey::Pubkey,
            system_instruction,
            message::v0,
            transaction::VersionedTransaction,
            hash::Hash,
        };
        
        let mut server = Server::new_async().await;

        let _server = Server::new_async().await;
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

        let buy_tx = VersionedTransaction::try_new(
            solana_sdk::message::VersionedMessage::V0(msg),
            &[&wallet],
        ).unwrap();

        // Mock error response
        let mock = server.mock("POST", "/api/v1/bundles")
            .with_status(400)
            .with_header("content-type", "application/json")
            .with_body(r#"{"jsonrpc":"2.0","id":1,"error":{"code":-32602,"message":"Invalid params"}}"#)
            .create();

        let mut client = JitoClient::new();
        client.endpoint = server.url();

        let result = client.send_bundle(
            &buy_tx,
            1_500_000,
            &wallet,
            recent_blockhash,
        ).await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Jito bundle failed"));
        mock.assert();
    }

    #[tokio::test]
    #[ignore]
    async fn test_jito_bundle_submission() {
        // Integration test - requires real Jito endpoint and valid transaction
        // This should be run manually with proper setup
    }
}
