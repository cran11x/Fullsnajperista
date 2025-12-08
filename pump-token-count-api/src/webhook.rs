// webhook.rs - Helius webhook handler for CREATE token events
use anyhow::{anyhow, Result};
use redis::aio::ConnectionManager;
use serde::{Deserialize, Serialize};
use solana_sdk::pubkey::Pubkey;
use std::str::FromStr;
use std::time::{SystemTime, UNIX_EPOCH};

const PUMP_PROGRAM_ID: &str = "6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P";
const REDIS_KEY_PREFIX_CREATES: &str = "pump:creates:";
const REDIS_KEY_PREFIX_TOKENS: &str = "pump:tokens:";
const REDIS_KEY_SET_RECENT: &str = "pump:recent_tokens";
const REDIS_TTL_SECONDS: u64 = 30 * 24 * 60 * 60; // 30 days
const REDIS_RECENT_TTL_SECONDS: u64 = 24 * 60 * 60; // 24 hours for recent tokens set

#[derive(Debug, Serialize, Deserialize)]
pub struct HeliusWebhookPayload {
    pub webhook_id: Option<String>,
    pub timestamp: Option<u64>,
    pub data: Vec<HeliusTransactionData>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct HeliusTransactionData {
    #[serde(rename = "accountData")]
    pub account_data: Option<Vec<serde_json::Value>>,
    #[serde(rename = "nativeTransfers")]
    pub native_transfers: Option<Vec<serde_json::Value>>,
    #[serde(rename = "tokenTransfers")]
    pub token_transfers: Option<Vec<serde_json::Value>>,
    #[serde(rename = "transactionError")]
    pub transaction_error: Option<serde_json::Value>,
    pub instructions: Option<Vec<serde_json::Value>>,
    pub events: Option<serde_json::Value>,
    pub signature: Option<String>,
    pub slot: Option<u64>,
    #[serde(rename = "accountKeys")]
    pub account_keys: Option<Vec<serde_json::Value>>,
    pub logs: Option<Vec<String>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TokenCreateData {
    pub mint: String,
    pub creator: String,
    pub signature: String,
    pub timestamp: u64,
    pub slot: Option<u64>,
}

/// Handle Helius webhook POST request
pub async fn handle_helius_webhook(
    payload: HeliusWebhookPayload,
    redis: &mut ConnectionManager,
) -> Result<usize> {
    let mut processed_count = 0;

    for tx_data in payload.data {
        // Skip failed transactions
        if tx_data.transaction_error.is_some() {
            continue;
        }

        // Check if this is a CREATE instruction
        if let Some(logs) = &tx_data.logs {
            let has_create = logs.iter().any(|log| {
                log.contains("Program log: Instruction: Create")
                    || log.contains("Instruction: Create")
            });

            if !has_create {
                continue;
            }
        } else {
            // If no logs, check instructions
            if let Some(instructions) = &tx_data.instructions {
                let has_create = instructions.iter().any(|ix| {
                    if let Some(program_id) = ix.get("programId").and_then(|v| v.as_str()) {
                        program_id == PUMP_PROGRAM_ID
                            && ix
                                .get("data")
                                .and_then(|v| v.as_str())
                                .map_or(false, |d| d.starts_with("create"))
                    } else {
                        false
                    }
                });

                if !has_create {
                    continue;
                }
            } else {
                continue;
            }
        }

        // Extract signature
        let signature = tx_data
            .signature
            .ok_or_else(|| anyhow!("Missing transaction signature"))?;

        // Extract creator and mint from account keys
        let (creator, mint) = extract_creator_and_mint(&tx_data)?;

        // Get timestamp
        let timestamp = payload.timestamp.unwrap_or_else(|| {
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs()
        });

        let token_data = TokenCreateData {
            mint: mint.clone(),
            creator: creator.clone(),
            signature: signature.clone(),
            timestamp,
            slot: tx_data.slot,
        };

        // Store in Redis
        store_token_create(&token_data, redis).await?;

        processed_count += 1;

        tracing::info!(
            "Processed CREATE token: mint={}, creator={}, signature={}",
            mint,
            creator,
            signature
        );
    }

    Ok(processed_count)
}

/// Extract creator and mint from transaction data
fn extract_creator_and_mint(tx_data: &HeliusTransactionData) -> Result<(String, String)> {
    // Try to extract from account keys
    if let Some(account_keys) = &tx_data.account_keys {
        // Find creator (first signer that is not pump program, not system program, not compute budget)
        let creator = account_keys
            .iter()
            .enumerate()
            .find_map(|(idx, key)| {
                let key_str = key.as_str()?;
                if key_str != PUMP_PROGRAM_ID
                    && !key_str.starts_with("ComputeBudget")
                    && !key_str.contains("SystemProgram")
                    && !key_str.contains("11111111111111111111111111111111")
                {
                    // Check if it's a signer
                    if let Some(meta) = key.as_object() {
                        if meta.get("signer").and_then(|v| v.as_bool()).unwrap_or(false) {
                            return Some(key_str.to_string());
                        }
                    } else {
                        // If it's just a string, assume it's a signer if it's early in the list
                        if idx < 5 {
                            return Some(key_str.to_string());
                        }
                    }
                }
                None
            })
            .ok_or_else(|| anyhow!("Could not find creator in account keys"))?;

        // Find mint - usually in token transfers or account data
        let mint = extract_mint_from_tx(tx_data, &creator)?;

        Ok((creator, mint))
    } else {
        Err(anyhow!("Missing account keys in transaction data"))
    }
}

/// Extract mint address from transaction
fn extract_mint_from_tx(tx_data: &HeliusTransactionData, creator: &str) -> Result<String> {
    // Try token transfers first
    if let Some(token_transfers) = &tx_data.token_transfers {
        for transfer in token_transfers {
            if let Some(mint) = transfer.get("mint").and_then(|v| v.as_str()) {
                // Validate it's a valid pubkey
                if Pubkey::from_str(mint).is_ok() {
                    return Ok(mint.to_string());
                }
            }
        }
    }

    // Try account data
    if let Some(account_data) = &tx_data.account_data {
        for account in account_data {
            if let Some(address) = account.get("account").and_then(|v| v.as_str()) {
                // Skip known programs and creator
                if address != PUMP_PROGRAM_ID
                    && address != creator
                    && !address.contains("SystemProgram")
                    && !address.contains("ComputeBudget")
                {
                    // Check if it might be a mint (token account or mint account)
                    if Pubkey::from_str(address).is_ok() {
                        // Try to get more info from account data
                        if let Some(data) = account.get("nativeBalanceChange") {
                            // This might be a token account
                            return Ok(address.to_string());
                        }
                    }
                }
            }
        }
    }

    // Try instructions to find mint
    if let Some(instructions) = &tx_data.instructions {
        for ix in instructions {
            if let Some(accounts) = ix.get("accounts").and_then(|v| v.as_array()) {
                for account in accounts {
                    if let Some(addr) = account.as_str() {
                        if addr != PUMP_PROGRAM_ID
                            && addr != creator
                            && !addr.contains("SystemProgram")
                            && !addr.contains("ComputeBudget")
                            && !addr.contains("Token")
                            && Pubkey::from_str(addr).is_ok()
                        {
                            // This might be the mint
                            return Ok(addr.to_string());
                        }
                    }
                }
            }
        }
    }

    Err(anyhow!("Could not extract mint address from transaction"))
}

/// Store token create data in Redis
async fn store_token_create(
    token_data: &TokenCreateData,
    redis: &mut ConnectionManager,
) -> Result<()> {
    // 1. Increment creator count (same as WebSocket listener)
    let creator_key = format!("{}{}", REDIS_KEY_PREFIX_CREATES, token_data.creator);
    let mut pipe = redis::pipe();
    pipe.incr(&creator_key, 1u64)
        .expire(&creator_key, REDIS_TTL_SECONDS as i64);

    // 2. Store full token data
    let token_key = format!("{}{}", REDIS_KEY_PREFIX_TOKENS, token_data.mint);
    let token_json = serde_json::to_string(token_data)?;
    pipe.set(&token_key, &token_json)
        .expire(&token_key, REDIS_TTL_SECONDS as i64);

    // 3. Add to recent tokens set (sorted set by timestamp)
    pipe.zadd(
        REDIS_KEY_SET_RECENT,
        &token_data.mint,
        token_data.timestamp as i64,
    )
    .expire(REDIS_KEY_SET_RECENT, REDIS_RECENT_TTL_SECONDS as i64);

    let _: () = pipe.query_async(redis).await?;

    Ok(())
}

/// Get recent tokens from Redis (last N tokens)
pub async fn get_recent_tokens(
    limit: usize,
    redis: &mut ConnectionManager,
) -> Result<Vec<TokenCreateData>> {
    // Get most recent tokens from sorted set (highest timestamp first)
    let tokens: Vec<(String, i64)> = redis::cmd("ZREVRANGE")
        .arg(REDIS_KEY_SET_RECENT)
        .arg(0)
        .arg((limit - 1) as i64)
        .arg("WITHSCORES")
        .query_async(redis)
        .await?;

    let mut result = Vec::new();

    for (mint, _timestamp) in tokens {
        let token_key = format!("{}{}", REDIS_KEY_PREFIX_TOKENS, mint);
        if let Ok(Some(token_json)): Result<Option<String>, _> =
            redis::cmd("GET").arg(&token_key).query_async(redis).await
        {
            if let Ok(token_data) = serde_json::from_str::<TokenCreateData>(&token_json) {
                result.push(token_data);
            }
        }
    }

    Ok(result)
}

