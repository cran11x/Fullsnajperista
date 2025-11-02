// socials.rs - OPTIMIZED: Shorter timeouts for speed

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::time::Duration;

const HELIUS_API_KEY: &str = "7ef7af02-aa9d-4f5c-9c98-d5fa303d1f04";

// ⚡ AGGRESSIVE TIMEOUTS - Fail fast!
const DAS_TIMEOUT_MS: u64 = 1000;      // 1 second for DAS API
const IPFS_TIMEOUT_MS: u64 = 1500;     // 1.5 seconds for IPFS fetch
const TOTAL_TIMEOUT_MS: u64 = 2500;    // 2.5 seconds max total

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

/// ⚡ OPTIMIZED: Fast social check with aggressive timeouts
pub async fn check_token_socials(mint_address: &str) -> Result<Socials> {
    let start = std::time::Instant::now();

    // ⚡ Optimized client with connection pooling
    let client = reqwest::Client::builder()
        .timeout(Duration::from_millis(TOTAL_TIMEOUT_MS))
        .pool_idle_timeout(Duration::from_secs(30))
        .pool_max_idle_per_host(10)
        .build()?;

    // Step 1: Get json_uri from DAS API (aggressive timeout)
    let das_url = format!("https://mainnet.helius-rpc.com/?api-key={}", HELIUS_API_KEY);
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

    // Step 3: Return socials
    Ok(Socials {
        twitter: metadata.twitter,
        website: metadata.website,
        telegram: metadata.telegram,
        discord: metadata.discord,
    })
}

/// 🚀 ULTRA FAST: Try to get socials, but don't wait forever
/// Returns None if check takes too long or fails
pub async fn quick_check_socials(mint_address: &str) -> Option<Socials> {
    match tokio::time::timeout(
        Duration::from_millis(TOTAL_TIMEOUT_MS),
        check_token_socials(mint_address)
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
    }
}