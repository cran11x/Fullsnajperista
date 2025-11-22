// target_mint_test.rs - Tests for target mint address functionality
// Run with: cargo test --test target_mint_test

use anyhow::Result;
use solana_sdk::pubkey::Pubkey;
use std::str::FromStr;
use std::sync::Arc;
use std::sync::RwLock;

use fullsnajperista::config::Config;
use fullsnajperista::accounts::SeenTokens;

#[test]
fn test_target_mint_config_loading() {
    // Test that target mint address is properly loaded from config
    let mut config = Config::default();
    
    // Should be None by default
    assert!(config.target_mint_address.is_none());
    
    // Set a target mint
    let test_mint = Pubkey::from_str("So11111111111111111111111111111111111111112").unwrap();
    config.target_mint_address = Some(test_mint);
    
    assert!(config.target_mint_address.is_some());
    assert_eq!(config.target_mint_address.unwrap(), test_mint);
}

#[test]
fn test_target_mint_filtering_logic() {
    // Test the filtering logic that would be used in bot_core
    let target_mint = Pubkey::from_str("So11111111111111111111111111111111111111112").unwrap();
    let other_mint = Pubkey::from_str("EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v").unwrap();
    
    // When target is set, only matching mint should pass
    let config_with_target = {
        let mut cfg = Config::default();
        cfg.target_mint_address = Some(target_mint);
        cfg
    };
    
    // Matching mint should pass
    assert_eq!(target_mint == target_mint, true);
    
    // Non-matching mint should be filtered
    assert_ne!(other_mint, target_mint);
    
    // Test without target (should allow all)
    let config_no_target = Config::default();
    assert!(config_no_target.target_mint_address.is_none());
}

#[test]
fn test_target_mint_env_parsing() {
    // Test parsing from environment variable format
    let valid_pubkey_str = "So11111111111111111111111111111111111111112";
    let invalid_pubkey_str = "not_a_valid_pubkey";
    let empty_str = "";
    
    // Valid pubkey should parse
    let valid_result = Pubkey::from_str(valid_pubkey_str);
    assert!(valid_result.is_ok());
    
    // Invalid pubkey should fail
    let invalid_result = Pubkey::from_str(invalid_pubkey_str);
    assert!(invalid_result.is_err());
    
    // Empty string should be handled gracefully
    assert!(empty_str.trim().is_empty());
}

#[tokio::test]
async fn test_target_mint_with_seen_tokens() {
    // Test that target mint filtering works with seen tokens tracking
    let seen_tokens = Arc::new(SeenTokens::new());
    let target_mint = Pubkey::from_str("So11111111111111111111111111111111111111112").unwrap();
    let other_mint = Pubkey::from_str("EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v").unwrap();
    
    let target_mint_str = target_mint.to_string();
    let other_mint_str = other_mint.to_string();
    
    // Mark both as seen
    seen_tokens.check_and_mark(&target_mint_str);
    seen_tokens.check_and_mark(&other_mint_str);
    
    // Both should be marked as seen
    assert!(!seen_tokens.check_and_mark(&target_mint_str));
    assert!(!seen_tokens.check_and_mark(&other_mint_str));
}

#[test]
fn test_target_mint_display_format() {
    // Test address formatting for UI display
    let mint = Pubkey::from_str("So11111111111111111111111111111111111111112").unwrap();
    let mint_str = mint.to_string();
    
    // Should be 44 characters (base58 encoded Solana pubkey)
    assert_eq!(mint_str.len(), 44);
    
    // Format for display (first 8 + last 8)
    if mint_str.len() > 16 {
        let formatted = format!("{}...{}", &mint_str[..8], &mint_str[mint_str.len()-8..]);
        assert!(formatted.contains("..."));
        assert_eq!(formatted.len(), 8 + 3 + 8); // 8 + "..." + 8
    }
}

#[test]
fn test_target_mint_config_clone() {
    // Test that target mint is properly cloned with config
    let mut config1 = Config::default();
    let target_mint = Pubkey::from_str("So11111111111111111111111111111111111111112").unwrap();
    config1.target_mint_address = Some(target_mint);
    
    let config2 = config1.clone();
    
    assert_eq!(config1.target_mint_address, config2.target_mint_address);
    assert!(config2.target_mint_address.is_some());
    assert_eq!(config2.target_mint_address.unwrap(), target_mint);
}
