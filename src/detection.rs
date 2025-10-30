// detection.rs - Ekstrakcija account-a iz Pump.Fun transakcija

use anyhow::{anyhow, Result};
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::{pubkey::Pubkey, signature::Signature, commitment_config::CommitmentConfig};
use std::str::FromStr;

const PUMP_PROGRAM_ID: &str = "6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P";

/// Ekstraktovani account-i iz Pump.Fun transakcija
#[derive(Debug, Clone)]
pub struct PumpBuyAccounts {
    pub mint: Pubkey,
    pub bonding_curve: Pubkey,
    pub associated_bonding_curve: Pubkey,

    // Pravi account-i (ne ATA):
    pub creator_vault: Pubkey,      // System account za SOL fee
    pub global_volume: Pubkey,      // Global stats PDA
    pub user_volume: Pubkey,        // User stats PDA

    // Static accounts
    pub global: Pubkey,
    pub fee_recipient: Pubkey,
    pub event_authority: Pubkey,
    pub fee_config: Pubkey,
    pub fee_program: Pubkey,
}

impl PumpBuyAccounts {
    /// Ekstraktuje account-e iz InitializeBondingCurve transakcije
    pub async fn from_initialize_tx(
        rpc: &RpcClient,
        mint: &Pubkey,
        signature: &str,
    ) -> Result<Self> {
        println!("\n🔍 Fetching InitializeBondingCurve transaction...");

        let sig = Signature::from_str(signature)?;

        // Fetch sa retry
        let tx = Self::fetch_tx_with_retry(rpc, &sig, 5).await?;

        // Ekstraktuj sve account-e iz initialize instrukcije
        Self::extract_from_initialize_instruction(&tx, mint)
    }

    /// Kopira account-e iz prvog postojećeg Buy TX-a (NAJSIGURNIJE)
    pub async fn from_existing_buy_tx(
        rpc: &RpcClient,
        mint: &Pubkey,
    ) -> Result<Self> {
        println!("\n🔍 Finding existing Buy transaction for this mint...");

        let pump_program = Pubkey::from_str(PUMP_PROGRAM_ID)?;

        // Izračunaj bonding curve
        let (bonding_curve, _) = Pubkey::find_program_address(
            &[b"bonding-curve", &mint.to_bytes()],
            &pump_program,
        );

        // Fetch nedavne transakcije za bonding curve
        let signatures = rpc
            .get_signatures_for_address_with_config(
                &bonding_curve,
                solana_client::rpc_client::GetConfirmedSignaturesForAddress2Config {
                    limit: Some(10),
                    ..Default::default()
                },
            )
            .await?;

        // Pronađi prvi Buy TX
        for sig_info in signatures {
            if let Ok(sig) = Signature::from_str(&sig_info.signature) {
                let tx = rpc.get_transaction_with_config(
                    &sig,
                    solana_client::rpc_config::RpcTransactionConfig {
                        encoding: Some(solana_transaction_status::UiTransactionEncoding::JsonParsed),
                        max_supported_transaction_version: Some(0),
                        commitment: Some(CommitmentConfig::confirmed()),
                    }
                ).await?;

                // Proveri da li je Buy instrukcija
                if let solana_transaction_status::EncodedTransaction::Json(ui_tx) = &tx.transaction.transaction {
                    if let solana_transaction_status::UiMessage::Parsed(parsed) = &ui_tx.message {

                        let accounts: Vec<Pubkey> = parsed.account_keys
                            .iter()
                            .filter_map(|key| Pubkey::from_str(&key.pubkey).ok())
                            .collect();

                        if accounts.len() >= 10 {
                            println!("\n✅ Found Buy TX, extracting accounts...");

                            // Account layout iz Buy TX-a
                            let global = accounts[0];
                            let fee_recipient = accounts[1];
                            let bonding_curve = accounts[3];
                            let associated_bonding_curve = accounts[4];
                            let creator_vault = accounts[9];
                            let event_authority = accounts[10];

                            // Ostali account-i
                            let fee_config = Pubkey::from_str("8Wf5TiAheLUqBrKXeYg2JtAFFMWtKdG2BSFgqUcPVwTt")?;
                            let fee_program = Pubkey::from_str("pfeeUxB6jkeY1Hxd7CsFCAjcbHA9rWtchMGdZ6VojVZ")?;
                            let global_volume = Pubkey::from_str("Hq2wp8uJ9jCPsYgNHex8RtqdvMPfVGoYwjvF1ATiwn2Y")?;
                            let user_volume = if accounts.len() > 13 {
                                accounts[13]
                            } else {
                                Pubkey::from_str("2wkkPpX4nML1Tzjrh2neECxmhhj4NwjXU7z5q56xjJH9")?
                            };

                            return Ok(PumpBuyAccounts {
                                mint: *mint,
                                bonding_curve,
                                associated_bonding_curve,
                                creator_vault,
                                global_volume,
                                user_volume,
                                global,
                                fee_recipient,
                                event_authority,
                                fee_config,
                                fee_program,
                            });
                        }
                    }
                }
            }
        }

        Err(anyhow!("No Buy transaction found for this mint"))
    }

