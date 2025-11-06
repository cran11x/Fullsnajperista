// detection.rs - FIXED: Extract creator from CREATE instruction account[8]

use anyhow::{anyhow, Result};
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::{pubkey::Pubkey, commitment_config::CommitmentConfig};
use std::str::FromStr;

const PUMP_PROGRAM_ID: &str = "6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P";
const BUY_DISCRIMINATOR: [u8; 8] = [0x66, 0x06, 0x3d, 0x12, 0x01, 0xda, 0xeb, 0xea];

// 🔧 TOGGLE THIS: true = detailed logs, false = normal logs
const DEBUG: bool = true;

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
    pub dev_buy_sol: u64,
    pub creator: Pubkey,
}

impl PumpBuyAccounts {
    pub async fn from_initialize_tx(
        rpc: &RpcClient,
        init_signature: &str,
    ) -> Result<(Self, Pubkey)> {
        let sig = solana_sdk::signature::Signature::from_str(init_signature)?;

        let mut attempts = 0;
        let max_attempts = 8;

        let tx = loop {
            attempts += 1;

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

                    let wait_ms = match attempts {
                        1 => 100,
                        2 => 150,
                        3 => 200,
                        4 => 300,
                        _ => 400,
                    };

                    println!("      ⏳ Waiting for TX... ({}/{}, {}ms)", attempts, max_attempts, wait_ms);
                    tokio::time::sleep(tokio::time::Duration::from_millis(wait_ms)).await;
                }
            }
        };

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
            let mint = account_keys.get(1).ok_or_else(|| anyhow!("No mint found"))?;

            // ✅ FIXED: Get creator from first signer (payer of transaction)
            let creator = match &versioned_tx.message {
                VersionedMessage::Legacy(msg) => {
                    msg.account_keys.get(0).copied()
                        .ok_or_else(|| anyhow!("No signer found"))?
                }
                VersionedMessage::V0(msg) => {
                    msg.account_keys.get(0).copied()
                        .ok_or_else(|| anyhow!("No signer found"))?
                }
            };

            if DEBUG {
                println!("      ✅ Creator (signer): {}", creator);
            }

            let mut dev_buy_sol = 0u64;
            let mut creator_vault: Option<Pubkey> = None;

            if DEBUG {
                println!("      🔍 DEBUG: TX {} instructions, {} accounts",
                         instructions.len(), account_keys.len());
            }

            for (ix_idx, ix) in instructions.iter().enumerate() {
                let program_id_idx = ix.program_id_index as usize;
                if let Some(&program_id) = account_keys.get(program_id_idx) {
                    if program_id == pump_program {
                        let ix_accounts: Vec<Pubkey> = ix.accounts
                            .iter()
                            .filter_map(|&idx| account_keys.get(idx as usize).copied())
                            .collect();

                        if ix.data.len() >= 8 {
                            let discriminator = &ix.data[0..8];

                            if DEBUG {
                                println!("      🔍 IX[{}]: Pump instruction, discriminator={:02x?}",
                                         ix_idx, discriminator);
                            }

                            // ✅ Extract vault from CREATE instruction
                            if discriminator == &[0x18, 0x1e, 0xc8, 0x28, 0x05, 0x1c, 0x07, 0x77] {
                                if ix_accounts.len() > 9 {
                                    creator_vault = Some(ix_accounts[9]);
                                    if DEBUG {
                                        println!("      ✅ Creator vault: {}", ix_accounts[9]);
                                    }
                                }
                            }

                            if discriminator == BUY_DISCRIMINATOR {
                                if ix.data.len() >= 24 {
                                    let max_sol_bytes = &ix.data[16..24];
                                    let raw_value = u64::from_le_bytes(max_sol_bytes.try_into().unwrap_or([0u8; 8]));

                                    // 🔥 SANITY CHECK: If > 100 SOL, might be wrong parsing
                                    if raw_value > 100_000_000_000 { // 100 SOL in lamports
                                        if DEBUG {
                                            println!("      ⚠️  Suspicious value: {} ({} SOL) - might be parsing error",
                                                     raw_value, raw_value as f64 / 1e9);
                                        }
                                        // Don't set dev_buy_sol if unrealistic
                                    } else {
                                        dev_buy_sol = raw_value;

                                        if DEBUG {
                                            let token_amount = u64::from_le_bytes(ix.data[8..16].try_into().unwrap());
                                            println!("      ✅ BUY found: {} SOL, {} tokens, bytes={:02x?}",
                                                     dev_buy_sol as f64 / 1e9, token_amount, max_sol_bytes);
                                        } else {
                                            println!("      💰 Dev buy: {} SOL", dev_buy_sol as f64 / 1e9);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            if dev_buy_sol == 0 {
                if DEBUG {
                    println!("      ⚠️  No BUY instruction - checking balances...");
                }

                if let Some(meta) = &tx.transaction.meta {
                    let pre_balances = &meta.pre_balances;
                    let post_balances = &meta.post_balances;

                    if DEBUG {
                        println!("      📊 Balance changes:");
                        for i in 0..pre_balances.len().min(10) {
                            if i < post_balances.len() {
                                let pre = pre_balances[i];
                                let post = post_balances[i];
                                let diff = if pre > post {
                                    format!("-{:.4}", (pre - post) as f64 / 1e9)
                                } else {
                                    format!("+{:.4}", (post - pre) as f64 / 1e9)
                                };
                                println!("         [{}] {} SOL", i, diff);
                            }
                        }
                    }

                    // 🔥 Check ALL accounts for spending, not just 6-7
                    for i in 0..pre_balances.len().min(10) {
                        if i < post_balances.len() {
                            let pre = pre_balances[i];
                            let post = post_balances[i];
                            if pre > post {
                                let spent = pre - post;
                                // Look for significant spends (>0.1 SOL, but not rent ~0.002)
                                if spent > 100_000_000 && spent < 100_000_000_000 { // 0.1-100 SOL
                                    dev_buy_sol = spent;
                                    if DEBUG {
                                        println!("      ⚠️  Balance fallback [{}]: {} SOL",
                                                 i, dev_buy_sol as f64 / 1e9);
                                    } else {
                                        println!("      💰 Dev buy (balances): {} SOL", dev_buy_sol as f64 / 1e9);
                                    }
                                    break;
                                }
                            }
                        }
                    }
                }
            }

            if DEBUG && dev_buy_sol > 0 {
                println!("      📋 Final: {} SOL", dev_buy_sol as f64 / 1e9);
            }

            // ✅ Check if we found creator_vault
            if creator_vault.is_none() {
                println!("      ❌ EXTRACTION FAILED - DEBUG INFO:");
                println!("         mint: {:?}", mint);
                println!("         creator: {:?}", creator);
                println!("         dev_buy_sol: {} SOL", dev_buy_sol as f64 / 1e9);
                println!("         creator_vault: None");
                println!("         Total instructions analyzed: {}", instructions.len());

                // Show which instructions were found
                let mut pump_ix_count = 0;
                let mut buy_ix_count = 0;
                for (ix_idx, ix) in instructions.iter().enumerate() {
                    let program_id_idx = ix.program_id_index as usize;
                    if let Some(&program_id) = account_keys.get(program_id_idx) {
                        if program_id == pump_program {
                            pump_ix_count += 1;
                            if ix.data.len() >= 8 {
                                let disc = &ix.data[0..8];
                                println!("         IX[{}]: Pump.fun, discriminator={:02x?}, {} accounts",
                                         ix_idx, disc, ix.accounts.len());
                                if disc == &BUY_DISCRIMINATOR {
                                    buy_ix_count += 1;
                                }
                            }
                        }
                    }
                }

                println!("         Pump.fun instructions found: {}", pump_ix_count);
                println!("         BUY instructions found: {}", buy_ix_count);

                if buy_ix_count == 0 {
                    println!("      💡 REASON: No BUY instruction found - dev created token but didn't buy");
                } else {
                    println!("      💡 REASON: BUY instruction found but creator_vault missing");
                }

                return Err(anyhow!("Token created without buy - dev didn't buy"));
            }

            // ✅ Unwrap creator_vault safely - we checked above
            let creator_vault = creator_vault.unwrap();

            let (bonding_curve, _) = Pubkey::find_program_address(
                &[b"bonding-curve", &mint.to_bytes()],
                &pump_program,
            );

            let associated_bonding_curve =
                spl_associated_token_account::get_associated_token_address(
                    &bonding_curve,
                    mint
                );

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
                dev_buy_sol,
                creator, // ✅ Now using correct creator from CREATE instruction
            };

            return Ok((accounts, *mint));
        }

        // If we get here, something went wrong during parsing
        println!("      ❌ FATAL: Failed to parse transaction");
        println!("         This should never happen - check transaction format");
        Err(anyhow!("Failed to extract accounts - transaction parse error"))
    }

    pub async fn check_creator_token_count(
        rpc: &RpcClient,
        creator: &Pubkey,
    ) -> Result<usize> {
        use solana_client::rpc_config::RpcTransactionConfig;
        use solana_transaction_status::UiTransactionEncoding;

        let pump_program = Pubkey::from_str(PUMP_PROGRAM_ID)?;

        let sigs = match rpc.get_signatures_for_address(creator).await {
            Ok(s) => s,
            Err(_) => return Ok(0),
        };

        let mut token_count = 0;

        for sig_info in sigs.iter().take(50) {
            let sig = match solana_sdk::signature::Signature::from_str(&sig_info.signature) {
                Ok(s) => s,
                Err(_) => continue,
            };

            let tx_result = rpc.get_transaction_with_config(
                &sig,
                RpcTransactionConfig {
                    encoding: Some(UiTransactionEncoding::Json),
                    max_supported_transaction_version: Some(0),
                    commitment: Some(CommitmentConfig::confirmed()),
                }
            ).await;

            if let Ok(tx) = tx_result {
                if let Some(meta) = tx.transaction.meta {
                    let logs: Option<Vec<String>> = meta.log_messages.into();

                    if let Some(log_messages) = logs {
                        let has_pump = log_messages.iter().any(|log|
                            log.contains(&pump_program.to_string())
                        );

                        let has_create = log_messages.iter().any(|log|
                            log.contains("Program log: Instruction: Create")
                        );

                        if has_pump && has_create {
                            token_count += 1;
                        }
                    }
                }
            }

            if token_count > 10 {
                break;
            }
        }

        Ok(token_count)
    }
}