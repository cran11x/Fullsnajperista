// jito.rs - ULTRA FAST BUNDLE SUBMISSION

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

// Jito tip accounts (rotacija za load balancing)
const JITO_TIP_ACCOUNTS: [&str; 8] = [
    "96gYZGLnJYVFmbjzopPSU6QiEV5fGqZNyN9nmNhvrZU5",
    "HFqU5x63VTqvQss8hp11i4wVV8bD44PvwucfZ2bU7gRe",
    "Cw8CFyM9FkoMi7K7Crf6HNQqf4uEMzpKw6QNghXLvLkY",
    "ADaUMid9yfUytqMBgopwjb2DTLSokTSzL1zt6iGPaS49",
    "DfXygSm4jCyNCybVYYK6DwvWqjKee8pbDmJGcLWNDXjh",
    "ADuUkR4vqLUMWXxW9gh6D6L8pMSawimctcNZ5pGwDcEt",
    "DttWaMuVvTiduZRnguLF7jNxTgiMBZ1hyAumKUiL2KRL",
    "3AVi9Tg9Uo68tJfuvoKvqKNWKkC5wPdSSdeBnizKZ6jT",
];

const JITO_ENDPOINTS: [&str; 4] = [
    "https://frankfurt.mainnet.block-engine.jito.wtf",
    "https://amsterdam.mainnet.block-engine.jito.wtf",
    "https://mainnet.block-engine.jito.wtf",
    "https://ny.mainnet.block-engine.jito.wtf",

];

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
            http_client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(5))
                .build()
                .unwrap(),
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
        let response = self.http_client
            .post(format!("{}/api/v1/bundles", self.endpoint))
            .header("Content-Type", "application/json")
            .json(&serde_json::json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "sendBundle",
                "params": [bundle]
            }))
            .send()
            .await?;

        if !response.status().is_success() {
            let error_text = response.text().await?;
            return Err(anyhow!("Jito bundle failed: {}", error_text));
        }

        let result: serde_json::Value = response.json().await?;

        if let Some(bundle_id) = result["result"].as_str() {
            Ok(bundle_id.to_string())
        } else if let Some(error) = result["error"].as_object() {
            Err(anyhow!("Jito error: {:?}", error))
        } else {
            Err(anyhow!("Unexpected Jito response: {:?}", result))
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
}