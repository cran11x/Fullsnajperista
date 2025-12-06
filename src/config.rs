// config.rs - ENV-BASED CONFIGURATION WITH VALIDATION
#![allow(dead_code)]

use anyhow::{anyhow, Result};
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::pubkey::Pubkey;
use std::str::FromStr;
use std::collections::HashSet;

use crate::constants::PUMP_PROGRAM_ID;

/// Bot configuration loaded from environment variables
#[derive(Debug, Clone)]
pub struct Config {
    pub rpc_url: String,
    pub wss_url: String,
    pub helius_api_key: String,
    pub sol_price_usd: f64,
    pub buy_amount_sol: f64,
    pub priority_fee: u64,
    pub enable_dynamic_priority_fee: bool,
    pub compute_units: u32,
    pub one_shot_mode: bool,
    pub submission_mode: SubmissionMode,
    pub jito_tip: u64,
    pub require_socials: bool,
    pub require_twitter: bool,
    pub min_socials_count: usize,
    pub min_dev_buy_usd: f64,
    pub max_dev_buy_usd: f64,
    pub min_dev_tokens: usize,
    pub max_dev_tokens: usize,
    pub enable_tracker: bool,
    pub mock_buy: bool,
    pub mock_sell: bool,
    pub target_mint_address: Option<Pubkey>,
    pub pump_program_id: Pubkey,
    pub global_account: Pubkey,
    pub fee_recipient: Pubkey,
    pub event_authority: Pubkey,
    pub global_volume: Pubkey,
    pub fee_config: Pubkey,
    pub fee_program: Pubkey,
    pub enable_auto_sell: bool,
    pub stop_loss_percent: f64,
    pub take_profit_mc_usd: f64,
    pub sell_percent: f64,
    pub monitor_interval_sec: u64,
    pub blacklisted_tokens: HashSet<Pubkey>,
    pub blacklisted_creators: HashSet<Pubkey>,
    pub whitelisted_tokens: Option<HashSet<Pubkey>>, // None = svi dozvoljeni
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubmissionMode {
    Helius,
    Jito,
    Rpc,
    All, // Try all methods
}

impl SubmissionMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            SubmissionMode::Helius => "Helius",
            SubmissionMode::Jito => "Jito",
            SubmissionMode::Rpc => "RPC",
            SubmissionMode::All => "All",
        }
    }
}

