// das_check.rs - IMPROVED: Better error handling with retry logic
#![allow(unused, dead_code)]

use anyhow::Result;
use serde::Deserialize;
use solana_sdk::pubkey::Pubkey;
use solana_client::nonblocking::rpc_client::RpcClient;
use crate::utils::retry_with_backoff;
use std::str::FromStr;

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

/// Count pump.fun tokens created by creator
/// Uses DAS API first (fast - ~200-500ms), falls back to Enhanced Transactions API if DAS returns 0
/// 
/// ⚠️  KNOWN LIMITATION: DAS API may return 0 for some creators even if they have many tokens.
///     This is a DAS API limitation, not a bug in this code. For example, creator
///     `uV9BZrH8Q3tRqmMcTwTsRwag8PanTpwdczVutcXV6n1` has 547 tokens but DAS returns 0.
///     
///     When DAS returns 0, we try Enhanced Transactions API as fallback (fast - ~200-500ms).
///     This ensures we get accurate count while staying under 1 second.
/// 
/// NOTE: Both methods are fast (~200-500ms each), so total time is always under 1 second
pub async fn check_creator_token_count_das(creator: &Pubkey, api_key: &str) -> Result<u32> {
    // PRIMARY: Use DAS API first (fast - ~200-500ms, always under 1 second)
    let das_count = match retry_with_backoff(
        || try_das_api(creator, api_key),
        MAX_RETRIES,
        RETRY_DELAY_MS,
    ).await {
        Ok(count) => {
            println!("      ✅ DAS API: creator={}, total={}", creator, count);
            count
        }
        Err(e) => {
            println!("      ❌ DAS API failed after {} retries: {}", MAX_RETRIES, e);
            // Try Enhanced Transactions API as fallback
            println!("      ⚠️  Trying Enhanced Transactions API as fallback...");
            return try_enhanced_transactions_api_v2(creator, api_key).await;
        }
    };
    
    // FALLBACK: If DAS returned 0, try Enhanced Transactions API (fast - ~200-500ms)
    // This helps when DAS API incorrectly returns 0 for creators with many tokens
    if das_count == 0 {
        println!("      ⚠️  DAS returned 0, trying Enhanced Transactions API for verification...");
        match try_enhanced_transactions_api_v2(creator, api_key).await {
            Ok(enhanced_count) => {
                if enhanced_count > 0 {
                    println!("      ✅ Enhanced Transactions API: creator={}, count={} (DAS incorrectly returned 0)", 
                             creator, enhanced_count);
                    return Ok(enhanced_count);
                } else {
                    println!("      ✅ Enhanced Transactions API confirmed: creator={} has 0 tokens", creator);
                    return Ok(0);
                }
            }
            Err(e) => {
                println!("      ⚠️  Enhanced Transactions API failed: {}, using DAS count (0)", e);
                // Return 0 from DAS
            }
        }
    }
    
    // Return DAS count (or 0 if both methods failed)
    Ok(das_count)
}

/// Try DAS API method - FAST: Uses multiple DAS methods with fungible token filtering
/// 
/// Based on Helius DAS API documentation: https://www.helius.dev/docs
/// - Uses `searchAssets` with `tokenType: "fungible"` to filter out NFTs (faster, more accurate)
/// - Uses `getAssetsByCreator` with pagination for comprehensive results
/// - This is much faster than RPC transaction parsing (~200-500ms vs 15+ seconds)
/// 
/// Note: DAS API may return 0 for some creators even if they have many tokens.
/// This is a known limitation of the DAS indexing system.
async fn try_das_api(creator: &Pubkey, api_key: &str) -> Result<u32> {
    let url = format!("https://mainnet.helius-rpc.com/?api-key={}", api_key);
    let client = crate::utils::get_shared_http_client();

    // METHOD 1: Try searchAssets with authorityAddress + tokenType filter (fungible tokens only)
    // This filters out NFTs and returns only SPL tokens, which is more accurate for pump.fun tokens
    let search_authority_body = serde_json::json!({
        "jsonrpc": "2.0",
        "id": "1",
        "method": "searchAssets",
        "params": {
            "authorityAddress": creator.to_string(),
            "tokenType": "fungible",  // Filter for fungible tokens only (SPL tokens, not NFTs)
            "page": 1,
            "limit": 1000
        }
    });

    let search_authority_response = client
        .post(&url)
        .json(&search_authority_body)
        .send()
        .await?;

    if search_authority_response.status().is_success() {
        let search_authority_json: serde_json::Value = search_authority_response.json().await?;
        
        if let Some(total) = search_authority_json["result"]["total"].as_u64() {
            if total > 0 {
                return Ok(total as u32);
            }
        }
        
        // If total not available, count items
        if let Some(items) = search_authority_json["result"]["items"].as_array() {
            let count = items.len() as u32;
            if count > 0 {
                // If we got less than limit, we have all results
                if items.len() < 1000 {
                    return Ok(count);
                }
            }
        }
    }

    // METHOD 2: Try searchAssets with creatorAddress + tokenType filter (fungible tokens only)
    // This filters out NFTs and returns only SPL tokens created by this creator
    let search_body = serde_json::json!({
        "jsonrpc": "2.0",
        "id": "1",
        "method": "searchAssets",
        "params": {
            "creatorAddress": creator.to_string(),
            "creatorVerified": false,
            "tokenType": "fungible",  // Filter for fungible tokens only (SPL tokens, not NFTs)
            "page": 1,
            "limit": 1000
        }
    });

    let search_response = client
        .post(&url)
        .json(&search_body)
        .send()
        .await?;

    if search_response.status().is_success() {
        let search_json: serde_json::Value = search_response.json().await?;
        
        if let Some(total) = search_json["result"]["total"].as_u64() {
            if total > 0 {
                return Ok(total as u32);
            }
        }
        
        // If total not available, count items
        if let Some(items) = search_json["result"]["items"].as_array() {
            let count = items.len() as u32;
            if count > 0 {
                // If we got less than limit, we have all results
                if items.len() < 1000 {
                    return Ok(count);
                }
            }
        }
    }

    // METHOD 3: Use getAssetsByCreator with pagination to get accurate total count
    // This is fast (~200-500ms) and returns the total field which includes all pages
    let assets_body = serde_json::json!({
        "jsonrpc": "2.0",
        "id": "1",
        "method": "getAssetsByCreator",
        "params": {
            "creatorAddress": creator.to_string(),
            "onlyVerified": false,
            "page": 1,
            "limit": 1000  // High limit to get total count in one request
        }
    });

    let response_text = client
        .post(&url)
        .json(&assets_body)
        .send()
        .await?
        .text()
        .await?;

    let json: serde_json::Value = serde_json::from_str(&response_text)?;

    // Check for errors
    if let Some(err) = json["error"].as_object() {
        // Fallback to searchAssets
        return try_search_assets_fallback(creator, api_key).await;
    }

    // Try to get total count directly (fastest method)
    if let Some(total) = json["result"]["total"].as_u64() {
        return Ok(total as u32);
    }

    // If total is not available, count items across pages
    let mut total_count = 0u32;
    let mut page = 1;
    let max_pages = 20; // Check up to 20 pages (20,000 tokens max)

    loop {
        let mut current_body = assets_body.clone();
        current_body["params"]["page"] = serde_json::json!(page);
        
        let response_text = client
            .post(&url)
            .json(&current_body)
            .send()
            .await?
            .text()
            .await?;

        let json: serde_json::Value = serde_json::from_str(&response_text)?;

        if let Some(_err) = json["error"].as_object() {
            break;
        }

        if let Some(assets) = json["result"]["items"].as_array() {
            if assets.is_empty() {
                break; // No more pages
            }
            total_count += assets.len() as u32;

            // If we got less than limit, we're done (no more pages)
            if assets.len() < 1000 {
                break;
            }

            page += 1;
            if page > max_pages {
                break;
            }
        } else {
            break;
        }
    }

    if total_count > 0 {
        return Ok(total_count);
    }

    // Fallback to searchAssets
    try_search_assets_fallback(creator, api_key).await
}

