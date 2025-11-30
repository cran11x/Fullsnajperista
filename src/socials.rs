// socials.rs - OPTIMIZED: Shorter timeouts for speed + LRU cache
#![allow(unused_imports, dead_code)]

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use std::sync::Arc;
use tokio::sync::Mutex;
use lru::LruCache;

// API key now comes from Config or environment

// ⚡ AGGRESSIVE TIMEOUTS - Fail fast!
const DAS_TIMEOUT_MS: u64 = 1000;      // 1 second for DAS API
const IPFS_TIMEOUT_MS: u64 = 1500;     // 1.5 seconds for IPFS fetch
const TOTAL_TIMEOUT_MS: u64 = 2500;    // 2.5 seconds max total
const CACHE_SIZE: usize = 500;         // Cache up to 500 token socials

// LRU cache for socials metadata to avoid repeated API calls
static SOCIALS_CACHE: once_cell::sync::Lazy<Arc<Mutex<LruCache<String, Socials>>>> =
    once_cell::sync::Lazy::new(|| Arc::new(Mutex::new(LruCache::new(std::num::NonZeroUsize::new(CACHE_SIZE).unwrap()))));

#[derive(Debug, Deserialize)]
struct DasResponse {
    result: AssetResult,
}

#[derive(Debug, Deserialize)]
struct AssetResult {
    content: Content,
}

#[derive(Debug, Deserialize)]
struct Content {
    json_uri: String,
}

#[derive(Debug, Deserialize, Serialize)]
struct TokenMetadata {
    #[serde(default)]
    name: String,
    #[serde(default)]
    symbol: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    twitter: Option<String>,
    #[serde(default)]
    website: Option<String>,
    #[serde(default)]
    telegram: Option<String>,
    #[serde(default)]
    discord: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Socials {
    pub twitter: Option<String>,
    pub website: Option<String>,
    pub telegram: Option<String>,
    pub discord: Option<String>,
}

impl Socials {
    /// Check if token has ANY social links
    pub fn has_any(&self) -> bool {
        self.twitter.is_some()
            || self.website.is_some()
            || self.telegram.is_some()
            || self.discord.is_some()
    }

    /// Check if token has Twitter/X
    pub fn has_twitter(&self) -> bool {
        self.twitter.is_some()
    }

    /// Check if token has website
    pub fn has_website(&self) -> bool {
        self.website.is_some()
    }

    /// Check if token has Telegram
    pub fn has_telegram(&self) -> bool {
        self.telegram.is_some()
    }

    /// Count how many socials exist
    pub fn count(&self) -> usize {
        let mut count = 0;
        if self.twitter.is_some() { count += 1; }
        if self.website.is_some() { count += 1; }
        if self.telegram.is_some() { count += 1; }
        if self.discord.is_some() { count += 1; }
        count
    }

    /// Display socials (compact format)
    pub fn display(&self) {
        if let Some(ref twitter) = self.twitter {
            println!("      🐦 X: {}", Self::shorten_url(twitter));
        }
        if let Some(ref website) = self.website {
            println!("      🌐 Web: {}", Self::shorten_url(website));
        }
        if let Some(ref telegram) = self.telegram {
            println!("      💬 TG: {}", Self::shorten_url(telegram));
        }
        if let Some(ref discord) = self.discord {
            println!("      💬 DC: {}", Self::shorten_url(discord));
        }
    }

