// lib.rs - Public API for getting creator token count
use anyhow::Result;
use redis::aio::ConnectionManager;
use solana_sdk::pubkey::Pubkey;
use std::str::FromStr;

pub const REDIS_KEY_PREFIX: &str = "pump:creates:";
pub const REDIS_TTL_SECONDS: u64 = 30 * 24 * 60 * 60; // 30 days

/// Get creator token count from Redis
/// Returns count if found, None if Redis miss
/// This is the fast path (< 15ms)
pub async fn get_creator_count_from_redis(
    address: &str,
    redis: &mut ConnectionManager,
) -> Result<Option<u64>> {
    // Validate address is valid base58 Pubkey
    let _pubkey = Pubkey::from_str(address)
        .map_err(|_| anyhow::anyhow!("Invalid base58 address: {}", address))?;

    let redis_key = format!("{}{}", REDIS_KEY_PREFIX, address);

    // Try Redis first (fast path - < 15ms)
    let count: Option<u64> = redis::cmd("GET")
        .arg(&redis_key)
        .query_async(redis)
        .await
        .ok()
        .flatten();

    Ok(count)
}

/// Cache count in Redis with TTL
pub async fn cache_count(
    address: &str,
    count: u64,
    redis: &mut ConnectionManager,
) -> Result<()> {
    if count > 0 {
        let redis_key = format!("{}{}", REDIS_KEY_PREFIX, address);
        let mut pipe = redis::pipe();
        pipe.set(&redis_key, count)
            .expire(&redis_key, REDIS_TTL_SECONDS as i64);
        let _: () = pipe.query_async(redis).await?;
    }
    Ok(())
}