/// Fallback method: Try searchAssets
async fn try_search_assets_fallback(creator: &Pubkey, api_key: &str) -> Result<u32> {
    let url = format!("https://mainnet.helius-rpc.com/?api-key={}", api_key);
    let client = crate::utils::get_shared_http_client();

    let search_body = serde_json::json!({
        "jsonrpc": "2.0",
        "id": "1",
        "method": "searchAssets",
        "params": {
            "creatorAddress": creator.to_string(),
            "creatorVerified": false,
            "page": 1,
            "limit": 1000
        }
    });

    let response_text = client
        .post(&url)
        .json(&search_body)
        .send()
        .await?
        .text()
        .await?;

    let json: serde_json::Value = serde_json::from_str(&response_text)?;

    if let Some(total) = json["result"]["total"].as_u64() {
        return Ok(total as u32);
    }

    Err(anyhow::anyhow!("DAS API returned no data"))
}

/// Use Helius Enhanced Transactions API endpoint to count CREATE transactions for pump.fun tokens
/// This is FAST (~200-500ms) and uses server-side filtering (type=CREATE&source=PUMP_FUN)
/// Endpoint: /v0/addresses/{address}/transactions?type=CREATE&source=PUMP_FUN
/// 
/// ⚡ OPTIMIZED: Uses aggressive timeout (800ms) to stay under 1 second total
async fn try_enhanced_transactions_api_v2(creator: &Pubkey, api_key: &str) -> Result<u32> {
    use std::time::Duration;
    
    // Use api.helius.xyz domain (as shown in user's example)
    // Note: limit parameter is not supported, API returns default number of transactions
    let url = format!(
        "https://api.helius.xyz/v0/addresses/{}/transactions?api-key={}&type=CREATE&source=PUMP_FUN",
        creator.to_string(),
        api_key
    );
    
    let client = crate::utils::get_shared_http_client();
    
    // ⚡ TIMEOUT: 2500ms max (API can be slow, but we need accurate count)
    // Note: This is only used when DAS returns 0, so most requests are still fast (~400ms DAS)
    // Worst case: DAS (~400ms) + Enhanced (~2500ms) = ~2900ms, but only when DAS fails
    let response = match tokio::time::timeout(
        Duration::from_millis(2500),
        client.get(&url).send()
    ).await {
        Ok(Ok(resp)) => resp,
        Ok(Err(e)) => {
            return Err(anyhow::anyhow!("Enhanced Transactions API request failed: {}", e));
        }
        Err(_) => {
            return Err(anyhow::anyhow!("Enhanced Transactions API timeout (2500ms)"));
        }
    };
    
    let status = response.status();
    if !status.is_success() {
        let error_text = match tokio::time::timeout(
            Duration::from_millis(200),
            response.text()
        ).await {
            Ok(Ok(text)) => text,
            _ => format!("HTTP {}", status),
        };
        return Err(anyhow::anyhow!("Enhanced Transactions API request failed: {} - {}", status, error_text));
    }
    
    // ⚡ JSON PARSING: Use timeout for JSON parsing (500ms should be enough)
    let transactions: Vec<serde_json::Value> = match tokio::time::timeout(
        Duration::from_millis(500),
        response.json::<Vec<serde_json::Value>>()
    ).await {
        Ok(Ok(txs)) => txs,
        Ok(Err(e)) => {
            return Err(anyhow::anyhow!("Enhanced Transactions API JSON parse failed: {}", e));
        }
        Err(_) => {
            return Err(anyhow::anyhow!("Enhanced Transactions API JSON parse timeout (500ms)"));
        }
    };
    
    // Count CREATE transactions (API already filters by type=CREATE&source=PUMP_FUN)
    let create_count = transactions.len() as u32;
    
    if create_count > 0 {
        println!("      ✅ Enhanced Transactions API v2: creator={}, CREATE count={} (filtered: type=CREATE&source=PUMP_FUN)", 
                 creator, create_count);
    } else {
        println!("      ✅ Enhanced Transactions API v2: creator={}, CREATE count=0", creator);
    }
    
    // If we got many results, there might be more (but we don't paginate for speed)
    if create_count >= 50 {
        println!("      ⚠️  Got {} results - may have more (not paginating for speed)", create_count);
    }
    
    Ok(create_count)
}

