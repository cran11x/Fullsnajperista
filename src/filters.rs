// filters.rs - TOKEN FILTERING LOGIC
#![allow(unused_imports, dead_code)]

use anyhow::Result;
use solana_sdk::pubkey::Pubkey;
use crate::detection::PumpBuyAccounts;
use crate::socials::Socials;
use crate::config::Config;

/// Check if token passes all filters
pub fn should_process_token(
    config: &Config,
    accounts: &PumpBuyAccounts,
    socials: Option<&Socials>,
) -> Result<bool> {
    // Dev buy filter
    let dev_buy_sol = accounts.dev_buy_sol as f64 / 1e9;
    let min_sol = config.min_dev_buy_sol();
    let max_sol = config.max_dev_buy_sol();

    if dev_buy_sol < min_sol || dev_buy_sol > max_sol {
        return Ok(false);
    }

    // Social filters
    if let Some(socials) = socials {
        if config.require_socials && !socials.has_any() {
            return Ok(false);
        }

        if config.require_twitter && !socials.has_twitter() {
            return Ok(false);
        }

        if socials.count() < config.min_socials_count {
            return Ok(false);
        }
    } else if config.require_socials || config.require_twitter || config.min_socials_count > 0 {
        // Socials required but not available
        return Ok(false);
    }

    Ok(true)
}