impl Config {
    /// Load configuration from environment variables
    pub fn from_env() -> Result<Self> {
        // Load .env file if it exists (but only in non-test builds)
        #[cfg(not(test))]
        {
            dotenv::dotenv().ok();
        }

        let rpc_url = std::env::var("RPC_URL")
            .unwrap_or_else(|_| "https://mainnet.helius-rpc.com/?api-key=".to_string());
        
        let helius_api_key = std::env::var("HELIUS_API_KEY")
            .unwrap_or_else(|_| "7ef7af02-aa9d-4f5c-9c98-d5fa303d1f04".to_string());

        let wss_url = std::env::var("WSS_URL")
            .unwrap_or_else(|_| format!("wss://mainnet.helius-rpc.com/?api-key={}", helius_api_key));

        let sol_price_usd = std::env::var("SOL_PRICE_USD")
            .unwrap_or_else(|_| "137.0".to_string())
            .parse::<f64>()
            .map_err(|_| anyhow!("Invalid SOL_PRICE_USD"))?;

        let buy_amount_sol_str = std::env::var("BUY_AMOUNT_SOL")
            .unwrap_or_else(|_| "0.015".to_string());
        let buy_amount_sol = buy_amount_sol_str
            .parse::<f64>()
            .map_err(|_| anyhow!("Invalid BUY_AMOUNT_SOL: '{}'", buy_amount_sol_str))?;

        let priority_fee = std::env::var("PRIORITY_FEE")
            .unwrap_or_else(|_| "11000000".to_string())
            .parse::<u64>()
            .map_err(|_| anyhow!("Invalid PRIORITY_FEE"))?;

        let compute_units = std::env::var("COMPUTE_UNITS")
            .unwrap_or_else(|_| "200000".to_string())
            .parse::<u32>()
            .map_err(|_| anyhow!("Invalid COMPUTE_UNITS"))?;

        let one_shot_mode = std::env::var("ONE_SHOT_MODE")
            .unwrap_or_else(|_| "true".to_string())
            .parse::<bool>()
            .unwrap_or(true);

        let submission_mode_str = std::env::var("SUBMISSION_MODE")
            .unwrap_or_else(|_| "helius".to_string())
            .to_lowercase();
        
        let submission_mode = match submission_mode_str.as_str() {
            "helius" => SubmissionMode::Helius,
            "jito" => SubmissionMode::Jito,
            "rpc" => SubmissionMode::Rpc,
            "all" => SubmissionMode::All,
            _ => SubmissionMode::Helius,
        };

        let jito_tip = std::env::var("JITO_TIP_SOL")
            .unwrap_or_else(|_| "0.0015".to_string())
            .parse::<f64>()
            .map_err(|_| anyhow!("Invalid JITO_TIP_SOL"))?;

        let require_socials = std::env::var("REQUIRE_SOCIALS")
            .unwrap_or_else(|_| "false".to_string())
            .parse::<bool>()
            .unwrap_or(false);

        let require_twitter = std::env::var("REQUIRE_TWITTER")
            .unwrap_or_else(|_| "false".to_string())
            .parse::<bool>()
            .unwrap_or(false);

        let min_socials_count = std::env::var("MIN_SOCIALS_COUNT")
            .unwrap_or_else(|_| "0".to_string())
            .parse::<usize>()
            .unwrap_or(0);

        let min_dev_buy_usd = std::env::var("MIN_DEV_BUY_USD")
            .unwrap_or_else(|_| "100.0".to_string())
            .parse::<f64>()
            .map_err(|_| anyhow!("Invalid MIN_DEV_BUY_USD"))?;

        let max_dev_buy_usd = std::env::var("MAX_DEV_BUY_USD")
            .unwrap_or_else(|_| "1000.0".to_string())
            .parse::<f64>()
            .map_err(|_| anyhow!("Invalid MAX_DEV_BUY_USD"))?;

        let min_dev_tokens = std::env::var("MIN_DEV_TOKENS")
            .unwrap_or_else(|_| "0".to_string())
            .parse::<usize>()
            .unwrap_or(0);

        let max_dev_tokens = std::env::var("MAX_DEV_TOKENS")
            .unwrap_or_else(|_| "10".to_string())
            .parse::<usize>()
            .unwrap_or(10);

        let enable_tracker = std::env::var("ENABLE_TRACKER")
            .unwrap_or_else(|_| "true".to_string())
            .parse::<bool>()
            .unwrap_or(true);

        let mock_buy = std::env::var("MOCK_BUY")
            .unwrap_or_else(|_| "false".to_string())
            .parse::<bool>()
            .unwrap_or(false);

        let mock_sell = std::env::var("MOCK_SELL")
            .unwrap_or_else(|_| "false".to_string())
            .parse::<bool>()
            .unwrap_or(false);

        let pump_program_id = Pubkey::from_str(
            std::env::var("PUMP_PROGRAM_ID")
                .unwrap_or_else(|_| PUMP_PROGRAM_ID.to_string())
                .as_str()
        )?;

        let global_account = Pubkey::from_str(
            std::env::var("GLOBAL_ACCOUNT")
                .unwrap_or_else(|_| "4wTV1YmiEkRvAtNtsSGPtUrqRYQMe5SKy2uB4Jjaxnjf".to_string())
                .as_str()
        )?;

        let fee_recipient = Pubkey::from_str(
            std::env::var("FEE_RECIPIENT")
                .unwrap_or_else(|_| "CebN5WGQ4jvEPvsVU4EoHEpgzq1VV7AbicfhtW4xC9iM".to_string())
                .as_str()
        )?;

        let event_authority = Pubkey::from_str(
            std::env::var("EVENT_AUTHORITY")
                .unwrap_or_else(|_| "Ce6TQqeHC9p8KetsN6JsjHK7UTZk7nasjjnr7XxXp9F1".to_string())
                .as_str()
        )?;

        // Global Volume Accumulator is hardcoded - always use Hq2wp8uJ9jCPsYgNHex8RtqdvMPfVGoYwjvF1ATiwn2Y
        // Cannot be changed via environment variable
        let global_volume = Pubkey::from_str("Hq2wp8uJ9jCPsYgNHex8RtqdvMPfVGoYwjvF1ATiwn2Y")?;

        let fee_config = Pubkey::from_str(
            std::env::var("FEE_CONFIG")
                .unwrap_or_else(|_| "8Wf5TiAheLUqBrKXeYg2JtAFFMWtKdG2BSFgqUcPVwTt".to_string())
                .as_str()
        )?;

        let fee_program = Pubkey::from_str(
            std::env::var("FEE_PROGRAM")
                .unwrap_or_else(|_| "pfeeUxB6jkeY1Hxd7CsFCAjcbHA9rWtchMGdZ6VojVZ".to_string())
                .as_str()
        )?;

        // Optional: Target mint address (if set, only buy this specific token)
        let target_mint_address = std::env::var("TARGET_MINT_ADDRESS")
            .ok()
            .and_then(|s| {
                let trimmed = s.trim();
                if trimmed.is_empty() {
                    None
                } else {
                    Pubkey::from_str(trimmed).ok()
                }
            });

        // Auto-sell configuration
        let enable_auto_sell = std::env::var("ENABLE_AUTO_SELL")
            .unwrap_or_else(|_| "false".to_string())
            .parse::<bool>()
            .unwrap_or(false);

        let stop_loss_percent = std::env::var("STOP_LOSS_PERCENT")
            .unwrap_or_else(|_| "30.0".to_string())
            .parse::<f64>()
            .map_err(|_| anyhow!("Invalid STOP_LOSS_PERCENT"))?;

        let take_profit_mc_usd = std::env::var("TAKE_PROFIT_MC_USD")
            .unwrap_or_else(|_| "24000.0".to_string())
            .parse::<f64>()
            .map_err(|_| anyhow!("Invalid TAKE_PROFIT_MC_USD"))?;

        let sell_percent = std::env::var("SELL_PERCENT")
            .unwrap_or_else(|_| "100.0".to_string())
            .parse::<f64>()
            .map_err(|_| anyhow!("Invalid SELL_PERCENT"))?;

        let monitor_interval_sec = std::env::var("MONITOR_INTERVAL_SEC")
            .unwrap_or_else(|_| "5".to_string())
            .parse::<u64>()
            .map_err(|_| anyhow!("Invalid MONITOR_INTERVAL_SEC"))?;

        let enable_dynamic_priority_fee = std::env::var("ENABLE_DYNAMIC_PRIORITY_FEE")
            .unwrap_or_else(|_| "false".to_string())
            .parse::<bool>()
            .unwrap_or(false);

        // Parse blacklisted tokens (comma-separated base58 addresses)
        let blacklisted_tokens = std::env::var("BLACKLISTED_TOKENS")
            .unwrap_or_else(|_| String::new())
            .split(',')
            .filter_map(|s| {
                let trimmed = s.trim();
                if trimmed.is_empty() {
                    None
                } else {
                    Pubkey::from_str(trimmed).ok()
                }
            })
            .collect();

        // Parse blacklisted creators (comma-separated base58 addresses)
        let blacklisted_creators = std::env::var("BLACKLISTED_CREATORS")
            .unwrap_or_else(|_| String::new())
            .split(',')
            .filter_map(|s| {
                let trimmed = s.trim();
                if trimmed.is_empty() {
                    None
                } else {
                    Pubkey::from_str(trimmed).ok()
                }
            })
            .collect();

        // Parse whitelisted tokens (comma-separated base58 addresses, None = all allowed)
        let whitelisted_tokens = std::env::var("WHITELISTED_TOKENS")
            .ok()
            .and_then(|s| {
                let tokens: HashSet<Pubkey> = s.split(',')
                    .filter_map(|s| {
                        let trimmed = s.trim();
                        if trimmed.is_empty() {
                            None
                        } else {
                            Pubkey::from_str(trimmed).ok()
                        }
                    })
                    .collect();
                if tokens.is_empty() {
                    None
                } else {
                    Some(tokens)
                }
            });

        let config = Self {
            rpc_url: if rpc_url.ends_with('=') {
                format!("{}{}", rpc_url, helius_api_key)
            } else if !rpc_url.contains(&helius_api_key) && !rpc_url.contains("YOUR_KEY") {
                // If RPC_URL is set but doesn't contain API key, add it
                if rpc_url.contains("?") {
                    format!("{}&api-key={}", rpc_url, helius_api_key)
                } else {
                    format!("{}?api-key={}", rpc_url, helius_api_key)
                }
            } else {
                rpc_url
            },
            wss_url: if wss_url.contains("api-key=") && !wss_url.contains(&helius_api_key) && !wss_url.contains("YOUR_KEY") {
                // If WSS_URL has api-key placeholder or different key, replace it
                if let Some(pos) = wss_url.find("api-key=") {
                    let after_key = &wss_url[pos + 8..];
                    let end_pos = after_key.find(&['&', ' '][..]).unwrap_or(after_key.len());
                    format!("{}api-key={}{}", &wss_url[..pos + 8], helius_api_key, &wss_url[pos + 8 + end_pos..])
                } else {
                    format!("{}?api-key={}", wss_url, helius_api_key)
                }
            } else if wss_url.contains("YOUR_KEY") {
                // Replace placeholder
                wss_url.replace("YOUR_KEY", &helius_api_key)
            } else if !wss_url.contains("api-key=") {
                // Add API key if missing
                if wss_url.contains("?") {
                    format!("{}&api-key={}", wss_url, helius_api_key)
                } else {
                    format!("{}?api-key={}", wss_url, helius_api_key)
                }
            } else {
                wss_url
            },
            helius_api_key,
            sol_price_usd,
            buy_amount_sol,
            priority_fee,
            enable_dynamic_priority_fee,
            compute_units,
            one_shot_mode,
            submission_mode,
            jito_tip: (jito_tip * 1e9) as u64, // Convert SOL to lamports
            require_socials,
            require_twitter,
            min_socials_count,
            min_dev_buy_usd,
            max_dev_buy_usd,
            min_dev_tokens,
            max_dev_tokens,
            enable_tracker,
            mock_buy,
            mock_sell,
            target_mint_address,
            pump_program_id,
            global_account,
            fee_recipient,
            event_authority,
            global_volume,
            fee_config,
            fee_program,
            enable_auto_sell,
            stop_loss_percent,
            take_profit_mc_usd,
            sell_percent,
            monitor_interval_sec,
            blacklisted_tokens,
            blacklisted_creators,
            whitelisted_tokens,
        };

        config.validate()?;
        Ok(config)
    }