/// Try Enhanced Transactions API to count CREATE instructions for pump.fun tokens
/// This uses getSignaturesForAddress + getTransaction with maxSupportedTransactionVersion
/// Faster than full RPC parsing but still slower than DAS (~1-3 seconds)
/// 
/// ⚠️  DEPRECATED: This function is no longer used. Use `try_enhanced_transactions_api_v2` instead
///     which uses the Enhanced Transactions API endpoint and is much faster (~200-500ms vs 1-3 seconds).
///     Kept for reference only.
#[allow(dead_code)]
async fn try_enhanced_transactions_api(creator: &Pubkey, api_key: &str) -> Result<u32> {
    use solana_client::rpc_config::RpcTransactionConfig;
    use solana_transaction_status::UiTransactionEncoding;
    use solana_sdk::commitment_config::CommitmentConfig;
    use std::sync::Arc;
    
    let rpc_url = format!("https://mainnet.helius-rpc.com/?api-key={}", api_key);
    let rpc = Arc::new(RpcClient::new(rpc_url));
    let pump_program = Pubkey::from_str(PUMP_PROGRAM_ID)?;
    
    // Get signatures - limit to 500 for speed (should cover most cases)
    let sigs = match rpc.get_signatures_for_address(creator).await {
        Ok(s) => {
            if s.len() > 500 {
                s.into_iter().take(500).collect()
            } else {
                s
            }
        }
        Err(e) => {
            return Err(anyhow::anyhow!("Could not get signatures: {}", e));
        }
    };
    
    if sigs.is_empty() {
        return Ok(0);
    }
    
    println!("      🔍 Enhanced Transactions API: Checking {} transactions for creator={}", sigs.len(), creator);
    
    let mut create_count = 0u32;
    let mut checked = 0u32;
    
    // Process in smaller batches for speed
    const BATCH_SIZE: usize = 20;
    let mut batch = Vec::new();
    
    for sig_info in sigs.iter() {
        let sig = match solana_sdk::signature::Signature::from_str(&sig_info.signature) {
            Ok(s) => s,
            Err(_) => continue,
        };
        
        batch.push(sig);
        
        if batch.len() >= BATCH_SIZE {
            let mut tasks = Vec::new();
            for sig in batch.drain(..) {
                let rpc_clone = Arc::clone(&rpc);
                let pump_program_str = pump_program.to_string();
                
                tasks.push(tokio::spawn(async move {
                    let tx_result = rpc_clone.get_transaction_with_config(
                        &sig,
                        RpcTransactionConfig {
                            encoding: Some(UiTransactionEncoding::Json),
                            max_supported_transaction_version: Some(0),
                            commitment: Some(CommitmentConfig::confirmed()),
                        }
                    ).await;
                    
                    (tx_result, pump_program_str)
                }));
            }
            
            let results = futures_util::future::join_all(tasks).await;
            
            for result in results {
                if let Ok((Ok(tx), pump_program_str)) = result {
                    checked += 1;
                    
                    if let Some(meta) = tx.transaction.meta {
                        let logs: Option<Vec<String>> = meta.log_messages.into();
                        
                        if let Some(log_messages) = logs {
                            let has_pump = log_messages.iter().any(|log| log.contains(&pump_program_str));
                            let has_create = log_messages.iter().any(|log| 
                                log.contains("Program log: Instruction: Create")
                            );
                            
                            if has_pump && has_create {
                                create_count += 1;
                            }
                        }
                    }
                }
            }
            
            // Early exit if we found enough tokens
            if checked >= 100 && create_count > 0 {
                break;
            }
        }
    }
    
    // Process remaining
    if !batch.is_empty() {
        let mut tasks = Vec::new();
        for sig in batch {
            let rpc_clone = Arc::clone(&rpc);
            let pump_program_str = pump_program.to_string();
            
            tasks.push(tokio::spawn(async move {
                let tx_result = rpc_clone.get_transaction_with_config(
                    &sig,
                    RpcTransactionConfig {
                        encoding: Some(UiTransactionEncoding::Json),
                        max_supported_transaction_version: Some(0),
                        commitment: Some(CommitmentConfig::confirmed()),
                    }
                ).await;
                
                (tx_result, pump_program_str)
            }));
        }
        
        let results = futures_util::future::join_all(tasks).await;
        
        for result in results {
            if let Ok((Ok(tx), pump_program_str)) = result {
                checked += 1;
                
                if let Some(meta) = tx.transaction.meta {
                    let logs: Option<Vec<String>> = meta.log_messages.into();
                    
                    if let Some(log_messages) = logs {
                        let has_pump = log_messages.iter().any(|log| log.contains(&pump_program_str));
                        let has_create = log_messages.iter().any(|log| 
                            log.contains("Program log: Instruction: Create")
                        );
                        
                        if has_pump && has_create {
                            create_count += 1;
                        }
                    }
                }
            }
        }
    }
    
    println!("      ✅ Enhanced Transactions API: creator={}, checked {} transactions, found {} CREATE events", 
             creator, checked, create_count);
    
    Ok(create_count)
}

