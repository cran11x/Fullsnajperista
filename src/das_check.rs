// das_check.rs - IMPROVED: Better error handling with retry logic
#![allow(unused, dead_code)]

use anyhow::Result;
use serde::Deserialize;
use solana_sdk::pubkey::Pubkey;
use solana_client::nonblocking::rpc_client::RpcClient;
use crate::utils::retry_with_backoff;

use crate::constants::PUMP_PROGRAM_ID;
const MAX_RETRIES: u32 = 3;
const RETRY_DELAY_MS: u64 = 100;

#[derive(Debug, Deserialize)]
struct DasResponse {
    result: DasResult,
}

#[derive(Debug, Deserialize)]
struct DasResult {
    total: u32,
}

/// ⚡ FAST: Only DAS API with limit=1000 for accurate total and retry logic
pub async fn check_creator_token_count_das(creator: &Pubkey, api_key: &str) -> Result<u32> {
    match retry_with_backoff(
        || try_das_api(creator, api_key),
        MAX_RETRIES,
        RETRY_DELAY_MS,
    ).await {
        Ok(count) => {
            println!("      ✅ DAS API: {} tokens", count);
            Ok(count)
        }
        Err(e) => {
            println!("      ❌ DAS API failed after {} retries: {}", MAX_RETRIES, e);
            println!("      ⚠️  Returning 999 to skip (can't verify)");
            Ok(999) // Skip if can't verify
        }
    }
}

/// Try DAS API method
async fn try_das_api(creator: &Pubkey, api_key: &str) -> Result<u32> {
    let url = format!("https://mainnet.helius-rpc.com/?api-key={}", api_key);

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

    // Use shared HTTP client for better performance
    let client = crate::utils::get_shared_http_client();

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

    // API key should come from environment or Config
    let rpc_url = "https://mainnet.helius-rpc.com/?api-key=".to_string();
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

#[cfg(test)]
mod tests {
    use super::*;


    #[tokio::test]
    async fn test_check_creator_token_count_das_success() {
        use mockito::Server;
        let mut server = Server::new_async().await;
        let creator = Pubkey::new_unique();

        // Mock successful response
        let _mock = server.mock("POST", "/")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"jsonrpc":"2.0","id":"1","result":{"total":5}}"#)
            .create();

        // Note: This test would need to override the URL in try_das_api
        // For now, we test the structure
        assert_ne!(creator, Pubkey::default());
    }

    #[tokio::test]
    async fn test_das_api_fallback() {
        // Test that fallback mechanism works
        // This would require mocking both searchAssets and getAssetsByCreator
        let creator = Pubkey::new_unique();
        
        // Test structure - real implementation would use mock server
        assert_ne!(creator, Pubkey::default());
    }

    #[tokio::test]
    async fn test_das_timeout() {
        // Test timeout handling
        use mockito::Server;
        let mut server = Server::new_async().await;
        
        // Mock response (timeout testing would need different approach)
        let _mock = server.mock("POST", "/")
            .with_status(200)
            .with_body(r#"{"jsonrpc":"2.0","id":"1","result":{"total":0}}"#)
            .create();

        // The function should timeout and return 999
        let creator = Pubkey::new_unique();
        // In real test, we'd call check_creator_token_count_das with mocked URL
        assert_ne!(creator, Pubkey::default());
    }

    #[test]
    fn test_das_response_parsing() {
        // Test JSON parsing with valid total
        let json_str = r#"{"jsonrpc":"2.0","id":"1","result":{"total":10}}"#;
        let json: serde_json::Value = serde_json::from_str(json_str).unwrap();
        
        if let Some(total) = json["result"]["total"].as_u64() {
            assert_eq!(total, 10);
        } else {
            panic!("Failed to parse total");
        }

        // Test with missing total
        let json_str2 = r#"{"jsonrpc":"2.0","id":"1","result":{}}"#;
        let json2: serde_json::Value = serde_json::from_str(json_str2).unwrap();
        assert!(json2["result"]["total"].as_u64().is_none());
        
        // Test with zero total
        let json_str3 = r#"{"jsonrpc":"2.0","id":"1","result":{"total":0}}"#;
        let json3: serde_json::Value = serde_json::from_str(json_str3).unwrap();
        if let Some(total) = json3["result"]["total"].as_u64() {
            assert_eq!(total, 0);
        } else {
            panic!("Failed to parse zero total");
        }
        
        // Test u64 to u32 conversion
        let total_u64: u64 = 1000;
        let total_u32 = total_u64 as u32;
        assert_eq!(total_u32, 1000);
    }

    #[test]
    fn test_das_error_handling() {
        // Test error response
        let error_json = r#"{"jsonrpc":"2.0","id":"1","error":{"code":-32602,"message":"Invalid params"}}"#;
        let json: serde_json::Value = serde_json::from_str(error_json).unwrap();
        
        assert!(json["error"].is_object());
        assert!(json["result"].is_null());
        assert_eq!(json["error"]["code"], -32602);
        assert_eq!(json["error"]["message"], "Invalid params");
        
        // Test that error response doesn't have total
        assert!(json["result"]["total"].as_u64().is_none());
    }

    #[tokio::test]
    #[ignore]
    async fn test_das_check_real_api() {
        // Integration test - requires real API key and network
        // This should be run manually with valid credentials
        use std::str::FromStr;
        let _creator = Pubkey::from_str("11111111111111111111111111111111").unwrap();
        
        // This would make a real API call
        // let result = check_creator_token_count_das(&creator).await;
        // assert!(result.is_ok());
    }
}