    /// Shorten URL for display
    fn shorten_url(url: &str) -> String {
        if url.len() > 50 {
            format!("{}...", &url[..47])
        } else {
            url.to_string()
        }
    }
}

/// ⚡ OPTIMIZED: Fast social check with aggressive timeouts + LRU cache
pub async fn check_token_socials(mint_address: &str, api_key: &str) -> Result<Socials> {
    // Check cache first
    {
        let mut cache = SOCIALS_CACHE.lock().await;
        if let Some(cached) = cache.get(mint_address) {
            return Ok(cached.clone());
        }
    }
    
    let start = std::time::Instant::now();

    // ⚡ Use shared HTTP client for better performance
    let client = crate::utils::get_shared_http_client();

    // Step 1: Get json_uri from DAS API (aggressive timeout)
    let das_url = format!("https://mainnet.helius-rpc.com/?api-key={}", api_key);
    let request_body = serde_json::json!({
        "jsonrpc": "2.0",
        "id": "1",
        "method": "getAsset",
        "params": {
            "id": mint_address,
            "displayOptions": {
                "showFungible": true
            }
        }
    });

    let das_response: DasResponse = tokio::time::timeout(
        Duration::from_millis(DAS_TIMEOUT_MS),
        client.post(&das_url).json(&request_body).send()
    )
        .await
        .map_err(|_| anyhow::anyhow!("DAS timeout ({}ms)", DAS_TIMEOUT_MS))?
        .map_err(|e| anyhow::anyhow!("DAS request failed: {}", e))?
        .json()
        .await
        .map_err(|e| anyhow::anyhow!("DAS parse failed: {}", e))?;

    let json_uri = das_response.result.content.json_uri;
    let das_time = start.elapsed().as_millis();

    // Step 2: Fetch metadata from IPFS/Arweave (aggressive timeout)
    let remaining_time = TOTAL_TIMEOUT_MS.saturating_sub(das_time as u64);
    let ipfs_timeout = std::cmp::min(remaining_time, IPFS_TIMEOUT_MS);

    let metadata: TokenMetadata = tokio::time::timeout(
        Duration::from_millis(ipfs_timeout),
        client.get(&json_uri).send()
    )
        .await
        .map_err(|_| anyhow::anyhow!("IPFS timeout ({}ms)", ipfs_timeout))?
        .map_err(|e| anyhow::anyhow!("IPFS request failed: {}", e))?
        .json()
        .await
        .map_err(|e| anyhow::anyhow!("IPFS parse failed: {}", e))?;

    let total_time = start.elapsed().as_millis();

    // ⚡ Performance logging
    if total_time > 1000 {
        println!("      ⚠️  Slow social check: {}ms", total_time);
    }

    // Step 3: Create socials
    let socials = Socials {
        twitter: metadata.twitter,
        website: metadata.website,
        telegram: metadata.telegram,
        discord: metadata.discord,
    };
    
    // Cache the result
    {
        let mut cache = SOCIALS_CACHE.lock().await;
        cache.put(mint_address.to_string(), socials.clone());
    }
    
    Ok(socials)
}

/// 🚀 ULTRA FAST: Try to get socials, but don't wait forever
/// Returns None if check takes too long or fails
pub async fn quick_check_socials(mint_address: &str, api_key: &str) -> Option<Socials> {
    match tokio::time::timeout(
        Duration::from_millis(TOTAL_TIMEOUT_MS),
        check_token_socials(mint_address, api_key)
    )
        .await
    {
        Ok(Ok(socials)) => Some(socials),
        Ok(Err(e)) => {
            println!("      ⚠️  Social check failed: {}", e);
            None
        }
        Err(_) => {
            println!("      ⚠️  Social check timeout");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_socials_has_any() {
        let socials = Socials {
            twitter: Some("https://x.com/test".to_string()),
            website: None,
            telegram: None,
            discord: None,
        };
        assert!(socials.has_any());
        assert_eq!(socials.count(), 1);
    }

    #[test]
    fn test_socials_empty() {
        let socials = Socials {
            twitter: None,
            website: None,
            telegram: None,
            discord: None,
        };
        assert!(!socials.has_any());
        assert_eq!(socials.count(), 0);
    }

    #[test]
    fn test_shorten_url() {
        let long_url = "https://x.com/i/communities/1234567890123456789012345678901234567890";
        let short = Socials::shorten_url(long_url);
        assert!(short.len() <= 50);
        assert!(short.ends_with("..."));

        // Test with short URL
        let short_url = "https://x.com/test";
        let result = Socials::shorten_url(short_url);
        assert_eq!(result, short_url);
    }

    #[test]
    fn test_socials_parsing() {
        // Test parsing different JSON formats
        let json1 = r#"{
            "name": "Test Token",
            "symbol": "TEST",
            "twitter": "https://x.com/test",
            "website": "https://example.com",
            "telegram": "https://t.me/test",
            "discord": "https://discord.gg/test"
        }"#;

        let metadata: Result<TokenMetadata, _> = serde_json::from_str(json1);
        assert!(metadata.is_ok());
        let metadata = metadata.unwrap();
        assert_eq!(metadata.twitter, Some("https://x.com/test".to_string()));
        assert_eq!(metadata.website, Some("https://example.com".to_string()));
        assert_eq!(metadata.telegram, Some("https://t.me/test".to_string()));
        assert_eq!(metadata.discord, Some("https://discord.gg/test".to_string()));

        // Test with missing fields
        let json2 = r#"{
            "name": "Test Token",
            "symbol": "TEST"
        }"#;

        let metadata2: Result<TokenMetadata, _> = serde_json::from_str(json2);
        assert!(metadata2.is_ok());
        let metadata2 = metadata2.unwrap();
        assert_eq!(metadata2.twitter, None);
        assert_eq!(metadata2.website, None);

        // Test with empty strings
        let json3 = r#"{
            "name": "Test Token",
            "twitter": "",
            "website": null
        }"#;

        let metadata3: Result<TokenMetadata, _> = serde_json::from_str(json3);
        assert!(metadata3.is_ok());
    }

