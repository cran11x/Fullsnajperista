// detection.rs - FIXED: User Volume se NE ekstraktuje (izvodi se u buy.rs)

use anyhow::{anyhow, Result};
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::{pubkey::Pubkey, commitment_config::CommitmentConfig, system_program};
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
    // ❌ UKLONJEN user_volume - izvodi se za svakog korisnika posebno!
    pub global: Pubkey,
    pub fee_recipient: Pubkey,
    pub fee_config: Pubkey,
    pub fee_program: Pubkey,
}

impl PumpBuyAccounts {
    pub async fn validate(&self, rpc: &RpcClient) -> Result<()> {
        println!("\n🔍 Validating accounts...");

        match rpc.get_account(&self.creator_vault).await {
            Ok(account) => {
                if account.owner != system_program::id() {
                    return Err(anyhow!(
                        "❌ INVALID creator_vault: {} owned by {} (expected System)",
                        self.creator_vault, account.owner
                    ));
                }
                println!("   ✅ Creator Vault: System account");
            }
            Err(e) => return Err(anyhow!("❌ Cannot fetch creator_vault: {}", e)),
        }

        match rpc.get_account(&self.associated_bonding_curve).await {
            Ok(account) => {
                if account.owner != spl_token::id() {
                    return Err(anyhow!(
                        "❌ INVALID associated_bonding_curve: {} owned by {}",
                        self.associated_bonding_curve, account.owner
                    ));
                }
                println!("   ✅ Associated BC: Token account");
            }
            Err(_) => {
                println!("   ⚠️  Associated BC: Not created yet (will be created on first buy)");
            }
        }

        println!("✅ All accounts validated!\n");
        Ok(())
    }

    pub async fn from_existing_buy_tx(
        rpc: &RpcClient,
        mint: &Pubkey,
    ) -> Result<Self> {
        println!("\n⚡ Fast mode: Waiting for Buy TX (max 3s)...");

        let pump_program = Pubkey::from_str(PUMP_PROGRAM_ID)?;
        let (bonding_curve, _) = Pubkey::find_program_address(
            &[b"bonding-curve", &mint.to_bytes()],
            &pump_program,
        );

        for attempt in 1..=6 {


            let signatures = match rpc
                .get_signatures_for_address_with_config(
                    &bonding_curve,
                    solana_client::rpc_client::GetConfirmedSignaturesForAddress2Config {
                        limit: Some(5),
                        ..Default::default()
                    },
                )
                .await {
                Ok(sigs) => sigs,
                Err(_) => {
                    println!("   ⏳ Attempt {}/6: RPC error, retrying...", attempt);
                    continue;
                }
            };

            if signatures.is_empty() {
                println!("   ⏳ Attempt {}/6: No Buy TX yet...", attempt);
                continue;
            }

            println!("   ✅ Found {} signatures, checking...", signatures.len());

            for sig_info in signatures {
                if let Ok(sig) = solana_sdk::signature::Signature::from_str(&sig_info.signature) {
                    let tx = match rpc.get_transaction_with_config(
                        &sig,
                        solana_client::rpc_config::RpcTransactionConfig {
                            encoding: Some(solana_transaction_status::UiTransactionEncoding::Base64),
                            max_supported_transaction_version: Some(0),
                            commitment: Some(CommitmentConfig::confirmed()),
                        }
                    ).await {
                        Ok(tx) => tx,
                        Err(_) => continue,
                    };

                    if let solana_transaction_status::EncodedTransaction::Binary(encoded, _) = &tx.transaction.transaction {
                        use base64::{engine::general_purpose, Engine as _};
                        use solana_sdk::message::VersionedMessage;
                        use solana_sdk::transaction::VersionedTransaction;

                        let tx_bytes = match general_purpose::STANDARD.decode(encoded) {
                            Ok(b) => b,
                            Err(_) => continue,
                        };

                        let versioned_tx: VersionedTransaction = match bincode::deserialize(&tx_bytes) {
                            Ok(tx) => tx,
                            Err(_) => continue,
                        };

                        let account_keys = match &versioned_tx.message {
                            VersionedMessage::Legacy(msg) => &msg.account_keys,
                            VersionedMessage::V0(msg) => &msg.account_keys,
                        };

                        let instructions = match &versioned_tx.message {
                            VersionedMessage::Legacy(msg) => &msg.instructions,
                            VersionedMessage::V0(msg) => &msg.instructions,
                        };

                        for ix in instructions {
                            let program_id_idx = ix.program_id_index as usize;
                            if let Some(&program_id) = account_keys.get(program_id_idx) {
                                if program_id == pump_program {
                                    let ix_accounts: Vec<Pubkey> = ix.accounts
                                        .iter()
                                        .filter_map(|&idx| account_keys.get(idx as usize).copied())
                                        .collect();

                                    if ix_accounts.len() >= 15 {
                                        println!("✅ Found Buy TX with {} instruction accounts\n", ix_accounts.len());

                                        let global = ix_accounts[0];
                                        let fee_recipient = ix_accounts[1];
                                        let bonding_curve = ix_accounts[3];
                                        let associated_bonding_curve = ix_accounts[4];
                                        let creator_vault = ix_accounts[9];
                                        let event_authority = ix_accounts[10];

                                        let global_volume = ix_accounts.get(12).copied()
                                            .unwrap_or_else(|| Pubkey::from_str("Hq2wp8uJ9jCPsYgNHex8RtqdvMPfVGoYwjvF1ATiwn2Y").unwrap());

                                        let fee_config = ix_accounts.get(14).copied()
                                            .unwrap_or_else(|| Pubkey::from_str("8Wf5TiAheLUqBrKXeYg2JtAFFMWtKdG2BSFgqUcPVwTt").unwrap());

                                        let fee_program = ix_accounts.get(15).copied()
                                            .unwrap_or_else(|| Pubkey::from_str("pfeeUxB6jkeY1Hxd7CsFCAjcbHA9rWtchMGdZ6VojVZ").unwrap());

                                        println!("📊 EXTRACTED ACCOUNTS:");
                                        println!("   Creator Vault: {}", creator_vault);
                                        println!("   Event Authority: {}", event_authority);
                                        println!("   Global Volume: {}", global_volume);
                                        println!("   ℹ️  User Volume: Will be derived for your wallet");

                                        let result = PumpBuyAccounts {
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

                                        result.validate(rpc).await?;

                                        return Ok(result);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        Err(anyhow!("⏱️ Timeout: No Buy TX found after 3 seconds"))
    }

    pub async fn from_initialize_tx(
        rpc: &RpcClient,
        mint: &Pubkey,
        _signature: &str,
    ) -> Result<Self> {
        Self::from_existing_buy_tx(rpc, mint).await
    }
}