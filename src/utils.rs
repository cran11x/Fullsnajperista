// utils.rs - RETRY LOGIC, HTTP CLIENT POOLING, AND HELPER FUNCTIONS
#![allow(unused, dead_code)]

use anyhow::Result;
use reqwest::Client;
use std::time::Duration;
use tokio::time::sleep;
use std::sync::OnceLock;

/// Retry a function with exponential backoff
/// 
/// # Arguments
/// * `f` - Async function to retry
/// * `max_attempts` - Maximum number of attempts
/// * `initial_delay_ms` - Initial delay in milliseconds
/// 
/// # Returns
/// Result from the function, or error after max attempts
pub async fn retry_with_backoff<F, Fut, T, E>(
    mut f: F,
    max_attempts: u32,
    initial_delay_ms: u64,
) -> Result<T>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T, E>>,
    E: std::fmt::Display,
{
    let mut last_error = None;

    for attempt in 1..=max_attempts {
        match f().await {
            Ok(val) => return Ok(val),
            Err(e) => {
                last_error = Some(e.to_string());
                if attempt < max_attempts {
                    let delay_ms = initial_delay_ms * (1 << (attempt - 1)); // Exponential backoff
                    sleep(Duration::from_millis(delay_ms)).await;
                }
            }
        }
    }

    Err(anyhow::anyhow!(
        "Failed after {} attempts: {}",
        max_attempts,
        last_error.unwrap_or_else(|| "Unknown error".to_string())
    ))
}

/// Create a shared HTTP client with connection pooling
pub fn create_http_client(timeout_secs: u64) -> Result<Client> {
    Ok(Client::builder()
        .timeout(Duration::from_secs(timeout_secs))
        .pool_idle_timeout(Duration::from_secs(90))
        .pool_max_idle_per_host(20)
        .build()?)
}

/// Create default HTTP client (10 second timeout)
pub fn create_default_http_client() -> Result<Client> {
    create_http_client(10)
}

/// Global shared HTTP client for better performance
static SHARED_HTTP_CLIENT: OnceLock<Client> = OnceLock::new();

/// Get or create shared HTTP client
pub fn get_shared_http_client() -> &'static Client {
    SHARED_HTTP_CLIENT.get_or_init(|| {
        create_http_client(10).expect("Failed to create shared HTTP client")
    })
}

/// Retry with fixed delay (no exponential backoff)
pub async fn retry_with_fixed_delay<F, Fut, T, E>(
    mut f: F,
    max_attempts: u32,
    delay_ms: u64,
) -> Result<T>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T, E>>,
    E: std::fmt::Display,
{
    let mut last_error = None;

    for attempt in 1..=max_attempts {
        match f().await {
            Ok(val) => return Ok(val),
            Err(e) => {
                last_error = Some(e.to_string());
                if attempt < max_attempts {
                    sleep(Duration::from_millis(delay_ms)).await;
                }
            }
        }
    }

    Err(anyhow::anyhow!(
        "Failed after {} attempts: {}",
        max_attempts,
        last_error.unwrap_or_else(|| "Unknown error".to_string())
    ))
}

/// Format duration for display
pub fn format_duration(duration: Duration) -> String {
    let secs = duration.as_secs();
    if secs < 60 {
        format!("{}s", secs)
    } else if secs < 3600 {
        format!("{}m {}s", secs / 60, secs % 60)
    } else {
        format!("{}h {}m {}s", secs / 3600, (secs % 3600) / 60, secs % 60)
    }
}

/// Clamp a value between min and max
pub fn clamp<T: PartialOrd>(value: T, min: T, max: T) -> T {
    if value < min {
        min
    } else if value > max {
        max
    } else {
        value
    }
}

/// Get actual transaction fee from signature
pub async fn get_transaction_fee(
    rpc: &solana_client::nonblocking::rpc_client::RpcClient,
    signature: &str,
) -> Result<u64> {
    use solana_sdk::signature::Signature;
    use std::str::FromStr;
    
    let sig = Signature::from_str(signature)?;
    
    // Fetch transaction with meta
    let tx = rpc.get_transaction_with_config(
        &sig,
        solana_client::rpc_config::RpcTransactionConfig {
            encoding: Some(solana_transaction_status::UiTransactionEncoding::Json),
            commitment: Some(solana_sdk::commitment_config::CommitmentConfig::confirmed()),
            max_supported_transaction_version: Some(0),
        },
    ).await?;
    
    // Extract fee from meta
    if let Some(meta) = tx.transaction.meta {
        Ok(meta.fee)
    } else {
        Err(anyhow::anyhow!("Transaction meta not found"))
    }
}

/// Get actual SOL balance change for an account from transaction
pub async fn get_transaction_balance_change(
    rpc: &solana_client::nonblocking::rpc_client::RpcClient,
    signature: &str,
    account: &solana_sdk::pubkey::Pubkey,
) -> Result<i64> {
    use solana_sdk::signature::Signature;
    use std::str::FromStr;
    
    let sig = Signature::from_str(signature)?;
    let account_str = account.to_string();
    
    // Fetch transaction with meta
    let tx = rpc.get_transaction_with_config(
        &sig,
        solana_client::rpc_config::RpcTransactionConfig {
            encoding: Some(solana_transaction_status::UiTransactionEncoding::Json),
            commitment: Some(solana_sdk::commitment_config::CommitmentConfig::confirmed()),
            max_supported_transaction_version: Some(0),
        },
    ).await?;
    
    if let Some(meta) = tx.transaction.meta {
        // We need to find the index of the account in the transaction
        let account_keys = match &tx.transaction.transaction {
            solana_transaction_status::EncodedTransaction::Json(ui_tx) => {
                match &ui_tx.message {
                    solana_transaction_status::UiMessage::Parsed(parsed) => {
                        parsed.account_keys.iter().map(|k| k.pubkey.clone()).collect::<Vec<_>>()
                    },
                    solana_transaction_status::UiMessage::Raw(raw) => {
                        raw.account_keys.clone()
                    }
                }
            },
            _ => return Err(anyhow::anyhow!("Unsupported transaction encoding")),
        };
        
        if let Some(idx) = account_keys.iter().position(|k| k == &account_str) {
            let pre = meta.pre_balances.get(idx).copied().unwrap_or(0);
            let post = meta.post_balances.get(idx).copied().unwrap_or(0);
            return Ok(post as i64 - pre as i64);
        }
    }
    
    Err(anyhow::anyhow!("Account not found or meta missing"))
}

