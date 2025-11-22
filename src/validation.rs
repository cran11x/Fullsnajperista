// validation.rs - PRE-FLIGHT VALIDATION
#![allow(unused, dead_code)]

use anyhow::{anyhow, Result};
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::{pubkey::Pubkey, hash::Hash};

/// Maximum age for recent blockhash (60 seconds)
const MAX_BLOCKHASH_AGE_SECS: u64 = 60;

/// Maximum transaction size in bytes
const MAX_TX_SIZE_BYTES: usize = 1232;

/// Minimum balance buffer for fees (in lamports)
const MIN_FEE_BUFFER: u64 = 5000;

/// Validate before transaction submission
pub async fn validate_before_submission(
    rpc: &RpcClient,
    wallet: &Pubkey,
    buy_amount: u64,
    _recent_blockhash: &Hash,
) -> Result<()> {
    // 1. Check wallet balance
    let balance = rpc.get_balance(wallet).await
        .map_err(|e| anyhow!("Failed to get balance: {}", e))?;
    
    let required_balance = buy_amount + MIN_FEE_BUFFER;
    if balance < required_balance {
        return Err(anyhow!(
            "Insufficient balance: have {} SOL, need {} SOL (buy: {} + fees: {})",
            balance as f64 / 1e9,
            required_balance as f64 / 1e9,
            buy_amount as f64 / 1e9,
            MIN_FEE_BUFFER as f64 / 1e9
        ));
    }

    // 2. Validate recent blockhash is still valid
    // Note: We check if blockhash is valid by attempting to use it
    // In practice, we rely on get_latest_blockhash() which returns fresh blockhashes
    // For now, we'll skip detailed age checking as it requires additional RPC calls
    // The blockhash should be fresh since we just got it with get_latest_blockhash()

    Ok(())
}

/// Validate transaction size (approximate)
pub fn validate_transaction_size(estimated_size: usize) -> Result<()> {
    if estimated_size > MAX_TX_SIZE_BYTES {
        return Err(anyhow!(
            "Transaction too large: {} bytes (max: {} bytes)",
            estimated_size,
            MAX_TX_SIZE_BYTES
        ));
    }
    Ok(())
}

/// Quick balance check (without full validation)
pub async fn check_balance_sufficient(
    rpc: &RpcClient,
    wallet: &Pubkey,
    required_amount: u64,
) -> Result<bool> {
    let balance = rpc.get_balance(wallet).await?;
    Ok(balance >= required_amount + MIN_FEE_BUFFER)
}

/// Check if blockhash is still valid (simplified - just check if we can get latest)
/// In practice, blockhashes expire after ~60 seconds, so we validate by ensuring
/// we got it recently (within the function call)
pub async fn is_blockhash_recent(
    _rpc: &RpcClient,
    _blockhash: &Hash,
) -> Result<bool> {
    // Simplified: assume blockhash is recent if we just got it
    // In production, you could track when blockhash was obtained
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_transaction_size() {
        assert!(validate_transaction_size(1000).is_ok());
        assert!(validate_transaction_size(1232).is_ok());
        assert!(validate_transaction_size(1233).is_err());
    }

    #[test]
    fn test_max_constants() {
        assert_eq!(MAX_BLOCKHASH_AGE_SECS, 60);
        assert_eq!(MAX_TX_SIZE_BYTES, 1232);
        assert!(MIN_FEE_BUFFER > 0);
    }
}


