// fallback.rs - API fallback when Redis miss
use anyhow::Result;
use reqwest::Client;
use solana_sdk::pubkey::Pubkey;
use std::str::FromStr;
use std::time::Duration;

const TIMEOUT_SECONDS: u64 = 3;

/// Get count via API fallback
/// CRITICAL FIX #3: Enhanced Transactions API first (more reliable), then DAS
pub async fn get_count_via_api(address: &str, api_key: &str) -> Result<u64> {
    let creator = Pubkey::from_str(address)?;
    let client = Client::builder()
        .timeout(Duration::from_secs(TIMEOUT_SECONDS))
        .build()?;

    // STEP 1: Try Enhanced Transactions API first (more reliable for new devs)
    match try_enhanced_transactions_api(&creator, api_key, &client).await {
        Ok(count) if count > 0 => {
            tracing::info!("Enhanced Transactions API: {} = {}", address, count);
            return Ok(count);
        }
        Ok(_) => {
            // Enhanced returned 0, try DAS
        }
        Err(e) => {
            tracing::warn!("Enhanced Transactions API failed: {}, trying DAS", e);
        }
    }

    // STEP 2: Try DAS API (only if Enhanced returned 0)
    match try_das_api(&creator, api_key, &client).await {
        Ok(count) => {
            tracing::info!("DAS API: {} = {}", address, count);
            Ok(count)
        }
        Err(e) => {
            tracing::warn!("DAS API failed: {}", e);
            Ok(0) // Return 0 if both fail
        }
    }
}

/// Try Enhanced Transactions API
async fn try_enhanced_transactions_api(
    creator: &Pubkey,
    api_key: &str,
    client: &Client,
) -> Result<u64> {
    let url = format!(
        "https://api.helius.xyz/v0/addresses/{}/transactions?api-key={}&type=CREATE&source=PUMP_FUN",
        creator.to_string(),
        api_key
    );

    let response = client.get(&url).send().await?;

    if !response.status().is_success() {
        return Err(anyhow::anyhow!(
            "Enhanced Transactions API failed: {}",
            response.status()
        ));
    }

    let transactions: Vec<serde_json::Value> = response.json().await?;
    Ok(transactions.len() as u64)
}

/// Try DAS API with multiple methods
async fn try_das_api(creator: &Pubkey, api_key: &str, client: &Client) -> Result<u64> {
    let url = format!("https://mainnet.helius-rpc.com/?api-key={}", api_key);

    // METHOD 1: Try getAssetsByCreator with pagination
    let assets_body = serde_json::json!({
        "jsonrpc": "2.0",
        "id": "1",
        "method": "getAssetsByCreator",
        "params": {
            "creatorAddress": creator.to_string(),
            "onlyVerified": false,
            "page": 1,
            "limit": 1000
        }
    });

    let response = client.post(&url).json(&assets_body).send().await?;
    let json: serde_json::Value = response.json().await?;

    // Check for errors
    if json["error"].is_object() {
        return Err(anyhow::anyhow!("DAS API error: {:?}", json["error"]));
    }

    // Try to get total count directly
    if let Some(total) = json["result"]["total"].as_u64() {
        if total > 0 {
            return Ok(total);
        }
    }

    // If total not available, count items across pages
    let mut total_count = 0u64;
    let mut page = 1;
    let max_pages = 20; // Check up to 20 pages (20,000 tokens max)

    loop {
        let mut current_body = assets_body.clone();
        current_body["params"]["page"] = serde_json::json!(page);

        let response = client.post(&url).json(&current_body).send().await?;
        let json: serde_json::Value = response.json().await?;

        if json["error"].is_object() {
            break;
        }

        if let Some(assets) = json["result"]["items"].as_array() {
            if assets.is_empty() {
                break;
            }
            total_count += assets.len() as u64;

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

    // METHOD 2: Try searchAssets with creatorAddress
    let search_body = serde_json::json!({
        "jsonrpc": "2.0",
        "id": "1",
        "method": "searchAssets",
        "params": {
            "creatorAddress": creator.to_string(),
            "creatorVerified": false,
            "tokenType": "fungible",
            "page": 1,
            "limit": 1000
        }
    });

    let response = client.post(&url).json(&search_body).send().await?;
    let json: serde_json::Value = response.json().await?;

    if let Some(total) = json["result"]["total"].as_u64() {
        if total > 0 {
            return Ok(total);
        }
    }

    if let Some(items) = json["result"]["items"].as_array() {
        let count = items.len() as u64;
        if count > 0 && items.len() < 1000 {
            return Ok(count);
        }
    }

    Ok(0)
}