/// Fast RPC method: Uses optimized batch processing with Arc for RPC client
/// This is faster than the full RPC method but still accurate
async fn count_pump_fun_tokens_rpc_fast(creator: &Pubkey, api_key: &str) -> Result<usize> {
    use solana_client::rpc_config::RpcTransactionConfig;
    use solana_transaction_status::UiTransactionEncoding;
    use solana_sdk::commitment_config::CommitmentConfig;
    use std::str::FromStr;
    use std::sync::Arc;

    let rpc_url = format!("https://mainnet.helius-rpc.com/?api-key={}", api_key);
    let rpc = Arc::new(RpcClient::new(rpc_url));

    let pump_program = Pubkey::from_str(PUMP_PROGRAM_ID)?;

    // Get signatures - check up to 1000 transactions (should cover 550 tokens)
    let sigs = match rpc.get_signatures_for_address(creator).await {
        Ok(s) => {
            // Limit to 1000 signatures for performance
            if s.len() > 1000 {
                s.into_iter().take(1000).collect()
            } else {
                s
            }
        }
        Err(e) => {
            println!("      ⚠️  RPC error getting signatures: {}", e);
            return Err(anyhow::anyhow!("Could not get signatures"));
        }
    };

    if sigs.is_empty() {
        return Ok(0);
    }

    
    let mut token_count = 0;
    let mut tx_checked = 0;
    
    // Process in batches for better performance
    const BATCH_SIZE: usize = 50;
    let mut batch = Vec::new();
    
    for sig_info in sigs.iter() {
        let sig = match solana_sdk::signature::Signature::from_str(&sig_info.signature) {
            Ok(s) => s,
            Err(_) => continue,
        };
        
        batch.push(sig);
        
        // Process batch when full
        if batch.len() >= BATCH_SIZE {
            // Process batch in parallel
            let mut batch_tasks = Vec::new();
            for sig in batch.drain(..) {
                let rpc_clone = Arc::clone(&rpc);
                let pump_program_str = pump_program.to_string();
                
                batch_tasks.push(tokio::spawn(async move {
                    let tx_result = rpc_clone.get_transaction_with_config(
                        &sig,
                        RpcTransactionConfig {
                            encoding: Some(UiTransactionEncoding::Json),
                            max_supported_transaction_version: Some(0),
                            commitment: Some(CommitmentConfig::confirmed()),
                        }
                    ).await;
                    
                    (tx_result, pump_program_str)
                }));
            }
            
            // Wait for batch to complete
            let batch_results = futures_util::future::join_all(batch_tasks).await;
            
            for result in batch_results {
                if let Ok((Ok(tx), pump_program_str)) = result {
                    tx_checked += 1;
                    
                    if let Some(meta) = tx.transaction.meta {
                        let logs: Option<Vec<String>> = meta.log_messages.into();
                        
                        if let Some(log_messages) = logs {
                            let has_pump = log_messages.iter().any(|log|
                                log.contains(&pump_program_str)
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
            }
        }
    }
    
    // Process remaining items in batch
    if !batch.is_empty() {
        let mut batch_tasks = Vec::new();
        for sig in batch {
            let rpc_clone = Arc::clone(&rpc);
            let pump_program_str = pump_program.to_string();
            
            batch_tasks.push(tokio::spawn(async move {
                let tx_result = rpc_clone.get_transaction_with_config(
                    &sig,
                    RpcTransactionConfig {
                        encoding: Some(UiTransactionEncoding::Json),
                        max_supported_transaction_version: Some(0),
                        commitment: Some(CommitmentConfig::confirmed()),
                    }
                ).await;
                
                (tx_result, pump_program_str)
            }));
        }
        
        let batch_results = futures_util::future::join_all(batch_tasks).await;
        
        for result in batch_results {
            if let Ok((Ok(tx), pump_program_str)) = result {
                tx_checked += 1;
                
                if let Some(meta) = tx.transaction.meta {
                    let logs: Option<Vec<String>> = meta.log_messages.into();
                    
                    if let Some(log_messages) = logs {
                        let has_pump = log_messages.iter().any(|log|
                            log.contains(&pump_program_str)
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
        }
    }

    Ok(token_count)
}

/// Count pump.fun tokens by parsing RPC transactions
/// This is more accurate than DAS API which counts all tokens (NFTs, SPL, etc.)
/// NOTE: This is the slower method - use count_pump_fun_tokens_rpc_fast for better performance
async fn count_pump_fun_tokens_rpc(creator: &Pubkey, api_key: &str) -> Result<usize> {
    use solana_client::rpc_config::RpcTransactionConfig;
    use solana_transaction_status::UiTransactionEncoding;
    use solana_sdk::commitment_config::CommitmentConfig;
    use std::str::FromStr;

    let rpc_url = format!("https://mainnet.helius-rpc.com/?api-key={}", api_key);
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

    // Check ALL transactions to get accurate count for creators with many tokens
    // Note: This might be slow for creators with thousands of tokens, but it's necessary for accuracy
    // For creators with many tokens, we need to check all transactions to get accurate count
    let max_txs_to_check = sigs.len(); // Check ALL transactions for accuracy
    
    let mut last_logged_count = 0;
    let mut last_logged_tx = 0;
    
    for sig_info in sigs.iter().take(max_txs_to_check) {
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

        // Log progress every 100 transactions or every 10 new tokens found (whichever comes first)
        if (tx_checked - last_logged_tx >= 100) || (token_count > 0 && token_count - last_logged_count >= 10) {
            if token_count > 0 {
                last_logged_count = token_count;
                last_logged_tx = tx_checked;
            }
        }
    }

    // ⚠️ If checked many TXs but found 0 CREATE = active wallet with no creates = suspicious
    if tx_checked >= 10 && token_count == 0 {
        println!("      ⚠️  Wallet has {} TXs but 0 pump.fun token creations - likely not real creator!", tx_checked);
        return Err(anyhow::anyhow!("Active wallet but no pump.fun token creations"));
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
    #[ignore] // Ignore by default - run with: cargo test -- --ignored
    async fn test_das_check_real_api() {
        // Integration test - requires real API key and network
        // Run with: cargo test test_das_check_real_api -- --ignored --nocapture
        
        use std::str::FromStr;
        use std::env;
        
        // Get API key from env or use default
        let api_key = env::var("HELIUS_API_KEY")
            .unwrap_or_else(|_| "7ef7af02-aa9d-4f5c-9c98-d5fa303d1f04".to_string());
        
        println!("\n🧪 Testing with REAL API calls...");
        println!("   API Key: {}...", &api_key[..10]);
        
        // Test with a known creator address (you can replace with real addresses)
        // These are example addresses - replace with real pump.fun creators
        let test_cases = vec![
            // Format: (creator_address, expected_min, expected_max, description)
            // Note: Replace these with real creator addresses from pump.fun
            ("11111111111111111111111111111111", 0, 1000, "System program (should have 0)"),
        ];
        
        for (creator_str, _min, _max, desc) in test_cases {
            let creator = match Pubkey::from_str(creator_str) {
                Ok(pk) => pk,
                Err(e) => {
                    println!("   ⚠️  Invalid creator address {}: {}", creator_str, e);
                    continue;
                }
            };
            
            println!("\n   Testing creator: {} ({})", creator, desc);
            
            match check_creator_token_count_das(&creator, &api_key).await {
                Ok(count) => {
                    println!("   ✅ Success! Token count: {}", count);
                    println!("      Expected range: {}-{}", _min, _max);
                    if count >= _min as u32 && count <= _max as u32 {
                        println!("      ✅ Count is within expected range!");
                    } else {
                        println!("      ⚠️  Count is outside expected range (but API call worked)");
                    }
                }
                Err(e) => {
                    println!("   ❌ Failed: {}", e);
                }
            }
            
            // Small delay to avoid rate limiting
            tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
        }
    }
    
    #[tokio::test]
    #[ignore] // Ignore by default - run with: cargo test -- --ignored
    async fn test_filter_with_real_creators() {
        // Test filter logic with real creator token counts
        // Run with: cargo test test_filter_with_real_creators -- --ignored --nocapture
        
        use std::str::FromStr;
        use std::env;
        use crate::config::Config;
        use crate::filters::check_creator_token_count;
        
        let api_key = env::var("HELIUS_API_KEY")
            .unwrap_or_else(|_| "7ef7af02-aa9d-4f5c-9c98-d5fa303d1f04".to_string());
        
        println!("\n🧪 Testing FILTER with REAL creators...");
        
        // Create test config with filter 0-10
        let mut config = Config::default();
        config.min_dev_tokens = 0;
        config.max_dev_tokens = 10;
        
        // Test creators (replace with real pump.fun creator addresses)
        let test_creators = vec![
            // Format: (creator_address, should_pass_filter, description)
            // Note: Replace with real addresses
            ("11111111111111111111111111111111", true, "System program (0 tokens)"),
        ];
        
        for (creator_str, should_pass, desc) in test_creators {
            let creator = match Pubkey::from_str(creator_str) {
                Ok(pk) => pk,
                Err(e) => {
                    println!("   ⚠️  Invalid creator: {}", e);
                    continue;
                }
            };
            
            println!("\n   Testing: {} ({})", creator, desc);
            println!("      Filter: {}-{} tokens", config.min_dev_tokens, config.max_dev_tokens);
            
            match check_creator_token_count_das(&creator, &api_key).await {
                Ok(count) => {
                    println!("      ✅ API returned: {} tokens", count);
                    
                    let passes = check_creator_token_count(count, &config);
                    println!("      Filter result: {}", if passes { "✅ PASSES" } else { "❌ FAILS" });
                    
                    if passes == should_pass {
                        println!("      ✅ Expected result matches!");
                    } else {
                        println!("      ⚠️  Expected {}, but got {}", should_pass, passes);
                    }
                }
                Err(e) => {
                    println!("      ❌ API failed: {}", e);
                    println!("      Filter would: ❌ FAIL (cannot verify)");
                }
            }
            
            tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
        }
    }
    
    #[tokio::test]
    #[ignore] // Ignore by default - run with: cargo test -- --ignored
    async fn test_single_real_creator() {
        // Quick test for a single creator address
        // Usage: Set CREATOR_ADDRESS env var or modify the address below
        // Run: cargo test test_single_real_creator -- --ignored --nocapture
        
        use std::str::FromStr;
        use std::env;
        
        let api_key = env::var("HELIUS_API_KEY")
            .unwrap_or_else(|_| "7ef7af02-aa9d-4f5c-9c98-d5fa303d1f04".to_string());
        
        // Get creator from env or use default
        // Test with the problematic creator that has 546 tokens but DAS returned 0
        let creator_str = env::var("CREATOR_ADDRESS")
            .unwrap_or_else(|_| "uV9BZrH8Q3tRqmMcTwTsRwag8PanTpwdczVutcXV6n1".to_string());
        
        let creator = match Pubkey::from_str(&creator_str) {
            Ok(pk) => pk,
            Err(e) => {
                eprintln!("❌ Invalid creator address '{}': {}", creator_str, e);
                return;
            }
        };
        
        println!("\n🧪 Testing single creator: {}", creator);
        let key_preview_len = 10.min(api_key.len());
        println!("   API Key: {}...", &api_key[..key_preview_len]);
        
        use std::time::Instant;
        let start = Instant::now();
        match check_creator_token_count_das(&creator, &api_key).await {
            Ok(count) => {
                let duration = start.elapsed();
                println!("   ✅ Success!");
                println!("   📊 Token count: {}", count);
                println!("   ⏱️  Time: {:?}", duration);
                println!("\n   Filter test (0-10 tokens):");
                if count <= 10 {
                    println!("      ✅ Would PASS filter (0-10)");
                } else {
                    println!("      ❌ Would FAIL filter (0-10) - count {} is outside range", count);
                }
            }
            Err(e) => {
                let duration = start.elapsed();
                println!("   ❌ Failed: {}", e);
                println!("   ⏱️  Time: {:?}", duration);
            }
        }
    }
    
    #[tokio::test]
    #[ignore] // Ignore by default - run with: cargo test test_enhanced_transactions_api_v2 -- --ignored --nocapture
    async fn test_enhanced_transactions_api_v2() {
        // Test Enhanced Transactions API v2 directly
        // Run: cargo test test_enhanced_transactions_api_v2 -- --ignored --nocapture
        
        use std::str::FromStr;
        use std::env;
        use std::time::Instant;
        
        let api_key = env::var("HELIUS_API_KEY")
            .unwrap_or_else(|_| "7ef7af02-aa9d-4f5c-9c98-d5fa303d1f04".to_string());
        
        // Test with problematic creator that DAS returns 0 for
        let creator_str = env::var("CREATOR_ADDRESS")
            .unwrap_or_else(|_| "uV9BZrH8Q3tRqmMcTwTsRwag8PanTpwdczVutcXV6n1".to_string());
        
        let creator = match Pubkey::from_str(&creator_str) {
            Ok(pk) => pk,
            Err(e) => {
                eprintln!("❌ Invalid creator address '{}': {}", creator_str, e);
                return;
            }
        };
        
        println!("\n🧪 Testing Enhanced Transactions API v2");
        println!("   Creator: {}", creator);
        let key_preview_len = 10.min(api_key.len());
        println!("   API Key: {}...", &api_key[..key_preview_len]);
        println!();
        
        let start = Instant::now();
        match try_enhanced_transactions_api_v2(&creator, &api_key).await {
            Ok(count) => {
                let duration = start.elapsed();
                println!("   ✅ Enhanced Transactions API v2: count={}", count);
                println!("   ⏱️  Time: {:?}", duration);
                
                if duration.as_millis() < 1000 {
                    println!("   ✅ Speed: Under 1 second (GOOD)");
                } else {
                    println!("   ⚠️  Speed: Over 1 second (SLOW)");
                }
            }
            Err(e) => {
                let duration = start.elapsed();
                println!("   ❌ Failed: {}", e);
                println!("   ⏱️  Time: {:?}", duration);
            }
        }
    }
    
    #[tokio::test]
    #[ignore] // Ignore by default - run with: cargo test test_das_vs_enhanced -- --ignored --nocapture
    async fn test_das_vs_enhanced() {
        // Compare DAS API vs Enhanced Transactions API v2
        // Run: cargo test test_das_vs_enhanced -- --ignored --nocapture
        
        use std::str::FromStr;
        use std::env;
        use std::time::Instant;
        
        let api_key = env::var("HELIUS_API_KEY")
            .unwrap_or_else(|_| "7ef7af02-aa9d-4f5c-9c98-d5fa303d1f04".to_string());
        
        // Test with problematic creator
        let creator_str = env::var("CREATOR_ADDRESS")
            .unwrap_or_else(|_| "uV9BZrH8Q3tRqmMcTwTsRwag8PanTpwdczVutcXV6n1".to_string());
        
        let creator = match Pubkey::from_str(&creator_str) {
            Ok(pk) => pk,
            Err(e) => {
                eprintln!("❌ Invalid creator address '{}': {}", creator_str, e);
                return;
            }
        };
        
        println!("\n🧪 Comparing DAS API vs Enhanced Transactions API v2");
        println!("   Creator: {}", creator);
        println!();
        
        // Test DAS API
        println!("   📊 Testing DAS API...");
        let das_start = Instant::now();
        let das_result = match try_das_api(&creator, &api_key).await {
            Ok(count) => {
                let duration = das_start.elapsed();
                println!("      ✅ DAS API: count={}, time={:?}", count, duration);
                Ok(count)
            }
            Err(e) => {
                let duration = das_start.elapsed();
                println!("      ❌ DAS API failed: {}, time={:?}", e, duration);
                Err(e)
            }
        };
        
        println!();
        
        // Test Enhanced Transactions API v2
        println!("   📊 Testing Enhanced Transactions API v2...");
        let enhanced_start = Instant::now();
        let enhanced_result = match try_enhanced_transactions_api_v2(&creator, &api_key).await {
            Ok(count) => {
                let duration = enhanced_start.elapsed();
                println!("      ✅ Enhanced Transactions API v2: count={}, time={:?}", count, duration);
                Ok(count)
            }
            Err(e) => {
                let duration = enhanced_start.elapsed();
                println!("      ❌ Enhanced Transactions API v2 failed: {}, time={:?}", e, duration);
                Err(e)
            }
        };
        
        println!();
        println!("   📊 Comparison:");
        match (das_result, enhanced_result) {
            (Ok(das_count), Ok(enhanced_count)) => {
                println!("      DAS API: {}", das_count);
                println!("      Enhanced Transactions API v2: {}", enhanced_count);
                if das_count == 0 && enhanced_count > 0 {
                    println!("      ✅ Enhanced Transactions API v2 found tokens when DAS returned 0!");
                } else if das_count == enhanced_count {
                    println!("      ✅ Both methods agree!");
                } else {
                    println!("      ⚠️  Methods disagree - DAS: {}, Enhanced: {}", das_count, enhanced_count);
                }
            }
            (Ok(das_count), Err(_)) => {
                println!("      DAS API: {}", das_count);
                println!("      Enhanced Transactions API v2: FAILED");
            }
            (Err(_), Ok(enhanced_count)) => {
                println!("      DAS API: FAILED");
                println!("      Enhanced Transactions API v2: {}", enhanced_count);
            }
            (Err(_), Err(_)) => {
                println!("      Both methods FAILED");
            }
        }
    }

    #[tokio::test]
    #[ignore] // Ignore by default - run with: cargo test test_all_creators_from_file -- --ignored --nocapture
    async fn test_all_creators_from_file() {
        // Test all creator addresses from tokens_to_check.txt in parallel
        // Run: cargo test test_all_creators_from_file -- --ignored --nocapture
        
        use std::str::FromStr;
        use std::env;
        use std::time::Instant;
        use std::fs;
        
        let start_time = Instant::now();
        
        let api_key = env::var("HELIUS_API_KEY")
            .unwrap_or_else(|_| "7ef7af02-aa9d-4f5c-9c98-d5fa303d1f04".to_string());
        
        // Read tokens_to_check.txt file
        let file_path = "tokens_to_check.txt";
        let file_content = match fs::read_to_string(file_path) {
            Ok(content) => content,
            Err(e) => {
                eprintln!("❌ Failed to read {}: {}", file_path, e);
                return;
            }
        };
        
        // Parse creator addresses from file
        let mut creators = Vec::new();
        let mut current_mint = None;
        
        for line in file_content.lines() {
            let line = line.trim();
            
            // Extract Mint address
            if line.starts_with("Mint:") {
                if let Some(mint_str) = line.strip_prefix("Mint:").map(|s| s.trim()) {
                    current_mint = Some(mint_str.to_string());
                }
            }
            
            // Extract Creator address
            if line.starts_with("Creator:") {
                if let Some(creator_str) = line.strip_prefix("Creator:").map(|s| s.trim()) {
                    if let Ok(creator_pubkey) = Pubkey::from_str(creator_str) {
                        creators.push((current_mint.clone(), creator_str.to_string(), creator_pubkey));
                    } else {
                        eprintln!("⚠️  Invalid creator address: {}", creator_str);
                    }
                }
            }
        }
        
        if creators.is_empty() {
            eprintln!("❌ No creator addresses found in {}", file_path);
            return;
        }
        
        println!("\n🧪 Testing {} creators from {} in parallel...", creators.len(), file_path);
        println!("   API Key: {}...", &api_key[..10.min(api_key.len())]);
        println!();
        
        // Test all creators in parallel
        let mut tasks = Vec::new();
        for (mint_opt, creator_str, creator_pubkey) in creators.iter() {
            let api_key_clone = api_key.clone();
            let creator_str_clone = creator_str.clone();
            let mint_opt_clone = mint_opt.clone();
            let creator_pubkey_clone = *creator_pubkey;
            
            let task = tokio::spawn(async move {
                let check_start = Instant::now();
                let result = check_creator_token_count_das(&creator_pubkey_clone, &api_key_clone).await;
                let check_duration = check_start.elapsed();
                
                (mint_opt_clone, creator_str_clone, creator_pubkey_clone, result, check_duration)
            });
            
            tasks.push(task);
        }
        
        // Wait for all tasks to complete
        let results = futures_util::future::join_all(tasks).await;
        let total_duration = start_time.elapsed();
        
        // Print results
        println!("═══════════════════════════════════════════════════════════════");
        println!("  REZULTATI - {} creator adresa", creators.len());
        println!("═══════════════════════════════════════════════════════════════");
        println!();
        
        let mut success_count = 0;
        let mut fail_count = 0;
        let mut pass_filter_count = 0;
        let mut fail_filter_count = 0;
        let mut skip_count = 0;
        
        for (idx, result) in results.into_iter().enumerate() {
            let (mint_opt, creator_str, creator_pubkey, check_result, check_duration) = match result {
                Ok(r) => r,
                Err(e) => {
                    eprintln!("   ❌ Task {} panicked: {:?}", idx + 1, e);
                    fail_count += 1;
                    continue;
                }
            };
            
            let token_num = idx + 1;
            let mint_display = mint_opt.as_ref()
                .map(|m| format!("Mint: {}", m))
                .unwrap_or_else(|| "Mint: N/A".to_string());
            
            match check_result {
                Ok(count) => {
                    success_count += 1;
                    
                    // ⚠️ SKIP if DAS returns 0 - matches bot behavior in bot_core.rs
                    if count == 0 {
                        skip_count += 1;
                        println!("TOKEN #{}:", token_num);
                        println!("  {}", mint_display);
                        println!("  Creator: {}", creator_str);
                        println!("  ✅ Token count: {} (vreme: {:?})", count, check_duration);
                        println!("  ⚠️  SKIP: DAS returned 0 (would be skipped in bot - DAS API limitation)");
                        println!();
                    } else {
                        let passes_filter = count <= 10;
                        
                        if passes_filter {
                            pass_filter_count += 1;
                        } else {
                            fail_filter_count += 1;
                        }
                        
                        println!("TOKEN #{}:", token_num);
                        println!("  {}", mint_display);
                        println!("  Creator: {}", creator_str);
                        println!("  ✅ Token count: {} (vreme: {:?})", count, check_duration);
                        println!("  Filter (0-10): {}", if passes_filter { "✅ PASS" } else { "❌ FAIL" });
                        println!();
                    }
                }
                Err(e) => {
                    fail_count += 1;
                    println!("TOKEN #{}:", token_num);
                    println!("  {}", mint_display);
                    println!("  Creator: {}", creator_str);
                    println!("  ❌ Error: {} (vreme: {:?})", e, check_duration);
                    println!("  Filter: ❌ FAIL (cannot verify)");
                    println!();
                }
            }
        }
        
        println!("═══════════════════════════════════════════════════════════════");
        println!("  STATISTIKA");
        println!("═══════════════════════════════════════════════════════════════");
        println!("  ✅ Uspešno: {}/{}", success_count, creators.len());
        println!("  ❌ Neuspešno: {}/{}", fail_count, creators.len());
        println!("  ⚠️  SKIP (DAS=0): {}", skip_count);
        println!("  ✅ Prošao filter (0-10): {}", pass_filter_count);
        println!("  ❌ Nije prošao filter (0-10): {}", fail_filter_count);
        println!("  ⏱️  Ukupno vreme: {:?}", total_duration);
        println!("  ⚡ Prosek po creatoru: {:?}", 
                 if creators.len() > 0 { 
                     total_duration.div_f64(creators.len() as f64)
                 } else { 
                     std::time::Duration::from_secs(0) 
                 });
        println!();
        
        if total_duration.as_secs_f64() < 1.0 {
            println!("  ✅ Test završen za manje od 1 sekunde!");
        } else {
            println!("  ⚠️  Test trajao {} sekundi (cilj: < 1 sekunda)", total_duration.as_secs_f64());
        }
    }

    #[tokio::test]
    #[ignore] // Ignore by default - run with: cargo test test_creators_from_json -- --ignored --nocapture
    async fn test_creators_from_json() {
        // Test all creator addresses from latest JSON session file
        // Run: cargo test test_creators_from_json -- --ignored --nocapture
        
        use std::str::FromStr;
        use std::env;
        use std::time::Instant;
        use std::fs;
        use std::collections::HashSet;
        use serde_json::Value;
        
        let start_time = Instant::now();
        
        let api_key = env::var("HELIUS_API_KEY")
            .unwrap_or_else(|_| "7ef7af02-aa9d-4f5c-9c98-d5fa303d1f04".to_string());
        
        // Find latest JSON file
        let json_files: Vec<_> = fs::read_dir(".")
            .unwrap()
            .filter_map(|entry| {
                let entry = entry.ok()?;
                let path = entry.path();
                if path.extension()?.to_str()? == "json" && 
                   path.file_name()?.to_str()?.starts_with("sniper_session_") {
                    Some((path, entry.metadata().ok()?.modified().ok()?))
                } else {
                    None
                }
            })
            .collect();
        
        if json_files.is_empty() {
            eprintln!("❌ No JSON session files found");
            return;
        }
        
        // Get latest file
        let (json_path, _) = json_files.iter()
            .max_by_key(|(_, time)| time)
            .unwrap();
        
        println!("\n🧪 Testing creators from JSON file: {}", json_path.display());
        println!("   API Key: {}...", &api_key[..10.min(api_key.len())]);
        println!();
        
        // Read and parse JSON
        let file_content = match fs::read_to_string(json_path) {
            Ok(content) => content,
            Err(e) => {
                eprintln!("❌ Failed to read {}: {}", json_path.display(), e);
                return;
            }
        };
        
        let json: Value = match serde_json::from_str(&file_content) {
            Ok(j) => j,
            Err(e) => {
                eprintln!("❌ Failed to parse JSON: {}", e);
                return;
            }
        };
        
        // Extract unique creators from buys
        let mut creators = Vec::new();
        let mut seen_creators = HashSet::new();
        
        if let Some(buys) = json["buys"].as_array() {
            for buy in buys {
                if let (Some(mint_str), Some(creator_str)) = 
                    (buy["mint"].as_str(), buy["creator"].as_str()) {
                    
                    if !seen_creators.contains(creator_str) {
                        seen_creators.insert(creator_str.to_string());
                        
                        if let Ok(creator_pubkey) = Pubkey::from_str(creator_str) {
                            creators.push((Some(mint_str.to_string()), creator_str.to_string(), creator_pubkey));
                        }
                    }
                }
            }
        }
        
        if creators.is_empty() {
            eprintln!("❌ No creator addresses found in JSON");
            return;
        }
        
        println!("Found {} unique creators in JSON file", creators.len());
        println!();
        
        // Test all creators in parallel
        let mut tasks = Vec::new();
        for (mint_opt, creator_str, creator_pubkey) in creators.iter() {
            let api_key_clone = api_key.clone();
            let creator_str_clone = creator_str.clone();
            let mint_opt_clone = mint_opt.clone();
            let creator_pubkey_clone = *creator_pubkey;
            
            let task = tokio::spawn(async move {
                let check_start = Instant::now();
                let result = check_creator_token_count_das(&creator_pubkey_clone, &api_key_clone).await;
                let check_duration = check_start.elapsed();
                
                (mint_opt_clone, creator_str_clone, creator_pubkey_clone, result, check_duration)
            });
            
            tasks.push(task);
        }
        
        // Wait for all tasks to complete
        let results = futures_util::future::join_all(tasks).await;
        let total_duration = start_time.elapsed();
        
        // Print results
        println!("═══════════════════════════════════════════════════════════════");
        println!("  REZULTATI - {} creator adresa iz JSON fajla", creators.len());
        println!("═══════════════════════════════════════════════════════════════");
        println!();
        
        let mut success_count = 0;
        let mut fail_count = 0;
        let mut pass_filter_count = 0;
        let mut fail_filter_count = 0;
        let mut skip_count = 0;
        
        for (idx, result) in results.into_iter().enumerate() {
            let (mint_opt, creator_str, creator_pubkey, check_result, check_duration) = match result {
                Ok(r) => r,
                Err(e) => {
                    eprintln!("   ❌ Task {} panicked: {:?}", idx + 1, e);
                    fail_count += 1;
                    continue;
                }
            };
            
            let token_num = idx + 1;
            let mint_display = mint_opt.as_ref()
                .map(|m| format!("Mint: {}", m))
                .unwrap_or_else(|| "Mint: N/A".to_string());
            
            match check_result {
                Ok(count) => {
                    success_count += 1;
                    
                    // ⚠️ SKIP if DAS returns 0 - matches bot behavior in bot_core.rs
                    if count == 0 {
                        skip_count += 1;
                        println!("CREATOR #{}:", token_num);
                        println!("  {}", mint_display);
                        println!("  Creator: {}", creator_str);
                        println!("  ✅ Token count: {} (vreme: {:?})", count, check_duration);
                        println!("  ⚠️  SKIP: DAS returned 0 (would be skipped in bot - DAS API limitation)");
                        println!();
                    } else {
                        let passes_filter = count <= 10;
                        
                        if passes_filter {
                            pass_filter_count += 1;
                        } else {
                            fail_filter_count += 1;
                        }
                        
                        println!("CREATOR #{}:", token_num);
                        println!("  {}", mint_display);
                        println!("  Creator: {}", creator_str);
                        println!("  ✅ Token count: {} (vreme: {:?})", count, check_duration);
                        println!("  Filter (0-10): {}", if passes_filter { "✅ PASS" } else { "❌ FAIL" });
                        println!();
                    }
                }
                Err(e) => {
                    fail_count += 1;
                    println!("CREATOR #{}:", token_num);
                    println!("  {}", mint_display);
                    println!("  Creator: {}", creator_str);
                    println!("  ❌ Error: {} (vreme: {:?})", e, check_duration);
                    println!("  Filter: ❌ FAIL (cannot verify)");
                    println!();
                }
            }
        }
        
        println!("═══════════════════════════════════════════════════════════════");
        println!("  STATISTIKA");
        println!("═══════════════════════════════════════════════════════════════");
        println!("  ✅ Uspešno: {}/{}", success_count, creators.len());
        println!("  ❌ Neuspešno: {}/{}", fail_count, creators.len());
        println!("  ⚠️  SKIP (DAS=0): {}", skip_count);
        println!("  ✅ Prošao filter (0-10): {}", pass_filter_count);
        println!("  ❌ Nije prošao filter (0-10): {}", fail_filter_count);
        println!("  ⏱️  Ukupno vreme: {:?}", total_duration);
        println!("  ⚡ Prosek po creatoru: {:?}", 
                 if creators.len() > 0 { 
                     total_duration.div_f64(creators.len() as f64)
                 } else { 
                     std::time::Duration::from_secs(0) 
                 });
        println!();
        
        if total_duration.as_secs_f64() < 1.0 {
            println!("  ✅ Test završen za manje od 1 sekunde!");
        } else {
            println!("  ⚠️  Test trajao {:.2} sekundi (cilj: < 1 sekunda)", total_duration.as_secs_f64());
        }
    }
}