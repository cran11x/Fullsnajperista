// mock_buy_test.rs - Tests for mock buy functionality

use Fullsnajperista::config::Config;

#[test]
fn test_mock_buy_default_false() {
    let config = Config::default();
    assert_eq!(config.mock_buy, false, "Mock buy should default to false");
}

#[test]
fn test_mock_buy_from_env() {
    // Save original values
    let orig_mock_buy = std::env::var("MOCK_BUY").ok();
    let orig_api_key = std::env::var("HELIUS_API_KEY").ok();
    
    // Set required HELIUS_API_KEY for Config::from_env()
    std::env::set_var("HELIUS_API_KEY", "test-api-key-for-mock-buy-test");
    
    // Test with MOCK_BUY=true
    std::env::set_var("MOCK_BUY", "true");
    let config = Config::from_env().unwrap();
    assert_eq!(config.mock_buy, true, "Mock buy should be true when MOCK_BUY=true");
    
    // Test with MOCK_BUY=false
    std::env::set_var("MOCK_BUY", "false");
    let config = Config::from_env().unwrap();
    assert_eq!(config.mock_buy, false, "Mock buy should be false when MOCK_BUY=false");
    
    // Cleanup
    if let Some(val) = orig_mock_buy {
        std::env::set_var("MOCK_BUY", val);
    } else {
        std::env::remove_var("MOCK_BUY");
    }
    
    if let Some(val) = orig_api_key {
        std::env::set_var("HELIUS_API_KEY", val);
    } else {
        std::env::remove_var("HELIUS_API_KEY");
    }
}

#[test]
fn test_mock_buy_config_clone() {
    let mut config = Config::default();
    config.mock_buy = true;
    
    let cloned = config.clone();
    assert_eq!(cloned.mock_buy, true, "Cloned config should preserve mock_buy value");
    
    config.mock_buy = false;
    assert_eq!(config.mock_buy, false, "Original config should be updated");
    assert_eq!(cloned.mock_buy, true, "Cloned config should remain unchanged");
}

#[test]
fn test_mock_buy_validation() {
    let mut config = Config::default();
    config.mock_buy = true;
    
    // Mock buy should not affect validation
    assert!(config.validate().is_ok(), "Config with mock_buy=true should validate");
    
    config.mock_buy = false;
    assert!(config.validate().is_ok(), "Config with mock_buy=false should validate");
}

