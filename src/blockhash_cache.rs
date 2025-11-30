// blockhash_cache.rs - Blockhash caching for reduced RPC calls
#![allow(dead_code)]

use anyhow::Result;
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::hash::Hash;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const BLOCKHASH_TTL_SECS: u64 = 50; // Cache blockhash for 50 seconds (valid for ~60s)

/// Cached blockhash with timestamp
struct CachedBlockhash {
    hash: Hash,
    cached_at: Instant,
}

/// Global blockhash cache
static BLOCKHASH_CACHE: once_cell::sync::Lazy<Arc<Mutex<Option<CachedBlockhash>>>> =
    once_cell::sync::Lazy::new(|| Arc::new(Mutex::new(None)));

/// Get cached blockhash or fetch new one if cache is expired
/// 
/// Blockhashes are valid for ~60 seconds, so we cache them for 50 seconds
/// to ensure we always use a valid blockhash while reducing RPC calls.
pub async fn get_cached_blockhash(rpc: &RpcClient) -> Result<Hash> {
    // Check cache first
    {
        let cache_guard = BLOCKHASH_CACHE.lock().unwrap();
        if let Some(cached) = cache_guard.as_ref() {
            let age = cached.cached_at.elapsed();
            if age < Duration::from_secs(BLOCKHASH_TTL_SECS) {
                // Cache is still valid
                return Ok(cached.hash);
            }
        }
    }
    
    // Cache is expired or doesn't exist, fetch new blockhash
    let hash = rpc.get_latest_blockhash().await?;
    
    // Update cache
    {
        let mut cache_guard = BLOCKHASH_CACHE.lock().unwrap();
        *cache_guard = Some(CachedBlockhash {
            hash,
            cached_at: Instant::now(),
        });
    }
    
    Ok(hash)
}

/// Clear the blockhash cache (useful for testing or forced refresh)
pub fn clear_cache() {
    let mut cache_guard = BLOCKHASH_CACHE.lock().unwrap();
    *cache_guard = None;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_cache_ttl() {
        // Test that TTL is set correctly
        assert_eq!(BLOCKHASH_TTL_SECS, 50);
    }

    #[tokio::test]
    #[ignore] // Requires network
    async fn test_cache_usage() {
        // Integration test - requires real RPC
        // This would test that cache is used when valid
        // and refreshed when expired
        let rpc = RpcClient::new("https://api.mainnet-beta.solana.com".to_string());
        
        // First call should fetch
        let hash1 = get_cached_blockhash(&rpc).await.unwrap();
        
        // Second call should use cache
        let hash2 = get_cached_blockhash(&rpc).await.unwrap();
        assert_eq!(hash1, hash2);
        
        // Clear cache
        clear_cache();
        
        // After clearing, should fetch again
        let hash3 = get_cached_blockhash(&rpc).await.unwrap();
        // hash3 might be same or different depending on timing
    }
}