/// Check creator token count filter
pub fn check_creator_token_count(
    count: u32,
    config: &Config,
) -> bool {
    count >= config.min_dev_tokens as u32 && count <= config.max_dev_tokens as u32
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::socials::Socials;

    fn create_test_config() -> Config {
        Config::default()
    }

    fn create_test_accounts(dev_buy_sol: f64) -> PumpBuyAccounts {
        PumpBuyAccounts {
            mint: Pubkey::new_unique(),
            bonding_curve: Pubkey::new_unique(),
            associated_bonding_curve: Pubkey::new_unique(),
            creator_vault: Pubkey::new_unique(),
            event_authority: Pubkey::new_unique(),
            global_volume: Pubkey::new_unique(),
            global: Pubkey::new_unique(),
            fee_recipient: Pubkey::new_unique(),
            fee_config: Pubkey::new_unique(),
            fee_program: Pubkey::new_unique(),
            dev_buy_sol: (dev_buy_sol * 1e9) as u64,
            creator: Pubkey::new_unique(),
            associated_bonding_curve_instruction: None,
        }
    }

    #[test]
    fn test_should_process_token_dev_buy_filter() {
        let config = create_test_config();
        
        // Valid dev buy: 0.8 SOL = 0.8 * 162 = 129.6 USD (outside 500-1200 range)
        // Need to use value in range: 500/162 = 3.09 SOL to 1200/162 = 7.41 SOL
        let accounts = create_test_accounts(5.0); // 5 SOL = 810 USD (within range)
        assert!(should_process_token(&config, &accounts, None).unwrap());

        // Too low
        let accounts_low = create_test_accounts(0.001);
        assert!(!should_process_token(&config, &accounts_low, None).unwrap());

        // Too high
        let accounts_high = create_test_accounts(10.0);
        assert!(!should_process_token(&config, &accounts_high, None).unwrap());
    }

    #[test]
    fn test_should_process_token_social_filters() {
        let mut config = create_test_config();
        config.require_socials = true;

        let accounts = create_test_accounts(5.0); // Valid dev buy amount
        
        // No socials - should fail
        assert!(!should_process_token(&config, &accounts, None).unwrap());

        // With socials - should pass
        let socials = Socials {
            twitter: Some("https://x.com/test".to_string()),
            website: None,
            telegram: None,
            discord: None,
        };
        assert!(should_process_token(&config, &accounts, Some(&socials)).unwrap());
    }

    #[test]
    fn test_should_process_token_twitter_filter() {
        let mut config = create_test_config();
        config.require_twitter = true;

        let accounts = create_test_accounts(5.0); // Valid dev buy amount
        
        // No twitter - should fail
        let socials_no_twitter = Socials {
            twitter: None,
            website: Some("https://example.com".to_string()),
            telegram: None,
            discord: None,
        };
        assert!(!should_process_token(&config, &accounts, Some(&socials_no_twitter)).unwrap());

        // With twitter - should pass
        let socials_with_twitter = Socials {
            twitter: Some("https://x.com/test".to_string()),
            website: None,
            telegram: None,
            discord: None,
        };
        assert!(should_process_token(&config, &accounts, Some(&socials_with_twitter)).unwrap());
    }

    #[test]
    fn test_should_process_token_min_socials_count() {
        let mut config = create_test_config();
        config.min_socials_count = 2;

        let accounts = create_test_accounts(5.0); // Valid dev buy amount
        
        // Only 1 social - should fail
        let socials_one = Socials {
            twitter: Some("https://x.com/test".to_string()),
            website: None,
            telegram: None,
            discord: None,
        };
        assert!(!should_process_token(&config, &accounts, Some(&socials_one)).unwrap());

        // 2+ socials - should pass
        let socials_two = Socials {
            twitter: Some("https://x.com/test".to_string()),
            website: Some("https://example.com".to_string()),
            telegram: None,
            discord: None,
        };
        assert!(should_process_token(&config, &accounts, Some(&socials_two)).unwrap());
    }

    #[test]
    fn test_check_creator_token_count() {
        let mut config = create_test_config();
        config.min_dev_tokens = 6;
        config.max_dev_tokens = 10;
        
        // Valid count (within range)
        assert!(check_creator_token_count(7, &config), "7 should pass (within 6-10)");
        assert!(check_creator_token_count(6, &config), "6 should pass (min boundary)");
        assert!(check_creator_token_count(10, &config), "10 should pass (max boundary)");

        // Too low
        assert!(!check_creator_token_count(5, &config), "5 should fail (below min 6)");
        assert!(!check_creator_token_count(0, &config), "0 should fail (below min 6)");

        // Too high
        assert!(!check_creator_token_count(11, &config), "11 should fail (above max 10)");
        assert!(!check_creator_token_count(520, &config), "520 should fail (above max 10)");
        assert!(!check_creator_token_count(8500, &config), "8500 should fail (above max 10)");
    }

    #[test]
    fn test_check_creator_token_count_edge_cases() {
        let mut config = create_test_config();
        
        // Test with filter 0-10 (default)
        config.min_dev_tokens = 0;
        config.max_dev_tokens = 10;
        
        assert!(check_creator_token_count(0, &config), "0 should pass (min is 0)");
        assert!(check_creator_token_count(5, &config), "5 should pass");
        assert!(check_creator_token_count(10, &config), "10 should pass (max boundary)");
        assert!(!check_creator_token_count(11, &config), "11 should fail");
        assert!(!check_creator_token_count(520, &config), "520 should fail");
        
        // Test with filter 1-5
        config.min_dev_tokens = 1;
        config.max_dev_tokens = 5;
        
        assert!(!check_creator_token_count(0, &config), "0 should fail (below min 1)");
        assert!(check_creator_token_count(1, &config), "1 should pass");
        assert!(check_creator_token_count(3, &config), "3 should pass");
        assert!(check_creator_token_count(5, &config), "5 should pass");
        assert!(!check_creator_token_count(6, &config), "6 should fail (above max 5)");
    }

    #[test]
    fn test_filter_logic_simulation() {
        // Simulate the filter logic from bot_core.rs
        let mut config = create_test_config();
        config.min_dev_tokens = 0;
        config.max_dev_tokens = 10;
        
        // Simulate DAS API returning different counts
        let test_cases = vec![
            (0u32, true, "0 tokens should pass (min is 0)"),
            (5u32, true, "5 tokens should pass"),
            (10u32, true, "10 tokens should pass (max boundary)"),
            (11u32, false, "11 tokens should fail (above max)"),
            (520u32, false, "520 tokens should fail (way above max)"),
            (8500u32, false, "8500 tokens should fail (way above max)"),
        ];
        
        for (count, should_pass, msg) in test_cases {
            let passes = count >= config.min_dev_tokens as u32 && count <= config.max_dev_tokens as u32;
            assert_eq!(passes, should_pass, "{}: count={}, expected={}", msg, count, should_pass);
        }
    }
}

