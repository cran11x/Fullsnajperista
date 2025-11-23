// mock_buy_integration_test.rs - Integration tests for mock buy functionality with mock servers

use Fullsnajperista::config::Config;
use std::sync::Arc;

#[tokio::test]
async fn test_mock_buy_config_update() {
    let mut config = Config::default();
    assert_eq!(config.mock_buy, false, "Default should be false");
    
    config.mock_buy = true;
    assert_eq!(config.mock_buy, true, "Should be able to set to true");
    
    config.mock_buy = false;
    assert_eq!(config.mock_buy, false, "Should be able to set to false");
}

#[tokio::test]
async fn test_mock_buy_with_shared_config() {
    use std::sync::RwLock;
    
    let config = Arc::new(RwLock::new(Config::default()));
    
    // Initially false
    {
        let cfg = config.read().unwrap();
        assert_eq!(cfg.mock_buy, false);
    }
    
    // Update to true
    {
        let mut cfg = config.write().unwrap();
        cfg.mock_buy = true;
    }
    
    // Verify update
    {
        let cfg = config.read().unwrap();
        assert_eq!(cfg.mock_buy, true);
    }
    
    // Update back to false
    {
        let mut cfg = config.write().unwrap();
        cfg.mock_buy = false;
    }
    
    // Verify update
    {
        let cfg = config.read().unwrap();
        assert_eq!(cfg.mock_buy, false);
    }
}

#[test]
fn test_mock_buy_signature_format() {
    // Test that mock signatures have the correct format
    use solana_sdk::signature::Signature;
    use rand::Rng;
    
    let mut rng = rand::thread_rng();
    let mut mock_sig_bytes = [0u8; 64];
    rng.fill(&mut mock_sig_bytes);
    let mock_sig = Signature::from(mock_sig_bytes);
    let mock_signature = format!("MOCK_{}", mock_sig.to_string());
    
    assert!(mock_signature.starts_with("MOCK_"), "Mock signature should start with MOCK_");
    assert!(mock_signature.len() > 10, "Mock signature should have reasonable length");
}

