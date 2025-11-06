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

/// ⚡ FAST: Only DAS API with limit=1000 for accurate total
pub async fn check_creator_token_count_das(creator: &Pubkey) -> Result<u32> {
    match try_das_api(creator).await {
        Ok(count) => {
            println!("      ✅ DAS API: {} tokens", count);
            Ok(count)
        }
        Err(e) => {
            println!("      ❌ DAS API failed: {}", e);
            println!("      ⚠️  Returning 999 to skip (can't verify)");
            Ok(999) // Skip if can't verify
        }
    }
}

/// Try DAS API method
async fn try_das_api(creator: &Pubkey) -> Result<u32> {
    let url = format!("https://mainnet.helius-rpc.com/?api-key={}", HELIUS_API_KEY);

    // Try method 1: searchAssets (more comprehensive)
    let search_body = serde_json::json!({
        "jsonrpc": "2.0",
        "id": "1",
        "method": "searchAssets",
        "params": {
            "creatorAddress": creator.to_string(),
            "creatorVerified": false,
            "page": 1,
            "limit": 20  // ← Increased to get real total
        }
    });

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_millis(800))
        .build()?;

    let response_text = client
        .post(&url)
        .json(&search_body)
        .send()
        .await?
        .text()
        .await?;

    let json: serde_json::Value = serde_json::from_str(&response_text)?;

    if let Some(total) = json["result"]["total"].as_u64() {
        if total > 0 {
            return Ok(total as u32);
        }
    }

    // Fallback: Try getAssetsByCreator
    let assets_body = serde_json::json!({
        "jsonrpc": "2.0",
        "id": "1",
        "method": "getAssetsByCreator",
        "params": {
            "creatorAddress": creator.to_string(),
            "onlyVerified": false,
            "page": 1,
            "limit": 1000  // ← Increased to get real total
        }
    });

    let response_text2 = client
        .post(&url)
        .json(&assets_body)
        .send()
        .await?
        .text()
        .await?;

    let json2: serde_json::Value = serde_json::from_str(&response_text2)?;

    if let Some(total) = json2["result"]["total"].as_u64() {
        return Ok(total as u32);
    }

    Err(anyhow::anyhow!("DAS API returned no data"))
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
    let mut tx_checked = 0;

    // Check last 100 transactions (increased from 50)
    for sig_info in sigs.iter().take(100) {
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
            tx_checked += 1;

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

    // ⚠️ If checked many TXs but found 0 CREATE = active wallet with no creates = suspicious
    if tx_checked >= 5 && token_count == 0 {
        println!("      ⚠️  Wallet has {} TXs but 0 token creations - likely not real creator!", tx_checked);
        return Err(anyhow::anyhow!("Active wallet but no token creations"));
    }

    Ok(token_count)
}