/// Fetch SOL price from Jupiter API (reliable and free)
pub async fn fetch_sol_price_usd() -> Result<f64> {
    let client = get_shared_http_client();
    let response = client
        .get("https://api.jup.ag/price/v2?ids=So11111111111111111111111111111111111111112")
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?;
        
    if let Some(price_str) = response["data"]["So11111111111111111111111111111111111111112"]["price"].as_str() {
        let price = price_str.parse::<f64>()?;
        Ok(price)
    } else {
        Err(anyhow::anyhow!("Failed to parse price from Jupiter response"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    #[tokio::test]
    async fn test_retry_success() {
        use std::sync::atomic::{AtomicU32, Ordering};
        let attempts = AtomicU32::new(0);
        let result = retry_with_backoff(
            || {
                let attempts = &attempts;
                async move {
                    let count = attempts.fetch_add(1, Ordering::SeqCst);
                    if count < 2 {
                        Result::<u32, String>::Err("Not ready".to_string())
                    } else {
                        Ok(42)
                    }
                }
            },
            5,
            10,
        )
        .await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 42);
        assert!(attempts.load(Ordering::SeqCst) >= 3);
    }

    #[tokio::test]
    async fn test_retry_max_attempts() {
        let attempts = AtomicU32::new(0);
        let result = retry_with_backoff(
            || {
                let attempts = &attempts;
                async move {
                    attempts.fetch_add(1, Ordering::SeqCst);
                    Result::<u32, String>::Err("Always fails".to_string())
                }
            },
            3,
            10,
        )
        .await;

        assert!(result.is_err());
        assert_eq!(attempts.load(Ordering::SeqCst), 3);
        assert!(result.unwrap_err().to_string().contains("Failed after 3 attempts"));
    }

    #[tokio::test]
    async fn test_retry_exponential_backoff() {
        use std::sync::atomic::{AtomicU32, Ordering};
        let start = std::time::Instant::now();
        let attempts = AtomicU32::new(0);

        let _ = retry_with_backoff(
            || {
                let attempts = &attempts;
                async move {
                    let count = attempts.fetch_add(1, Ordering::SeqCst);
                    if count < 2 {
                        Result::<u32, String>::Err("Not ready".to_string())
                    } else {
                        Ok(42)
                    }
                }
            },
            5,
            50, // 50ms initial delay
        )
        .await;

        let elapsed = start.elapsed();
        // Should have waited: 50ms (after attempt 1) + 100ms (after attempt 2) = ~150ms
        assert!(elapsed.as_millis() >= 140); // Allow some margin
        assert!(elapsed.as_millis() < 500); // But not too much
    }

    #[tokio::test]
    async fn test_retry_with_fixed_delay() {
        use std::sync::atomic::{AtomicU32, Ordering};
        let start = std::time::Instant::now();
        let attempts = AtomicU32::new(0);

        let _ = retry_with_fixed_delay(
            || {
                let attempts = &attempts;
                async move {
                    let count = attempts.fetch_add(1, Ordering::SeqCst);
                    if count < 2 {
                        Result::<u32, String>::Err("Not ready".to_string())
                    } else {
                        Ok(42)
                    }
                }
            },
            5,
            50, // Fixed 50ms delay
        )
        .await;

        let elapsed = start.elapsed();
        // Should have waited: 50ms + 50ms = ~100ms
        assert!(elapsed.as_millis() >= 90);
        assert!(elapsed.as_millis() < 300);
    }

    #[test]
    fn test_create_http_client() {
        let client = create_http_client(5);
        assert!(client.is_ok());
    }

    #[test]
    fn test_create_default_http_client() {
        let client = create_default_http_client();
        assert!(client.is_ok());
    }

    #[test]
    fn test_format_duration() {
        assert_eq!(format_duration(Duration::from_secs(5)), "5s");
        assert_eq!(format_duration(Duration::from_secs(65)), "1m 5s");
        assert_eq!(format_duration(Duration::from_secs(3665)), "1h 1m 5s");
    }

    #[test]
    fn test_clamp() {
        assert_eq!(clamp(5, 0, 10), 5);
        assert_eq!(clamp(-5, 0, 10), 0);
        assert_eq!(clamp(15, 0, 10), 10);
        assert_eq!(clamp(5.5, 0.0, 10.0), 5.5);
    }

    #[tokio::test]
    #[ignore]
    async fn test_retry_with_real_http() {
        // Integration test - requires network
        let client = create_default_http_client().unwrap();
        
        let result = retry_with_backoff(
            || {
                let client = &client;
                async move {
                    let response = client
                        .get("https://httpbin.org/status/200")
                        .send()
                        .await?;
                    if response.status().is_success() {
                        Ok(())
                    } else {
                        Err(anyhow::anyhow!("Request failed"))
                    }
                }
            },
            3,
            100,
        )
        .await;

        assert!(result.is_ok());
    }
}
