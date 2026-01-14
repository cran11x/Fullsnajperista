// config.rs - ENV-BASED CONFIGURATION WITH VALIDATION

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
    pub buy_amount_sol: f64,
    pub slippage_percent: u32, // Slippage tolerance in percent (e.g., 120 = 20% slippage)
    pub priority_fee: u64,
    pub enable_dynamic_priority_fee: bool,
    pub compute_units: u32,
    pub one_shot_mode: bool,
    pub submission_mode: SubmissionMode,
    pub jito_tip: u64,
    pub require_socials: bool,
    pub require_twitter: bool,
    pub require_website: bool,
    pub require_telegram: bool,
    pub require_discord: bool,
    pub min_socials_count: usize,
    pub enable_min_socials_count: bool,
    pub min_dev_buy_sol: f64,
    pub max_dev_buy_sol: f64,
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
    pub take_profit_mc_sol: f64,
    pub sell_percent: f64,
    pub monitor_interval_sec: u64,
    pub enable_dead_coin_sell: bool,
    pub dead_coin_timeout_sec: u64,
    pub blacklisted_tokens: HashSet<Pubkey>,
    pub blacklisted_creators: HashSet<Pubkey>,
    pub whitelisted_tokens: Option<HashSet<Pubkey>>, // None = svi dozvoljeni
    pub require_uppercase_token: bool,
    pub max_name_length: usize,
    pub min_ticker_length: usize,
    pub max_ticker_length: usize,
    // Advanced filters - Basic
    pub enable_has_twitter: bool,
    pub enable_has_telegram: bool,
    pub enable_has_website: bool,
    pub enable_social_count_1_plus: bool,
    pub enable_social_count_2_plus: bool,
    pub enable_social_count_3: bool,
    pub enable_website_com: bool,
    pub enable_website_org: bool,
    pub enable_website_xyz: bool,
    pub enable_uppercase: bool,
    pub enable_lowercase: bool,
    pub enable_symbol_3_4: bool,
    pub enable_symbol_3_6: bool,
    pub enable_symbol_3_7: bool,
    pub enable_name_short: bool,
    pub enable_name_medium: bool,
    // Advanced filters - Twitter types
    pub enable_twitter_account: bool,
    pub enable_twitter_community: bool,
    pub enable_twitter_status: bool,
    pub enable_twitter_no_status: bool,
    pub enable_twitter_account_or_community: bool,
    pub enable_twitter_username_length_short: bool,
    pub enable_twitter_username_length_medium: bool,
    pub enable_has_twitter_with_username: bool,
    // Advanced filters - Brand matching
    pub enable_has_brand_match: bool,
    pub enable_brand_score_2_plus: bool,
    pub enable_brand_score_3_plus: bool,
    pub enable_brand_score_4_plus: bool,
    pub enable_perfect_brand_match: bool,
    pub enable_twitter_matches_website: bool,
    pub enable_name_matches_website: bool,
    pub enable_symbol_matches_website: bool,
    pub enable_name_matches_twitter: bool,
    pub enable_symbol_matches_twitter: bool,
    // Advanced filters - Combined brand matching
    pub enable_name_matches_both: bool,
    pub enable_symbol_matches_both: bool,
    pub enable_twitter_and_name_match_website: bool,
    pub enable_twitter_and_symbol_match_website: bool,
    pub enable_name_and_symbol_match_twitter: bool,
    pub enable_brand_match_and_twitter: bool,
    pub enable_brand_match_and_website: bool,
    pub enable_brand_match_and_twitter_and_website: bool,
    pub enable_perfect_brand_and_twitter: bool,
    pub enable_perfect_brand_and_website: bool,
    pub enable_perfect_brand_and_twitter_community: bool,
    pub enable_brand_score_3_plus_and_com: bool,
    pub enable_brand_score_4_plus_and_com: bool,
    pub enable_twitter_match_website_and_com: bool,
    pub enable_name_match_website_and_com: bool,
    // Socials fetch configuration
    pub socials_das_timeout_ms: u64,
    pub socials_ipfs_timeout_ms: u64,
    pub socials_total_timeout_ms: u64,
    pub socials_max_retries: u32,
    pub socials_retry_delay_ms: u64,
    pub socials_max_concurrent: usize,
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

        let buy_amount_sol_str = std::env::var("BUY_AMOUNT_SOL")
            .unwrap_or_else(|_| "0.015".to_string());
        let buy_amount_sol = buy_amount_sol_str
            .parse::<f64>()
            .map_err(|_| anyhow!("Invalid BUY_AMOUNT_SOL: '{}'", buy_amount_sol_str))?;

        let slippage_percent_str = std::env::var("SLIPPAGE_PERCENT")
            .unwrap_or_else(|_| "200".to_string()); // Default 200% (100% slippage)
        let slippage_percent = slippage_percent_str
            .parse::<u32>()
            .map_err(|_| anyhow!("Invalid SLIPPAGE_PERCENT: '{}'", slippage_percent_str))?;
        
        // Validate slippage is reasonable (between 100% and 500%)
        if slippage_percent < 100 || slippage_percent > 500 {
            return Err(anyhow!("SLIPPAGE_PERCENT must be between 100 and 500 (got {})", slippage_percent));
        }

        let priority_fee = std::env::var("PRIORITY_FEE")
            .unwrap_or_else(|_| "11000000".to_string())
            .parse::<u64>()
            .map_err(|_| anyhow!("Invalid PRIORITY_FEE"))?;

        let compute_units = std::env::var("COMPUTE_UNITS")
            .unwrap_or_else(|_| "200000".to_string())
            .parse::<u32>()
            .map_err(|_| anyhow!("Invalid COMPUTE_UNITS"))?;

        let one_shot_mode = std::env::var("ONE_SHOT_MODE")
            .unwrap_or_else(|_| "false".to_string())
            .parse::<bool>()
            .unwrap_or(false);

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

        let require_website = std::env::var("REQUIRE_WEBSITE")
            .unwrap_or_else(|_| "false".to_string())
            .parse::<bool>()
            .unwrap_or(false);

        let require_telegram = std::env::var("REQUIRE_TELEGRAM")
            .unwrap_or_else(|_| "false".to_string())
            .parse::<bool>()
            .unwrap_or(false);

        let require_discord = std::env::var("REQUIRE_DISCORD")
            .unwrap_or_else(|_| "false".to_string())
            .parse::<bool>()
            .unwrap_or(false);

        let enable_min_socials_count = std::env::var("ENABLE_MIN_SOCIALS_COUNT")
            .unwrap_or_else(|_| "false".to_string())
            .parse::<bool>()
            .unwrap_or(false);

        let min_socials_count = std::env::var("MIN_SOCIALS_COUNT")
            .unwrap_or_else(|_| "0".to_string())
            .parse::<usize>()
            .unwrap_or(0);

        let min_dev_buy_sol = std::env::var("MIN_DEV_BUY_SOL")
            .unwrap_or_else(|_| "0.73".to_string()) // ~100 USD at 137 SOL/USD
            .parse::<f64>()
            .map_err(|_| anyhow!("Invalid MIN_DEV_BUY_SOL"))?;

        let max_dev_buy_sol = std::env::var("MAX_DEV_BUY_SOL")
            .unwrap_or_else(|_| "7.3".to_string()) // ~1000 USD at 137 SOL/USD
            .parse::<f64>()
            .map_err(|_| anyhow!("Invalid MAX_DEV_BUY_SOL"))?;

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

        let take_profit_mc_sol = std::env::var("TAKE_PROFIT_MC_SOL")
            .unwrap_or_else(|_| "175.0".to_string()) // ~24000 USD at 137 SOL/USD
            .parse::<f64>()
            .map_err(|_| anyhow!("Invalid TAKE_PROFIT_MC_SOL"))?;

        let sell_percent = std::env::var("SELL_PERCENT")
            .unwrap_or_else(|_| "100.0".to_string())
            .parse::<f64>()
            .map_err(|_| anyhow!("Invalid SELL_PERCENT"))?;

        let monitor_interval_sec = std::env::var("MONITOR_INTERVAL_SEC")
            .unwrap_or_else(|_| "5".to_string())
            .parse::<u64>()
            .map_err(|_| anyhow!("Invalid MONITOR_INTERVAL_SEC"))?;

        let enable_dead_coin_sell = std::env::var("ENABLE_DEAD_COIN_SELL")
            .unwrap_or_else(|_| "false".to_string())
            .parse::<bool>()
            .unwrap_or(false);

        let dead_coin_timeout_sec = std::env::var("DEAD_COIN_TIMEOUT_SEC")
            .unwrap_or_else(|_| "15".to_string())
            .parse::<u64>()
            .map_err(|_| anyhow!("Invalid DEAD_COIN_TIMEOUT_SEC"))?;

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

        // Token metadata filters
        let require_uppercase_token = std::env::var("REQUIRE_UPPERCASE_TOKEN")
            .unwrap_or_else(|_| "false".to_string())
            .parse::<bool>()
            .unwrap_or(false);

        let max_name_length = std::env::var("MAX_NAME_LENGTH")
            .unwrap_or_else(|_| "20".to_string())
            .parse::<usize>()
            .unwrap_or(20);

        let min_ticker_length = std::env::var("MIN_TICKER_LENGTH")
            .unwrap_or_else(|_| "3".to_string())
            .parse::<usize>()
            .unwrap_or(3);

        let max_ticker_length = std::env::var("MAX_TICKER_LENGTH")
            .unwrap_or_else(|_| "7".to_string())
            .parse::<usize>()
            .unwrap_or(7);

        // Advanced filters - Basic
        let enable_has_twitter = std::env::var("ENABLE_HAS_TWITTER")
            .unwrap_or_else(|_| "false".to_string())
            .parse::<bool>()
            .unwrap_or(false);
        let enable_has_telegram = std::env::var("ENABLE_HAS_TELEGRAM")
            .unwrap_or_else(|_| "false".to_string())
            .parse::<bool>()
            .unwrap_or(false);
        let enable_has_website = std::env::var("ENABLE_HAS_WEBSITE")
            .unwrap_or_else(|_| "false".to_string())
            .parse::<bool>()
            .unwrap_or(false);
        let enable_social_count_1_plus = std::env::var("ENABLE_SOCIAL_COUNT_1_PLUS")
            .unwrap_or_else(|_| "false".to_string())
            .parse::<bool>()
            .unwrap_or(false);
        let enable_social_count_2_plus = std::env::var("ENABLE_SOCIAL_COUNT_2_PLUS")
            .unwrap_or_else(|_| "false".to_string())
            .parse::<bool>()
            .unwrap_or(false);
        let enable_social_count_3 = std::env::var("ENABLE_SOCIAL_COUNT_3")
            .unwrap_or_else(|_| "false".to_string())
            .parse::<bool>()
            .unwrap_or(false);
        let enable_website_com = std::env::var("ENABLE_WEBSITE_COM")
            .unwrap_or_else(|_| "false".to_string())
            .parse::<bool>()
            .unwrap_or(false);
        let enable_website_org = std::env::var("ENABLE_WEBSITE_ORG")
            .unwrap_or_else(|_| "false".to_string())
            .parse::<bool>()
            .unwrap_or(false);
        let enable_website_xyz = std::env::var("ENABLE_WEBSITE_XYZ")
            .unwrap_or_else(|_| "false".to_string())
            .parse::<bool>()
            .unwrap_or(false);
        let enable_uppercase = std::env::var("ENABLE_UPPERCASE")
            .unwrap_or_else(|_| "false".to_string())
            .parse::<bool>()
            .unwrap_or(false);
        let enable_lowercase = std::env::var("ENABLE_LOWERCASE")
            .unwrap_or_else(|_| "false".to_string())
            .parse::<bool>()
            .unwrap_or(false);
        let enable_symbol_3_4 = std::env::var("ENABLE_SYMBOL_3_4")
            .unwrap_or_else(|_| "false".to_string())
            .parse::<bool>()
            .unwrap_or(false);
        let enable_symbol_3_6 = std::env::var("ENABLE_SYMBOL_3_6")
            .unwrap_or_else(|_| "false".to_string())
            .parse::<bool>()
            .unwrap_or(false);
        let enable_symbol_3_7 = std::env::var("ENABLE_SYMBOL_3_7")
            .unwrap_or_else(|_| "false".to_string())
            .parse::<bool>()
            .unwrap_or(false);
        let enable_name_short = std::env::var("ENABLE_NAME_SHORT")
            .unwrap_or_else(|_| "false".to_string())
            .parse::<bool>()
            .unwrap_or(false);
        let enable_name_medium = std::env::var("ENABLE_NAME_MEDIUM")
            .unwrap_or_else(|_| "false".to_string())
            .parse::<bool>()
            .unwrap_or(false);
        // Advanced filters - Twitter types
        let enable_twitter_account = std::env::var("ENABLE_TWITTER_ACCOUNT")
            .unwrap_or_else(|_| "false".to_string())
            .parse::<bool>()
            .unwrap_or(false);
        let enable_twitter_community = std::env::var("ENABLE_TWITTER_COMMUNITY")
            .unwrap_or_else(|_| "false".to_string())
            .parse::<bool>()
            .unwrap_or(false);
        let enable_twitter_status = std::env::var("ENABLE_TWITTER_STATUS")
            .unwrap_or_else(|_| "false".to_string())
            .parse::<bool>()
            .unwrap_or(false);
        let enable_twitter_no_status = std::env::var("ENABLE_TWITTER_NO_STATUS")
            .unwrap_or_else(|_| "false".to_string())
            .parse::<bool>()
            .unwrap_or(false);
        let enable_twitter_account_or_community = std::env::var("ENABLE_TWITTER_ACCOUNT_OR_COMMUNITY")
            .unwrap_or_else(|_| "false".to_string())
            .parse::<bool>()
            .unwrap_or(false);
        let enable_twitter_username_length_short = std::env::var("ENABLE_TWITTER_USERNAME_LENGTH_SHORT")
            .unwrap_or_else(|_| "false".to_string())
            .parse::<bool>()
            .unwrap_or(false);
        let enable_twitter_username_length_medium = std::env::var("ENABLE_TWITTER_USERNAME_LENGTH_MEDIUM")
            .unwrap_or_else(|_| "false".to_string())
            .parse::<bool>()
            .unwrap_or(false);
        let enable_has_twitter_with_username = std::env::var("ENABLE_HAS_TWITTER_WITH_USERNAME")
            .unwrap_or_else(|_| "false".to_string())
            .parse::<bool>()
            .unwrap_or(false);
        // Advanced filters - Brand matching
        let enable_has_brand_match = std::env::var("ENABLE_HAS_BRAND_MATCH")
            .unwrap_or_else(|_| "false".to_string())
            .parse::<bool>()
            .unwrap_or(false);
        let enable_brand_score_2_plus = std::env::var("ENABLE_BRAND_SCORE_2_PLUS")
            .unwrap_or_else(|_| "false".to_string())
            .parse::<bool>()
            .unwrap_or(false);
        let enable_brand_score_3_plus = std::env::var("ENABLE_BRAND_SCORE_3_PLUS")
            .unwrap_or_else(|_| "false".to_string())
            .parse::<bool>()
            .unwrap_or(false);
        let enable_brand_score_4_plus = std::env::var("ENABLE_BRAND_SCORE_4_PLUS")
            .unwrap_or_else(|_| "false".to_string())
            .parse::<bool>()
            .unwrap_or(false);
        let enable_perfect_brand_match = std::env::var("ENABLE_PERFECT_BRAND_MATCH")
            .unwrap_or_else(|_| "false".to_string())
            .parse::<bool>()
            .unwrap_or(false);
        let enable_twitter_matches_website = std::env::var("ENABLE_TWITTER_MATCHES_WEBSITE")
            .unwrap_or_else(|_| "false".to_string())
            .parse::<bool>()
            .unwrap_or(false);
        let enable_name_matches_website = std::env::var("ENABLE_NAME_MATCHES_WEBSITE")
            .unwrap_or_else(|_| "false".to_string())
            .parse::<bool>()
            .unwrap_or(false);
        let enable_symbol_matches_website = std::env::var("ENABLE_SYMBOL_MATCHES_WEBSITE")
            .unwrap_or_else(|_| "false".to_string())
            .parse::<bool>()
            .unwrap_or(false);
        let enable_name_matches_twitter = std::env::var("ENABLE_NAME_MATCHES_TWITTER")
            .unwrap_or_else(|_| "false".to_string())
            .parse::<bool>()
            .unwrap_or(false);
        let enable_symbol_matches_twitter = std::env::var("ENABLE_SYMBOL_MATCHES_TWITTER")
            .unwrap_or_else(|_| "false".to_string())
            .parse::<bool>()
            .unwrap_or(false);
        // Advanced filters - Combined brand matching
        let enable_name_matches_both = std::env::var("ENABLE_NAME_MATCHES_BOTH")
            .unwrap_or_else(|_| "false".to_string())
            .parse::<bool>()
            .unwrap_or(false);
        let enable_symbol_matches_both = std::env::var("ENABLE_SYMBOL_MATCHES_BOTH")
            .unwrap_or_else(|_| "false".to_string())
            .parse::<bool>()
            .unwrap_or(false);
        let enable_twitter_and_name_match_website = std::env::var("ENABLE_TWITTER_AND_NAME_MATCH_WEBSITE")
            .unwrap_or_else(|_| "false".to_string())
            .parse::<bool>()
            .unwrap_or(false);
        let enable_twitter_and_symbol_match_website = std::env::var("ENABLE_TWITTER_AND_SYMBOL_MATCH_WEBSITE")
            .unwrap_or_else(|_| "false".to_string())
            .parse::<bool>()
            .unwrap_or(false);
        let enable_name_and_symbol_match_twitter = std::env::var("ENABLE_NAME_AND_SYMBOL_MATCH_TWITTER")
            .unwrap_or_else(|_| "false".to_string())
            .parse::<bool>()
            .unwrap_or(false);
        let enable_brand_match_and_twitter = std::env::var("ENABLE_BRAND_MATCH_AND_TWITTER")
            .unwrap_or_else(|_| "false".to_string())
            .parse::<bool>()
            .unwrap_or(false);
        let enable_brand_match_and_website = std::env::var("ENABLE_BRAND_MATCH_AND_WEBSITE")
            .unwrap_or_else(|_| "false".to_string())
            .parse::<bool>()
            .unwrap_or(false);
        let enable_brand_match_and_twitter_and_website = std::env::var("ENABLE_BRAND_MATCH_AND_TWITTER_AND_WEBSITE")
            .unwrap_or_else(|_| "false".to_string())
            .parse::<bool>()
            .unwrap_or(false);
        let enable_perfect_brand_and_twitter = std::env::var("ENABLE_PERFECT_BRAND_AND_TWITTER")
            .unwrap_or_else(|_| "false".to_string())
            .parse::<bool>()
            .unwrap_or(false);
        let enable_perfect_brand_and_website = std::env::var("ENABLE_PERFECT_BRAND_AND_WEBSITE")
            .unwrap_or_else(|_| "false".to_string())
            .parse::<bool>()
            .unwrap_or(false);
        let enable_perfect_brand_and_twitter_community = std::env::var("ENABLE_PERFECT_BRAND_AND_TWITTER_COMMUNITY")
            .unwrap_or_else(|_| "false".to_string())
            .parse::<bool>()
            .unwrap_or(false);
        let enable_brand_score_3_plus_and_com = std::env::var("ENABLE_BRAND_SCORE_3_PLUS_AND_COM")
            .unwrap_or_else(|_| "false".to_string())
            .parse::<bool>()
            .unwrap_or(false);
        let enable_brand_score_4_plus_and_com = std::env::var("ENABLE_BRAND_SCORE_4_PLUS_AND_COM")
            .unwrap_or_else(|_| "false".to_string())
            .parse::<bool>()
            .unwrap_or(false);
        let enable_twitter_match_website_and_com = std::env::var("ENABLE_TWITTER_MATCH_WEBSITE_AND_COM")
            .unwrap_or_else(|_| "false".to_string())
            .parse::<bool>()
            .unwrap_or(false);
        let enable_name_match_website_and_com = std::env::var("ENABLE_NAME_MATCH_WEBSITE_AND_COM")
            .unwrap_or_else(|_| "false".to_string())
            .parse::<bool>()
            .unwrap_or(false);

        // Socials fetch configuration
        let socials_das_timeout_ms = std::env::var("SOCIALS_DAS_TIMEOUT_MS")
            .unwrap_or_else(|_| "2000".to_string())
            .parse::<u64>()
            .unwrap_or(2000);
        
        let socials_ipfs_timeout_ms = std::env::var("SOCIALS_IPFS_TIMEOUT_MS")
            .unwrap_or_else(|_| "5000".to_string())
            .parse::<u64>()
            .unwrap_or(5000);
        
        let socials_total_timeout_ms = std::env::var("SOCIALS_TOTAL_TIMEOUT_MS")
            .unwrap_or_else(|_| "5000".to_string())
            .parse::<u64>()
            .unwrap_or(5000);
        
        let socials_max_retries = std::env::var("SOCIALS_MAX_RETRIES")
            .unwrap_or_else(|_| "2".to_string())
            .parse::<u32>()
            .unwrap_or(2);
        
        let socials_retry_delay_ms = std::env::var("SOCIALS_RETRY_DELAY_MS")
            .unwrap_or_else(|_| "200".to_string())
            .parse::<u64>()
            .unwrap_or(200);
        
        let socials_max_concurrent = std::env::var("SOCIALS_MAX_CONCURRENT")
            .unwrap_or_else(|_| "10".to_string())
            .parse::<usize>()
            .unwrap_or(10);

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
            buy_amount_sol,
            slippage_percent,
            priority_fee,
            enable_dynamic_priority_fee,
            compute_units,
            one_shot_mode,
            submission_mode,
            jito_tip: (jito_tip * 1e9) as u64, // Convert SOL to lamports
            require_socials,
            require_twitter,
            require_website,
            require_telegram,
            require_discord,
            min_socials_count,
            enable_min_socials_count,
            min_dev_buy_sol,
            max_dev_buy_sol,
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
            take_profit_mc_sol,
            sell_percent,
            monitor_interval_sec,
            enable_dead_coin_sell,
            dead_coin_timeout_sec,
            blacklisted_tokens,
            blacklisted_creators,
            whitelisted_tokens,
            require_uppercase_token,
            max_name_length,
            min_ticker_length,
            max_ticker_length,
            // Advanced filters - Basic
            enable_has_twitter,
            enable_has_telegram,
            enable_has_website,
            enable_social_count_1_plus,
            enable_social_count_2_plus,
            enable_social_count_3,
            enable_website_com,
            enable_website_org,
            enable_website_xyz,
            enable_uppercase,
            enable_lowercase,
            enable_symbol_3_4,
            enable_symbol_3_6,
            enable_symbol_3_7,
            enable_name_short,
            enable_name_medium,
            // Advanced filters - Twitter types
            enable_twitter_account,
            enable_twitter_community,
            enable_twitter_status,
            enable_twitter_no_status,
            enable_twitter_account_or_community,
            enable_twitter_username_length_short,
            enable_twitter_username_length_medium,
            enable_has_twitter_with_username,
            // Advanced filters - Brand matching
            enable_has_brand_match,
            enable_brand_score_2_plus,
            enable_brand_score_3_plus,
            enable_brand_score_4_plus,
            enable_perfect_brand_match,
            enable_twitter_matches_website,
            enable_name_matches_website,
            enable_symbol_matches_website,
            enable_name_matches_twitter,
            enable_symbol_matches_twitter,
            // Advanced filters - Combined brand matching
            enable_name_matches_both,
            enable_symbol_matches_both,
            enable_twitter_and_name_match_website,
            enable_twitter_and_symbol_match_website,
            enable_name_and_symbol_match_twitter,
            enable_brand_match_and_twitter,
            enable_brand_match_and_website,
            enable_brand_match_and_twitter_and_website,
            enable_perfect_brand_and_twitter,
            enable_perfect_brand_and_website,
            enable_perfect_brand_and_twitter_community,
            enable_brand_score_3_plus_and_com,
            enable_brand_score_4_plus_and_com,
            enable_twitter_match_website_and_com,
            enable_name_match_website_and_com,
            // Socials fetch configuration
            socials_das_timeout_ms,
            socials_ipfs_timeout_ms,
            socials_total_timeout_ms,
            socials_max_retries,
            socials_retry_delay_ms,
            socials_max_concurrent,
        };

        config.validate()?;
        Ok(config)
    }

    /// Validate configuration values
    pub fn validate(&self) -> Result<()> {
        if self.buy_amount_sol <= 0.0 {
            return Err(anyhow!("BUY_AMOUNT_SOL must be > 0"));
        }

        if self.min_dev_buy_sol >= self.max_dev_buy_sol {
            return Err(anyhow!("MIN_DEV_BUY_SOL must be < MAX_DEV_BUY_SOL"));
        }

        if self.min_dev_tokens > self.max_dev_tokens {
            return Err(anyhow!("MIN_DEV_TOKENS must be <= MAX_DEV_TOKENS"));
        }

        if self.min_ticker_length > self.max_ticker_length {
            return Err(anyhow!("MIN_TICKER_LENGTH must be <= MAX_TICKER_LENGTH"));
        }

        if self.compute_units == 0 {
            return Err(anyhow!("COMPUTE_UNITS must be > 0"));
        }

        // Validate auto-sell options only if auto-sell is enabled
        if self.enable_auto_sell {
            if self.stop_loss_percent < 0.0 || self.stop_loss_percent > 100.0 {
                return Err(anyhow!("STOP_LOSS_PERCENT must be between 0 and 100"));
            }

            if self.take_profit_mc_sol <= 0.0 {
                return Err(anyhow!("TAKE_PROFIT_MC_SOL must be > 0"));
            }

            if self.sell_percent <= 0.0 || self.sell_percent > 100.0 {
                return Err(anyhow!("SELL_PERCENT must be between 0 and 100"));
            }

            if self.monitor_interval_sec == 0 {
                return Err(anyhow!("MONITOR_INTERVAL_SEC must be > 0"));
            }
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
            buy_amount_sol: 0.015,
            slippage_percent: 200, // Default 200% (100% slippage tolerance)
            priority_fee: 11_000_000,
            enable_dynamic_priority_fee: false,
            compute_units: 200_000,
            one_shot_mode: false,
            submission_mode: SubmissionMode::Helius,
            jito_tip: 1_500_000,
            require_socials: false,
            require_twitter: false,
            require_website: false,
            require_telegram: false,
            require_discord: false,
            min_socials_count: 0,
            enable_min_socials_count: false,
            min_dev_buy_sol: 0.73, // ~100 USD at 137 SOL/USD
            max_dev_buy_sol: 7.3, // ~1000 USD at 137 SOL/USD
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
            take_profit_mc_sol: 175.0, // ~24000 USD at 137 SOL/USD
            sell_percent: 100.0,
            monitor_interval_sec: 5,
            enable_dead_coin_sell: false,
            dead_coin_timeout_sec: 15,
            blacklisted_tokens: HashSet::new(),
            blacklisted_creators: HashSet::new(),
            whitelisted_tokens: None,
            require_uppercase_token: false,
            max_name_length: 20,
            min_ticker_length: 3,
            max_ticker_length: 7,
            // Advanced filters - Basic
            enable_has_twitter: false,
            enable_has_telegram: false,
            enable_has_website: false,
            enable_social_count_1_plus: false,
            enable_social_count_2_plus: false,
            enable_social_count_3: false,
            enable_website_com: false,
            enable_website_org: false,
            enable_website_xyz: false,
            enable_uppercase: false,
            enable_lowercase: false,
            enable_symbol_3_4: false,
            enable_symbol_3_6: false,
            enable_symbol_3_7: false,
            enable_name_short: false,
            enable_name_medium: false,
            // Advanced filters - Twitter types
            enable_twitter_account: false,
            enable_twitter_community: false,
            enable_twitter_status: false,
            enable_twitter_no_status: false,
            enable_twitter_account_or_community: false,
            enable_twitter_username_length_short: false,
            enable_twitter_username_length_medium: false,
            enable_has_twitter_with_username: false,
            // Advanced filters - Brand matching
            enable_has_brand_match: false,
            enable_brand_score_2_plus: false,
            enable_brand_score_3_plus: false,
            enable_brand_score_4_plus: false,
            enable_perfect_brand_match: false,
            enable_twitter_matches_website: false,
            enable_name_matches_website: false,
            enable_symbol_matches_website: false,
            enable_name_matches_twitter: false,
            enable_symbol_matches_twitter: false,
            // Advanced filters - Combined brand matching
            enable_name_matches_both: false,
            enable_symbol_matches_both: false,
            enable_twitter_and_name_match_website: false,
            enable_twitter_and_symbol_match_website: false,
            enable_name_and_symbol_match_twitter: false,
            enable_brand_match_and_twitter: false,
            enable_brand_match_and_website: false,
            enable_brand_match_and_twitter_and_website: false,
            enable_perfect_brand_and_twitter: false,
            enable_perfect_brand_and_website: false,
            enable_perfect_brand_and_twitter_community: false,
            enable_brand_score_3_plus_and_com: false,
            enable_brand_score_4_plus_and_com: false,
            enable_twitter_match_website_and_com: false,
            enable_name_match_website_and_com: false,
            // Socials fetch configuration
            socials_das_timeout_ms: 2000,
            socials_ipfs_timeout_ms: 5000,
            socials_total_timeout_ms: 5000,
            socials_max_retries: 2,
            socials_retry_delay_ms: 200,
            socials_max_concurrent: 10,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use once_cell::sync::Lazy;
    use std::sync::Mutex;

    // Tests in this module mutate process-wide environment variables.
    // Serialize them to avoid flaky cross-test interference.
    static ENV_LOCK: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

    fn setup_test_env() {
        // Set valid test values (override any existing values)
        env::set_var("HELIUS_API_KEY", "test-api-key-123");
        env::set_var("BUY_AMOUNT_SOL", "0.02");
        env::set_var("PRIORITY_FEE", "5000000");
        env::set_var("COMPUTE_UNITS", "300000");
    }

    fn cleanup_test_env() {
        env::remove_var("HELIUS_API_KEY");
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
        env::remove_var("MIN_DEV_BUY_SOL");
        env::remove_var("MAX_DEV_BUY_SOL");
        env::remove_var("MIN_DEV_TOKENS");
        env::remove_var("MAX_DEV_TOKENS");
        env::remove_var("ENABLE_TRACKER");
        env::remove_var("MOCK_BUY");
    }

    #[test]
    fn test_config_from_env() {
        let _lock = ENV_LOCK.lock().unwrap();
        setup_test_env();
        
        let config = Config::from_env();
        assert!(config.is_ok());
        
        let config = config.unwrap();
        assert_eq!(config.helius_api_key, "test-api-key-123");
        assert_eq!(config.buy_amount_sol, 0.02);
        assert_eq!(config.priority_fee, 5000000);
        assert_eq!(config.compute_units, 300000);
        
        cleanup_test_env();
    }

    #[test]
    fn test_config_validation() {
        let _lock = ENV_LOCK.lock().unwrap();
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
        config.min_dev_buy_sol = 12.0;
        config.max_dev_buy_sol = 5.0;
        assert!(config.validate().is_err());
        
        // Reset
        config.min_dev_buy_sol = 5.0;
        config.max_dev_buy_sol = 12.0;
        
        // Invalid: compute_units == 0
        config.compute_units = 0;
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_config_defaults() {
        let _lock = ENV_LOCK.lock().unwrap();
        let config = Config::default();
        
        assert_eq!(config.buy_amount_sol, 0.015);
        assert_eq!(config.priority_fee, 11_000_000);
        assert_eq!(config.compute_units, 200_000);
        assert_eq!(config.submission_mode, SubmissionMode::Helius);
        assert!(!config.require_socials);
        assert!(!config.require_twitter);
    }

    #[test]
    fn test_config_submission_modes() {
        let _lock = ENV_LOCK.lock().unwrap();
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
        let _lock = ENV_LOCK.lock().unwrap();
        let config = Config::default();
        
        // Test buy_amount_lamports
        assert_eq!(config.buy_amount_lamports(), 15_000_000); // 0.015 SOL
        
        // Test min/max dev buy in SOL
        assert!(config.min_dev_buy_sol > 0.0);
        assert!(config.max_dev_buy_sol > config.min_dev_buy_sol);
        
        // Test RPC client creation
        let rpc = config.create_rpc_client();
        assert!(!rpc.url().is_empty());
    }

    #[test]
    fn test_config_missing_api_key() {
        let _lock = ENV_LOCK.lock().unwrap();
        cleanup_test_env();
        // Remove API key if it exists
        env::remove_var("HELIUS_API_KEY");
        
        // Config should use default API key when HELIUS_API_KEY is missing
        let config = Config::from_env();
        assert!(config.is_ok(), "Config should succeed with default API key");
        let config = config.unwrap();
        // Should use default API key value
        assert_eq!(config.helius_api_key, "7ef7af02-aa9d-4f5c-9c98-d5fa303d1f04");
    }

    #[test]
    fn test_config_invalid_numbers() {
        let _lock = ENV_LOCK.lock().unwrap();
        // Save original values
        let orig_buy_amount = env::var("BUY_AMOUNT_SOL").ok();
        let orig_api_key = env::var("HELIUS_API_KEY").ok();
        
        // Ensure we have API key for other validations to pass
        env::set_var("HELIUS_API_KEY", "test-key");
        
        // Invalid BUY_AMOUNT_SOL
        env::set_var("BUY_AMOUNT_SOL", "invalid");
        let config = Config::from_env();
        assert!(config.is_err(), "Expected error for invalid BUY_AMOUNT_SOL, got: {:?}", config);
        
        // Restore original values
        if let Some(val) = orig_buy_amount {
            env::set_var("BUY_AMOUNT_SOL", val);
        } else {
            env::remove_var("BUY_AMOUNT_SOL");
        }
        if let Some(val) = orig_api_key {
            env::set_var("HELIUS_API_KEY", val);
        } else {
            env::remove_var("HELIUS_API_KEY");
        }
    }

    #[test]
    fn test_target_mint_address_loading() {
        let _lock = ENV_LOCK.lock().unwrap();
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
        let _lock = ENV_LOCK.lock().unwrap();
        let config = Config::default();
        assert!(config.target_mint_address.is_none());
    }
}