    /// Validate configuration values
    pub fn validate(&self) -> Result<()> {
        if self.buy_amount_sol <= 0.0 {
            return Err(anyhow!("BUY_AMOUNT_SOL must be > 0"));
        }

        if self.sol_price_usd <= 0.0 {
            return Err(anyhow!("SOL_PRICE_USD must be > 0"));
        }

        if self.min_dev_buy_usd >= self.max_dev_buy_usd {
            return Err(anyhow!("MIN_DEV_BUY_USD must be < MAX_DEV_BUY_USD"));
        }

        if self.min_dev_tokens > self.max_dev_tokens {
            return Err(anyhow!("MIN_DEV_TOKENS must be <= MAX_DEV_TOKENS"));
        }

        if self.compute_units == 0 {
            return Err(anyhow!("COMPUTE_UNITS must be > 0"));
        }

        // Validate auto-sell options
        if self.stop_loss_percent < 0.0 || self.stop_loss_percent > 100.0 {
            return Err(anyhow!("STOP_LOSS_PERCENT must be between 0 and 100"));
        }

        if self.take_profit_mc_usd <= 0.0 {
            return Err(anyhow!("TAKE_PROFIT_MC_USD must be > 0"));
        }

        if self.sell_percent <= 0.0 || self.sell_percent > 100.0 {
            return Err(anyhow!("SELL_PERCENT must be between 0 and 100"));
        }

        if self.monitor_interval_sec == 0 {
            return Err(anyhow!("MONITOR_INTERVAL_SEC must be > 0"));
        }

        Ok(())
    }

