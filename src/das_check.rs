// das_check.rs - IMPROVED: Better error handling

use anyhow::Result;
use serde::Deserialize;
use solana_sdk::pubkey::Pubkey;
use solana_client::nonblocking::rpc_client::RpcClient;

const HELIUS_API_KEY: &str = "7ef7af02-aa9d-4f5c-9c98-d5fa303d1f04";
const PUMP_PROGRAM_ID: &str = "6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P";

#[derive(Debug, Deserialize)]
struct DasResponse {
    result: DasResult,
}

#[derive(Debug, Deserialize)]
struct DasResult {
    total: u32,
}

/// ⚡ HYBRID: Try DAS API first (fast), fallback to transaction parsing (reliable)
pub async fn check_creator_token_count_das(creator: &Pubkey) -> Result<u32> {
    // 🚀 STEP 1: Try DAS API (instant if indexed)
    match try_das_api(creator).await {
        Ok(count) if count > 0 => {
            println!("      ✅ DAS API: {} tokens", count);
            return Ok(count);
        }
        Ok(_) => {
            println!("      ⚠️  DAS API returned 0 - trying transaction fallback...");
        }
        Err(e) => {
            println!("      ⚠️  DAS API failed: {} - trying transaction fallback...", e);
        }
    }

    // 🔥 STEP 2: Fallback to transaction parsing (slower but reliable)
    match try_transaction_parsing(creator).await {
        Ok(count) => {
            println!("      ✅ Transaction parsing: {} tokens", count);
            Ok(count as u32)
        }
        Err(e) => {
            println!("      ❌ Both methods failed: {}", e);
            println!("      ⚠️  CRITICAL: Returning 999 to trigger skip filter!");
            Ok(999) // ← Return high number to skip risky tokens
        }
    }
}

/// Try DAS API method
async fn try_das_api(creator: &Pubkey) -> Result<u32> {
    let url = format!("https://mainnet.helius-rpc.com/?api-key={}", HELIUS_API_KEY);

    let request_body = serde_json::json!({
        "jsonrpc": "2.0",
        "id": "1",
        "method": "getAssetsByCreator",
        "params": {
            "creatorAddress": creator.to_string(),
            "onlyVerified": false,
            "page": 1,
            "limit": 1
        }
    });

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_millis(400)) // ← Increased timeout
        .build()?;

    let response: DasResponse = client
        .post(&url)
        .json(&request_body)
        .send()
        .await?
        .json()
        .await?;

    Ok(response.result.total)
}

/// Fallback: Parse transactions to count pump.fun creates
async fn try_transaction_parsing(creator: &Pubkey) -> Result<usize> {
    use solana_client::rpc_config::RpcTransactionConfig;
    use solana_transaction_status::UiTransactionEncoding;
    use solana_sdk::commitment_config::CommitmentConfig;
    use std::str::FromStr;

    let rpc_url = format!("https://mainnet.helius-rpc.com/?api-key={}", HELIUS_API_KEY);
    let rpc = RpcClient::new(rpc_url);

    let pump_program = Pubkey::from_str(PUMP_PROGRAM_ID)?;

    // Get signatures with retry
    let sigs = match rpc.get_signatures_for_address(creator).await {
        Ok(s) => s,
        Err(e) => {
            println!("      ⚠️  RPC error getting signatures: {}", e);
            return Err(anyhow::anyhow!("Could not get signatures"));
        }
    };

    if sigs.is_empty() {
        println!("      ⚠️  No TX history - likely fake creator (wallet never created tokens)");
        return Err(anyhow::anyhow!("No transaction history"));
    }

    let mut token_count = 0;

    // Check last 50 transactions
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

        // Stop if over 20 (optimization)
        if token_count > 20 {
            break;
        }
    }

    Ok(token_count)
}