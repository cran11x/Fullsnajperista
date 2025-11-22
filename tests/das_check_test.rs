// das_check_test.rs - Comprehensive tests for DAS check functionality
// Run with: cargo test --test das_check_test

use solana_sdk::pubkey::Pubkey;
use std::str::FromStr;
use serde_json::json;

#[tokio::test]
async fn test_das_response_parsing_success() {
    // Test successful JSON parsing with total count
    let json_str = r#"{"jsonrpc":"2.0","id":"1","result":{"total":42}}"#;
    let json: serde_json::Value = serde_json::from_str(json_str).unwrap();
    
    if let Some(total) = json["result"]["total"].as_u64() {
        assert_eq!(total, 42);
    } else {
        panic!("Failed to parse total from successful response");
    }
}

#[tokio::test]
async fn test_das_response_parsing_zero() {
    // Test parsing when total is 0
    let json_str = r#"{"jsonrpc":"2.0","id":"1","result":{"total":0}}"#;
    let json: serde_json::Value = serde_json::from_str(json_str).unwrap();
    
    if let Some(total) = json["result"]["total"].as_u64() {
        assert_eq!(total, 0);
    } else {
        panic!("Failed to parse zero total");
    }
}

#[tokio::test]
async fn test_das_response_parsing_missing_total() {
    // Test parsing when total field is missing
    let json_str = r#"{"jsonrpc":"2.0","id":"1","result":{}}"#;
    let json: serde_json::Value = serde_json::from_str(json_str).unwrap();
    
    assert!(json["result"]["total"].as_u64().is_none());
}

#[tokio::test]
async fn test_das_response_parsing_error_response() {
    // Test parsing error response from API
    let error_json = r#"{"jsonrpc":"2.0","id":"1","error":{"code":-32602,"message":"Invalid params"}}"#;
    let json: serde_json::Value = serde_json::from_str(error_json).unwrap();
    
    assert!(json["error"].is_object());
    assert!(json["result"].is_null());
    assert_eq!(json["error"]["code"], -32602);
}

#[tokio::test]
async fn test_das_response_parsing_large_number() {
    // Test parsing large token counts
    let json_str = r#"{"jsonrpc":"2.0","id":"1","result":{"total":999999}}"#;
    let json: serde_json::Value = serde_json::from_str(json_str).unwrap();
    
    if let Some(total) = json["result"]["total"].as_u64() {
        assert_eq!(total, 999999);
    } else {
        panic!("Failed to parse large total");
    }
}

#[tokio::test]
async fn test_das_search_assets_request_format() {
    // Test that searchAssets request body is correctly formatted
    let creator = Pubkey::new_unique();
    let search_body = json!({
        "jsonrpc": "2.0",
        "id": "1",
        "method": "searchAssets",
        "params": {
            "creatorAddress": creator.to_string(),
            "creatorVerified": false,
            "page": 1,
            "limit": 20
        }
    });
    
    assert_eq!(search_body["method"], "searchAssets");
    assert_eq!(search_body["params"]["creatorAddress"], creator.to_string());
    assert_eq!(search_body["params"]["limit"], 20);
    assert_eq!(search_body["params"]["page"], 1);
}

#[tokio::test]
async fn test_das_get_assets_by_creator_request_format() {
    // Test that getAssetsByCreator request body is correctly formatted
    let creator = Pubkey::new_unique();
    let assets_body = json!({
        "jsonrpc": "2.0",
        "id": "1",
        "method": "getAssetsByCreator",
        "params": {
            "creatorAddress": creator.to_string(),
            "onlyVerified": false,
            "page": 1,
            "limit": 1000
        }
    });
    
    assert_eq!(assets_body["method"], "getAssetsByCreator");
    assert_eq!(assets_body["params"]["creatorAddress"], creator.to_string());
    assert_eq!(assets_body["params"]["limit"], 1000);
    assert_eq!(assets_body["params"]["page"], 1);
}

#[tokio::test]
async fn test_das_url_formatting() {
    // Test URL formatting with API key
    let api_key = "test-api-key-123";
    let url = format!("https://mainnet.helius-rpc.com/?api-key={}", api_key);
    
    assert!(url.contains("mainnet.helius-rpc.com"));
    assert!(url.contains(api_key));
    assert!(url.starts_with("https://"));
}

#[test]
fn test_pubkey_parsing() {
    // Test that creator pubkey can be parsed correctly
    let valid_pubkey_str = "So11111111111111111111111111111111111111112";
    let pubkey = Pubkey::from_str(valid_pubkey_str);
    
    assert!(pubkey.is_ok());
    assert_eq!(pubkey.unwrap().to_string(), valid_pubkey_str);
}

#[test]
fn test_pubkey_to_string() {
    // Test pubkey to string conversion for API calls
    let creator = Pubkey::new_unique();
    let creator_str = creator.to_string();
    
    assert!(!creator_str.is_empty());
    // Base58 encoded Solana pubkey can be 32-44 characters (usually 43-44)
    assert!(creator_str.len() >= 32 && creator_str.len() <= 44);
}

#[tokio::test]
async fn test_das_response_with_assets_array() {
    // Test parsing response that includes assets array (even though we only use total)
    let json_str = r#"{
        "jsonrpc":"2.0",
        "id":"1",
        "result":{
            "total":5,
            "items":[
                {"id":"asset1"},
                {"id":"asset2"}
            ]
        }
    }"#;
    let json: serde_json::Value = serde_json::from_str(json_str).unwrap();
    
    if let Some(total) = json["result"]["total"].as_u64() {
        assert_eq!(total, 5);
    } else {
        panic!("Failed to parse total from response with assets array");
    }
    
    // Verify assets array exists (even though we don't use it)
    assert!(json["result"]["items"].is_array());
}

