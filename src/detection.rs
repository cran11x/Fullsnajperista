// detection.rs - WITH RETRY LOGIC!

use anyhow::{anyhow, Result};
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::{pubkey::Pubkey, commitment_config::CommitmentConfig};
use std::str::FromStr;

const PUMP_PROGRAM_ID: &str = "6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P";

#[derive(Debug, Clone)]
pub struct PumpBuyAccounts {
    pub mint: Pubkey,
    pub bonding_curve: Pubkey,
    pub associated_bonding_curve: Pubkey,
    pub creator_vault: Pubkey,
    pub event_authority: Pubkey,
    pub global_volume: Pubkey,
    pub global: Pubkey,
    pub fee_recipient: Pubkey,
    pub fee_config: Pubkey,
    pub fee_program: Pubkey,
}

impl PumpBuyAccounts {
    pub async fn from_initialize_tx(
        rpc: &RpcClient,
        init_signature: &str,
    ) -> Result<(Self, Pubkey)> {
        let sig = solana_sdk::signature::Signature::from_str(init_signature)?;

        // ⚡ RETRY LOGIC - Wait for TX to be available!
        let mut attempts = 0;
        let max_attempts = 8;  // Try for ~2.4 seconds

        let tx = loop {
            attempts += 1;

            // Try to fetch TX
            match rpc.get_transaction_with_config(
                &sig,
                solana_client::rpc_config::RpcTransactionConfig {
                    encoding: Some(solana_transaction_status::UiTransactionEncoding::Base64),
                    max_supported_transaction_version: Some(0),
                    commitment: Some(CommitmentConfig::confirmed()),
                }
            ).await {
                Ok(tx) => {
                    println!("      ✅ TX ready! (attempt {})", attempts);
                    break tx;
                }
                Err(e) => {
                    if attempts >= max_attempts {
                        return Err(anyhow!("TX not available after {} attempts: {}", max_attempts, e));
                    }

                    // Wait progressively longer
                    let wait_ms = match attempts {
                        1 => 100,
                        2 => 150,
                        3 => 200,
                        4 => 300,
                        _ => 400,
                    };

                    println!("      ⏳ Waiting for TX... ({}/{}, {}ms)",
                             attempts, max_attempts, wait_ms);
                    tokio::time::sleep(tokio::time::Duration::from_millis(wait_ms)).await;
                }
            }
        };

        // Parse transaction
        if let solana_transaction_status::EncodedTransaction::Binary(encoded, _) = &tx.transaction.transaction {
            use base64::{engine::general_purpose, Engine as _};
            use solana_sdk::message::VersionedMessage;
            use solana_sdk::transaction::VersionedTransaction;

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

            // Extract mint (account #1)
            let mint = account_keys.get(1).ok_or_else(|| anyhow!("No mint found"))?;

            // Find creator_vault from Buy instruction
            let mut creator_vault: Option<Pubkey> = None;

            for ix in instructions {
                let program_id_idx = ix.program_id_index as usize;
                if let Some(&program_id) = account_keys.get(program_id_idx) {
                    if program_id == pump_program {
                        let ix_accounts: Vec<Pubkey> = ix.accounts
                            .iter()
                            .filter_map(|&idx| account_keys.get(idx as usize).copied())
                            .collect();

                        if ix_accounts.len() >= 15 {
                            creator_vault = Some(ix_accounts[9]);
                            break;
                        }
                    }
                }
            }

            if let Some(creator_vault) = creator_vault {
                let (bonding_curve, _) = Pubkey::find_program_address(
                    &[b"bonding-curve", &mint.to_bytes()],
                    &pump_program,
                );

                let associated_bonding_curve =
                    spl_associated_token_account::get_associated_token_address(
                        &bonding_curve,
                        mint
                    );

                // Static accounts
                let global = Pubkey::from_str("4wTV1YmiEkRvAtNtsSGPtUrqRYQMe5SKy2uB4Jjaxnjf")?;
                let fee_recipient = Pubkey::from_str("CebN5WGQ4jvEPvsVU4EoHEpgzq1VV7AbicfhtW4xC9iM")?;
                let event_authority = Pubkey::from_str("Ce6TQqeHC9p8KetsN6JsjHK7UTZk7nasjjnr7XxXp9F1")?;
                let global_volume = Pubkey::from_str("Hq2wp8uJ9jCPsYgNHex8RtqdvMPfVGoYwjvF1ATiwn2Y")?;
                let fee_config = Pubkey::from_str("8Wf5TiAheLUqBrKXeYg2JtAFFMWtKdG2BSFgqUcPVwTt")?;
                let fee_program = Pubkey::from_str("pfeeUxB6jkeY1Hxd7CsFCAjcbHA9rWtchMGdZ6VojVZ")?;

                let accounts = PumpBuyAccounts {
                    mint: *mint,
                    bonding_curve,
                    associated_bonding_curve,
                    creator_vault,
                    event_authority,
                    global_volume,
                    global,
                    fee_recipient,
                    fee_config,
                    fee_program,
                };

                return Ok((accounts, *mint));
            }
        }

        Err(anyhow!("Failed to extract accounts"))
    }
}