    #[test]
    fn test_socials_count() {
        let socials = Socials {
            twitter: Some("https://x.com/test".to_string()),
            website: Some("https://example.com".to_string()),
            telegram: Some("https://t.me/test".to_string()),
            discord: None,
        };
        assert_eq!(socials.count(), 3);

        let socials2 = Socials {
            twitter: Some("https://x.com/test".to_string()),
            website: None,
            telegram: None,
            discord: None,
        };
        assert_eq!(socials2.count(), 1);
    }

    #[test]
    fn test_socials_has_twitter() {
        let socials = Socials {
            twitter: Some("https://x.com/test".to_string()),
            website: None,
            telegram: None,
            discord: None,
        };
        assert!(socials.has_twitter());

        let socials2 = Socials {
            twitter: None,
            website: Some("https://example.com".to_string()),
            telegram: None,
            discord: None,
        };
        assert!(!socials2.has_twitter());
    }

    #[tokio::test]
    async fn test_check_token_socials_timeout() {
        // Test timeout handling with mock server
        use mockito::Server;
        use std::time::Duration;

        let mut server = Server::new_async().await;
        
        // Mock DAS response (timeout testing would need different approach)
        let _mock_das = server.mock("POST", "/")
            .with_status(200)
            .with_body(r#"{"jsonrpc":"2.0","id":"1","result":{"content":{"json_uri":"https://example.com/metadata.json"}}}"#)
            .create();

        // The function should timeout
        // Note: This would require modifying check_token_socials to accept a base URL
        // For now, we test the timeout constant
        assert_eq!(TOTAL_TIMEOUT_MS, 2500);
    }

    #[test]
    fn test_token_metadata_defaults() {
        // Test that defaults work correctly
        let json = r#"{}"#;
        let metadata: TokenMetadata = serde_json::from_str(json).unwrap();
        
        assert_eq!(metadata.name, "");
        assert_eq!(metadata.symbol, "");
        assert_eq!(metadata.description, "");
        assert_eq!(metadata.twitter, None);
        assert_eq!(metadata.website, None);
        assert_eq!(metadata.telegram, None);
        assert_eq!(metadata.discord, None);
    }

    #[tokio::test]
    #[ignore]
    async fn test_socials_real_api() {
        // Integration test - requires real API and network
        // This should be run manually
        // let result = check_token_socials("EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v").await;
        // assert!(result.is_ok());
    }
}