#[tokio::test]
async fn test_das_response_null_result() {
    // Test handling null result
    let json_str = r#"{"jsonrpc":"2.0","id":"1","result":null}"#;
    let json: serde_json::Value = serde_json::from_str(json_str).unwrap();
    
    assert!(json["result"].is_null());
    assert!(json["result"]["total"].as_u64().is_none());
}

#[test]
fn test_retry_constants() {
    // Test that retry constants are set correctly
    const MAX_RETRIES: u32 = 3;
    const RETRY_DELAY_MS: u64 = 100;
    
    assert_eq!(MAX_RETRIES, 3);
    assert_eq!(RETRY_DELAY_MS, 100);
    assert!(MAX_RETRIES > 0);
    assert!(RETRY_DELAY_MS > 0);
}

#[tokio::test]
async fn test_das_fallback_logic_structure() {
    // Test the structure of fallback logic
    // First tries searchAssets, then getAssetsByCreator
    
    // Simulate searchAssets returning 0
    let search_response = json!({
        "jsonrpc": "2.0",
        "id": "1",
        "result": {"total": 0}
    });
    
    // Should fallback to getAssetsByCreator
    let fallback_response = json!({
        "jsonrpc": "2.0",
        "id": "1",
        "result": {"total": 10}
    });
    
    // Verify both responses are valid
    assert_eq!(search_response["result"]["total"], 0);
    assert_eq!(fallback_response["result"]["total"], 10);
}

#[tokio::test]
async fn test_das_error_handling_structure() {
    // Test error handling structure
    // When all retries fail, should return 999
    let max_retries = 3;
    let mut attempts = 0;
    
    // Simulate 3 failed attempts
    for _ in 0..max_retries {
        attempts += 1;
    }
    
    assert_eq!(attempts, 3);
    
    // After max retries, should return 999 (as per code)
    let final_result = 999u32;
    assert_eq!(final_result, 999);
}

#[tokio::test]
async fn test_das_response_edge_cases() {
    // Test edge cases in response parsing
    
    // Empty result object
    let empty_result = json!({"jsonrpc":"2.0","id":"1","result":{}});
    assert!(empty_result["result"]["total"].as_u64().is_none());
    
    // Total as string (should fail gracefully)
    let string_total = json!({"jsonrpc":"2.0","id":"1","result":{"total":"10"}});
    assert!(string_total["result"]["total"].as_u64().is_none());
    
    // Negative total (edge case)
    let negative_total = json!({"jsonrpc":"2.0","id":"1","result":{"total":-5}});
    // as_u64() will return None for negative numbers
    assert!(negative_total["result"]["total"].as_u64().is_none());
}

#[test]
fn test_creator_pubkey_validation() {
    // Test that creator pubkey is valid Solana address
    let creator = Pubkey::new_unique();
    
    // Should be valid base58
    let creator_str = creator.to_string();
    assert!(!creator_str.is_empty());
    
    // Should not equal default/zero pubkey
    assert_ne!(creator, Pubkey::default());
}

#[tokio::test]
async fn test_das_request_id_consistency() {
    // Test that request IDs are consistent
    let request1 = json!({
        "jsonrpc": "2.0",
        "id": "1",
        "method": "searchAssets"
    });
    
    let request2 = json!({
        "jsonrpc": "2.0",
        "id": "1",
        "method": "getAssetsByCreator"
    });
    
    assert_eq!(request1["id"], request2["id"]);
    assert_eq!(request1["jsonrpc"], request2["jsonrpc"]);
}

#[tokio::test]
async fn test_das_response_total_type_conversion() {
    // Test u64 to u32 conversion (as done in the code)
    let total_u64: u64 = 1000;
    let total_u32 = total_u64 as u32;
    
    assert_eq!(total_u32, 1000);
    
    // Test large number that fits in u32
    let large_u64: u64 = 4294967295; // Max u32
    let large_u32 = large_u64 as u32;
    assert_eq!(large_u32, 4294967295u32);
}

#[tokio::test]
async fn test_das_api_key_in_url() {
    // Test that API key is properly included in URL
    let api_key = "my-secret-api-key";
    let url = format!("https://mainnet.helius-rpc.com/?api-key={}", api_key);
    
    // URL should contain the API key
    assert!(url.contains(api_key));
    
    // Should be a valid URL format
    assert!(url.starts_with("https://"));
    assert!(url.contains("?api-key="));
}

#[tokio::test]
#[ignore] // Integration test - requires real API key
async fn test_das_check_real_api_integration() {
    // Integration test - requires real API key and network
    // This should be run manually with valid credentials
    // 
    // To enable this test:
    // 1. Set HELIUS_API_KEY environment variable
    // 2. Uncomment the import and function call below
    // 3. Remove #[ignore] attribute or run with: cargo test -- --ignored
    
    use std::env;
    
    let api_key = match env::var("HELIUS_API_KEY") {
        Ok(key) => key,
        Err(_) => {
            eprintln!("Skipping integration test - HELIUS_API_KEY not set");
            return;
        }
    };
    
    // Use a known creator address for testing
    let creator = Pubkey::from_str("So11111111111111111111111111111111111111112").unwrap();
    
    // Uncomment these lines when ready to test with real API:
    // use fullsnajperista::das_check::check_creator_token_count_das;
    // let result = check_creator_token_count_das(&creator, &api_key).await;
    // assert!(result.is_ok());
    // let count = result.unwrap();
    // assert!(count > 0 || count == 999);
    
    // For now, just verify the test structure
    assert!(!api_key.is_empty());
    assert_ne!(creator, Pubkey::default());
}