    // Private helper methods

    async fn fetch_tx_with_retry(
        rpc: &RpcClient,
        sig: &Signature,
        max_attempts: usize,
    ) -> Result<solana_transaction_status::EncodedConfirmedTransactionWithStatusMeta> {
        for attempt in 1..=max_attempts {
            tokio::time::sleep(tokio::time::Duration::from_millis(100 * attempt as u64)).await;

            match rpc.get_transaction_with_config(
                sig,
                solana_client::rpc_config::RpcTransactionConfig {
                    encoding: Some(solana_transaction_status::UiTransactionEncoding::JsonParsed),
                    max_supported_transaction_version: Some(0),
                    commitment: Some(CommitmentConfig::confirmed()),
                }
            ).await {
                Ok(tx) => return Ok(tx),
                Err(e) if attempt < max_attempts => {
                    println!("⚠️  Retry {}/{}: {}", attempt, max_attempts, e);
                    continue;
                }
                Err(e) => return Err(anyhow!("TX fetch failed: {}", e)),
            }
        }
        Err(anyhow!("Max retries exceeded"))
    }

    fn extract_from_initialize_instruction(
        tx: &solana_transaction_status::EncodedConfirmedTransactionWithStatusMeta,
        mint: &Pubkey,
    ) -> Result<Self> {
        if let solana_transaction_status::EncodedTransaction::Json(ui_tx) = &tx.transaction.transaction {
            if let solana_transaction_status::UiMessage::Parsed(parsed) = &ui_tx.message {

                // Sakupi sve account-e iz transakcije
                let accounts: Vec<Pubkey> = parsed.account_keys
                    .iter()
                    .filter_map(|key| Pubkey::from_str(&key.pubkey).ok())
                    .collect();

                println!("\n📋 InitializeBondingCurve accounts: {}", accounts.len());
                for (i, acc) in accounts.iter().enumerate() {
                    println!("  #{}: {}", i, acc);
                }

                // PUMP.FUN InitializeBondingCurve ACCOUNT STRUKTURA:
                //
                // #0: global (4wTV1YmiEkRvAtNtsSGPtUrqRYQMe5SKy2uB4Jjaxnjf)
                // #1: mint (novi token)
                // #2: bonding_curve (PDA)
                // #3: associated_bonding_curve (token account)
                // #4: creator (wallet)
                // #5: creator_vault (SYSTEM ACCOUNT za SOL fee - NE ATA!)
                // #6: metadata
                // #7: event_authority
                // #8: token_program
                // #9: associated_token_program
                // #10: system_program
                // #11: rent
                // --- Mogu biti i dodatni za volume tracking ---

                if accounts.len() < 8 {
                    return Err(anyhow!("Not enough accounts in transaction"));
                }

                // Ekstraktuj tačne account-e
                let global = accounts[0];
                let bonding_curve = accounts[2];
                let associated_bonding_curve = accounts[3];
                let creator_vault = accounts[5];  // SYSTEM ACCOUNT, NE ATA!
                let event_authority = accounts[7];

                println!("\n✅ Extracted accounts:");
                println!("   Global: {}", global);
                println!("   Mint: {}", mint);
                println!("   Bonding Curve: {}", bonding_curve);
                println!("   Assoc BC: {}", associated_bonding_curve);
                println!("   Creator Vault: {} (SYSTEM ACCOUNT)", creator_vault);
                println!("   Event Authority: {}", event_authority);

                // Static accounts
                let fee_recipient = Pubkey::from_str("CebN5WGQ4jvEPvsVU4EoHEpgzq1VV7AbicfhtW4xC9iM")?;
                let fee_config = Pubkey::from_str("8Wf5TiAheLUqBrKXeYg2JtAFFMWtKdG2BSFgqUcPVwTt")?;
                let fee_program = Pubkey::from_str("pfeeUxB6jkeY1Hxd7CsFCAjcbHA9rWtchMGdZ6VojVZ")?;

                // Volume accounts
                let global_volume = Pubkey::from_str("Hq2wp8uJ9jCPsYgNHex8RtqdvMPfVGoYwjvF1ATiwn2Y")?;

                let user_volume = if accounts.len() > 14 {
                    accounts[14]
                } else {
                    // Fallback ako nije u Initialize TX
                    Pubkey::from_str("2wkkPpX4nML1Tzjrh2neECxmhhj4NwjXU7z5q56xjJH9")?
                };

                return Ok(PumpBuyAccounts {
                    mint: *mint,
                    bonding_curve,
                    associated_bonding_curve,
                    creator_vault,
                    global_volume,
                    user_volume,
                    global,
                    fee_recipient,
                    event_authority,
                    fee_config,
                    fee_program,
                });
            }
        }

        Err(anyhow!("Could not parse InitializeBondingCurve transaction"))
    }
}