    /// Create RPC client from config
    pub fn create_rpc_client(&self) -> RpcClient {
        RpcClient::new(self.rpc_url.clone())
    }

    /// Get buy amount in lamports
    pub fn buy_amount_lamports(&self) -> u64 {
        (self.buy_amount_sol * 1e9) as u64
    }

    /// Get min/max dev buy in SOL
    pub fn min_dev_buy_sol(&self) -> f64 {
        self.min_dev_buy_usd / self.sol_price_usd
    }

    pub fn max_dev_buy_sol(&self) -> f64 {
        self.max_dev_buy_usd / self.sol_price_usd
    }

    /// Calculate dynamic priority fee based on recent network fees
    /// Returns the 75th percentile of recent prioritization fees, or falls back to configured fee
    pub async fn calculate_dynamic_priority_fee(&self, _rpc: &RpcClient) -> Result<u64> {
        use serde_json::json;
        use crate::utils::get_shared_http_client;

        // Use getRecentPrioritizationFees RPC method
        let request_body = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "getRecentPrioritizationFees",
            "params": []
        });

        let client = get_shared_http_client();
        let response: serde_json::Value = client
            .post(&self.rpc_url)
            .json(&request_body)
            .send()
            .await
            .map_err(|e| anyhow!("Failed to fetch priority fees: {}", e))?
            .json()
            .await
            .map_err(|e| anyhow!("Failed to parse priority fees response: {}", e))?;

        // Parse response: { "result": [{ "prioritizationFee": 1234 }, ...] }
        if let Some(result) = response["result"].as_array() {
            if result.is_empty() {
                // No recent fees, use configured fee
                return Ok(self.priority_fee);
            }

            // Extract fees and calculate percentile
            let mut fees: Vec<u64> = result
                .iter()
                .filter_map(|item| item["prioritizationFee"].as_u64())
                .collect();

            if fees.is_empty() {
                return Ok(self.priority_fee);
            }

            // Sort fees
            fees.sort();

            // Calculate 75th percentile (or use max if we want to be more aggressive)
            let percentile_index = (fees.len() as f64 * 0.75) as usize;
            let calculated_fee = fees[percentile_index.min(fees.len() - 1)];

            // Add 10% buffer to ensure transaction goes through
            let fee_with_buffer = (calculated_fee as f64 * 1.1) as u64;

            // Ensure minimum fee (at least the configured fee)
            Ok(fee_with_buffer.max(self.priority_fee))
        } else {
            // Invalid response, use configured fee
            Ok(self.priority_fee)
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            rpc_url: "https://mainnet.helius-rpc.com/?api-key=test".to_string(),
            wss_url: "wss://mainnet.helius-rpc.com/?api-key=test".to_string(),
            helius_api_key: "7ef7af02-aa9d-4f5c-9c98-d5fa303d1f04".to_string(),
            sol_price_usd: 137.0,
            buy_amount_sol: 0.015,
            priority_fee: 11_000_000,
            enable_dynamic_priority_fee: false,
            compute_units: 200_000,
            one_shot_mode: true,
            submission_mode: SubmissionMode::Helius,
            jito_tip: 1_500_000,
            require_socials: false,
            require_twitter: false,
            min_socials_count: 0,
            min_dev_buy_usd: 100.0,
            max_dev_buy_usd: 1000.0,
            min_dev_tokens: 0,
            max_dev_tokens: 10,
            enable_tracker: true,
            mock_buy: false,
            mock_sell: false,
            target_mint_address: None,
            pump_program_id: Pubkey::from_str("6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P").unwrap(),
            global_account: Pubkey::from_str("4wTV1YmiEkRvAtNtsSGPtUrqRYQMe5SKy2uB4Jjaxnjf").unwrap(),
            fee_recipient: Pubkey::from_str("CebN5WGQ4jvEPvsVU4EoHEpgzq1VV7AbicfhtW4xC9iM").unwrap(),
            event_authority: Pubkey::from_str("Ce6TQqeHC9p8KetsN6JsjHK7UTZk7nasjjnr7XxXp9F1").unwrap(),
            global_volume: Pubkey::from_str("Hq2wp8uJ9jCPsYgNHex8RtqdvMPfVGoYwjvF1ATiwn2Y").unwrap(),
            fee_config: Pubkey::from_str("8Wf5TiAheLUqBrKXeYg2JtAFFMWtKdG2BSFgqUcPVwTt").unwrap(),
            fee_program: Pubkey::from_str("pfeeUxB6jkeY1Hxd7CsFCAjcbHA9rWtchMGdZ6VojVZ").unwrap(),
            enable_auto_sell: false,
            stop_loss_percent: 30.0,
            take_profit_mc_usd: 24_000.0,
            sell_percent: 100.0,
            monitor_interval_sec: 5,
            blacklisted_tokens: HashSet::new(),
            blacklisted_creators: HashSet::new(),
            whitelisted_tokens: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    fn setup_test_env() {
        env::set_var("HELIUS_API_KEY", "test-api-key-123");
        env::set_var("SOL_PRICE_USD", "150.0");
        env::set_var("BUY_AMOUNT_SOL", "0.02");
        env::set_var("PRIORITY_FEE", "5000000");
        env::set_var("COMPUTE_UNITS", "300000");
    }

    fn cleanup_test_env() {
        env::remove_var("HELIUS_API_KEY");
        env::remove_var("SOL_PRICE_USD");
        env::remove_var("BUY_AMOUNT_SOL");
        env::remove_var("PRIORITY_FEE");
        env::remove_var("COMPUTE_UNITS");
        env::remove_var("TARGET_MINT_ADDRESS");
        env::remove_var("RPC_URL");
        env::remove_var("WSS_URL");
        env::remove_var("ONE_SHOT_MODE");
        env::remove_var("SUBMISSION_MODE");
        env::remove_var("JITO_TIP_SOL");
        env::remove_var("REQUIRE_SOCIALS");
        env::remove_var("REQUIRE_TWITTER");
        env::remove_var("MIN_SOCIALS_COUNT");
        env::remove_var("MIN_DEV_BUY_USD");
        env::remove_var("MAX_DEV_BUY_USD");
        env::remove_var("MIN_DEV_TOKENS");
        env::remove_var("MAX_DEV_TOKENS");
        env::remove_var("ENABLE_TRACKER");
        env::remove_var("TARGET_MINT_ADDRESS");
        env::remove_var("MOCK_BUY");
    }

    #[test]
    fn test_config_from_env() {
        setup_test_env();
        
        let config = Config::from_env();
        assert!(config.is_ok());
        
        let config = config.unwrap();
        assert_eq!(config.helius_api_key, "test-api-key-123");
        assert_eq!(config.sol_price_usd, 150.0);
        assert_eq!(config.buy_amount_sol, 0.02);
        assert_eq!(config.priority_fee, 5000000);
        assert_eq!(config.compute_units, 300000);
        
        cleanup_test_env();
    }

    #[test]
    fn test_config_validation() {
        let mut config = Config::default();
        
        // Valid config should pass
        assert!(config.validate().is_ok());
        
        // Invalid: buy_amount_sol <= 0
        config.buy_amount_sol = 0.0;
        assert!(config.validate().is_err());
        
        config.buy_amount_sol = -1.0;
        assert!(config.validate().is_err());
        
        // Reset
        config.buy_amount_sol = 0.015;
        
        // Invalid: min >= max
        config.min_dev_buy_usd = 1200.0;
        config.max_dev_buy_usd = 500.0;
        assert!(config.validate().is_err());
        
        // Reset
        config.min_dev_buy_usd = 500.0;
        config.max_dev_buy_usd = 1200.0;
        
        // Invalid: compute_units == 0
        config.compute_units = 0;
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_config_defaults() {
        let config = Config::default();
        
        assert_eq!(config.sol_price_usd, 162.0);
        assert_eq!(config.buy_amount_sol, 0.015);
        assert_eq!(config.priority_fee, 11_000_000);
        assert_eq!(config.compute_units, 200_000);
        assert_eq!(config.submission_mode, SubmissionMode::Helius);
        assert!(!config.require_socials);
        assert!(!config.require_twitter);
    }

    #[test]
    fn test_config_submission_modes() {
        // Save original API key
        let orig_api_key = env::var("HELIUS_API_KEY").ok();
        
        // Setup all required env vars for Config::from_env
        setup_test_env();
        
        // Test Helius mode
        env::set_var("SUBMISSION_MODE", "helius");
        let config = Config::from_env();
        assert!(config.is_ok(), "Failed to create config with helius mode");
        if let Ok(cfg) = config {
            assert_eq!(cfg.submission_mode, SubmissionMode::Helius);
        }
        
        // Test Jito mode
        env::set_var("SUBMISSION_MODE", "jito");
        let config = Config::from_env();
        assert!(config.is_ok(), "Failed to create config with jito mode");
        if let Ok(cfg) = config {
            assert_eq!(cfg.submission_mode, SubmissionMode::Jito);
        }
        
        // Test RPC mode
        env::set_var("SUBMISSION_MODE", "rpc");
        let config = Config::from_env();
        assert!(config.is_ok(), "Failed to create config with rpc mode");
        if let Ok(cfg) = config {
            assert_eq!(cfg.submission_mode, SubmissionMode::Rpc);
        }
        
        // Test All mode
        env::set_var("SUBMISSION_MODE", "all");
        let config = Config::from_env();
        assert!(config.is_ok(), "Failed to create config with all mode");
        if let Ok(cfg) = config {
            assert_eq!(cfg.submission_mode, SubmissionMode::All);
        }
        
        // Restore original API key
        if let Some(key) = orig_api_key {
            env::set_var("HELIUS_API_KEY", key);
        } else {
            env::remove_var("HELIUS_API_KEY");
        }
    }

    #[test]
    fn test_config_helper_methods() {
        let config = Config::default();
        
        // Test buy_amount_lamports
        assert_eq!(config.buy_amount_lamports(), 15_000_000); // 0.015 SOL
        
        // Test min/max dev buy in SOL
        let min_sol = config.min_dev_buy_sol();
        let max_sol = config.max_dev_buy_sol();
        assert!(min_sol > 0.0);
        assert!(max_sol > min_sol);
        
        // Test RPC client creation
        let rpc = config.create_rpc_client();
        assert!(!rpc.url().is_empty());
    }

    #[test]
    fn test_config_missing_api_key() {
        cleanup_test_env();
        // Remove API key if it exists
        env::remove_var("HELIUS_API_KEY");
        
        let config = Config::from_env();
        assert!(config.is_err(), "Expected error when HELIUS_API_KEY is missing");
        assert!(config.unwrap_err().to_string().contains("HELIUS_API_KEY"));
    }

    #[test]
    fn test_config_invalid_numbers() {
        // Save original values
        let orig_sol_price = env::var("SOL_PRICE_USD").ok();
        let orig_buy_amount = env::var("BUY_AMOUNT_SOL").ok();
        
        // Ensure we have API key for other validations to pass
        env::set_var("HELIUS_API_KEY", "test-key");
        
        // Invalid SOL_PRICE_USD
        env::set_var("SOL_PRICE_USD", "not-a-number");
        let config = Config::from_env();
        assert!(config.is_err(), "Expected error for invalid SOL_PRICE_USD");
        
        // Reset
        env::set_var("SOL_PRICE_USD", "150.0");
        
        // Invalid BUY_AMOUNT_SOL
        env::set_var("BUY_AMOUNT_SOL", "invalid");
        let config = Config::from_env();
        assert!(config.is_err(), "Expected error for invalid BUY_AMOUNT_SOL");
        
        // Restore original values
        if let Some(val) = orig_sol_price {
            env::set_var("SOL_PRICE_USD", val);
        } else {
            env::remove_var("SOL_PRICE_USD");
        }
        if let Some(val) = orig_buy_amount {
            env::set_var("BUY_AMOUNT_SOL", val);
        } else {
            env::remove_var("BUY_AMOUNT_SOL");
        }
        env::remove_var("HELIUS_API_KEY");
    }

    #[test]
    fn test_target_mint_address_loading() {
        setup_test_env();
        
        // Ensure HELIUS_API_KEY is set for test
        if env::var("HELIUS_API_KEY").is_err() {
            env::set_var("HELIUS_API_KEY", "test_api_key_for_testing");
        }
        
        // Test without target mint (should be None)
        env::remove_var("TARGET_MINT_ADDRESS");
        let config = Config::from_env();
        assert!(config.is_ok(), "Config should load without TARGET_MINT_ADDRESS");
        let config = config.unwrap();
        assert!(config.target_mint_address.is_none());
        
        // Test with valid target mint
        let test_mint = "So11111111111111111111111111111111111111112"; // Valid Solana pubkey format
        env::set_var("TARGET_MINT_ADDRESS", test_mint);
        let config = Config::from_env();
        assert!(config.is_ok(), "Config should load with valid TARGET_MINT_ADDRESS");
        let config = config.unwrap();
        assert!(config.target_mint_address.is_some());
        assert_eq!(config.target_mint_address.unwrap().to_string(), test_mint);
        
        // Test with empty string (should be None)
        env::set_var("TARGET_MINT_ADDRESS", "");
        let config = Config::from_env();
        assert!(config.is_ok(), "Config should load with empty TARGET_MINT_ADDRESS");
        let config = config.unwrap();
        assert!(config.target_mint_address.is_none());
        
        // Test with invalid pubkey (should be None, but config should still load)
        env::set_var("TARGET_MINT_ADDRESS", "invalid_pubkey");
        let config = Config::from_env();
        // Config should still load, but target_mint_address should be None
        assert!(config.is_ok(), "Config should load even with invalid TARGET_MINT_ADDRESS");
        if let Ok(cfg) = config {
            assert!(cfg.target_mint_address.is_none());
        }
        
        cleanup_test_env();
    }

    #[test]
    fn test_target_mint_address_default() {
        let config = Config::default();
        assert!(config.target_mint_address.is_none());
    }
}

