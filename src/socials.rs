// socials.rs - Fast social media validation

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::time::Duration;

const HELIUS_API_KEY: &str = "7ef7af02-aa9d-4f5c-9c98-d5fa303d1f04";

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

    /// Display socials
    pub fn display(&self) {
        if let Some(ref twitter) = self.twitter {
            println!("      🐦 Twitter: {}", twitter);
        }
        if let Some(ref website) = self.website {
            println!("      🌐 Website: {}", website);
        }
        if let Some(ref telegram) = self.telegram {
            println!("      💬 Telegram: {}", telegram);
        }
        if let Some(ref discord) = self.discord {
            println!("      💬 Discord: {}", discord);
        }
    }
}

/// Fast check for token socials (with timeout)
pub async fn check_token_socials(mint_address: &str) -> Result<Socials> {
    // ⚡ Short timeout - don't slow down the sniper!
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(3))
        .build()?;

    // Step 1: Get json_uri from DAS API
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

    let das_response: DasResponse = client
        .post(&das_url)
        .json(&request_body)
        .send()
        .await?
        .json()
        .await?;

    let json_uri = &das_response.result.content.json_uri;

    // Step 2: Fetch metadata from IPFS/Arweave
    let metadata: TokenMetadata = client
        .get(json_uri)
        .timeout(Duration::from_secs(2))  // Extra timeout for IPFS
        .send()
        .await?
        .json()
        .await?;

    // Step 3: Return socials
    Ok(Socials {
        twitter: metadata.twitter,
        website: metadata.website,
        telegram: metadata.telegram,
        discord: metadata.discord,
    })
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
}