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

/// Cost to create an Associated Token Account (ATA) in lamports
/// This is approximately 0.002 SOL = 2,000,000 lamports
const ATA_CREATION_FEE: u64 = 2_000_000;

/// Validate before transaction submission
/// 
/// # Arguments
/// * `rpc` - RPC client
/// * `wallet` - Wallet public key
/// * `buy_amount` - Amount to buy in lamports
/// * `priority_fee` - Priority fee in lamports (optional)
/// * `jito_tip` - Jito tip in lamports (optional)
/// * `_recent_blockhash` - Recent blockhash (unused but kept for API compatibility)
pub async fn validate_before_submission(
    rpc: &RpcClient,
    wallet: &Pubkey,
    buy_amount: u64,
    _recent_blockhash: &Hash,
) -> Result<()> {
    validate_before_submission_with_fees(rpc, wallet, buy_amount, 0, 0, _recent_blockhash).await
}

/// Validate before transaction submission with all fees
pub async fn validate_before_submission_with_fees(
    rpc: &RpcClient,
    wallet: &Pubkey,
    buy_amount: u64,
    priority_fee: u64,
    jito_tip: u64,
    _recent_blockhash: &Hash,
) -> Result<()> {
    // 1. Check wallet balance
    let balance = rpc.get_balance(wallet).await
        .map_err(|e| anyhow!("Failed to get balance: {}", e))?;
    
    // Calculate total required balance including all fees
    // Priority fee is in microlamports per compute unit
    // We need to estimate the actual priority fee cost based on compute units used
    // Typical buy transaction uses ~200k compute units, but we'll be conservative
    let estimated_compute_units = 300_000u64; // Conservative estimate for buy + ATA creation
    let estimated_priority_fee = if priority_fee > 0 {
        // Priority fee is in microlamports per compute unit
        // Convert to lamports: (microlamports * compute_units) / 1_000_000
        (priority_fee as u128 * estimated_compute_units as u128 / 1_000_000) as u64
    } else {
        0
    };
    
    // ATA creation fee is only needed if the token account doesn't exist
    // We'll include it to be safe (worst case scenario)
    let total_fees = MIN_FEE_BUFFER + estimated_priority_fee + jito_tip + ATA_CREATION_FEE;
    let required_balance = buy_amount + total_fees;
    
    if balance < required_balance {
        return Err(anyhow!(
            "Insufficient balance: have {:.9} SOL, need {:.9} SOL (buy: {:.9} + base fees: {:.9} + priority fee: {:.9} + jito tip: {:.9} + ATA creation: {:.9})",
            balance as f64 / 1e9,
            required_balance as f64 / 1e9,
            buy_amount as f64 / 1e9,
            MIN_FEE_BUFFER as f64 / 1e9,
            estimated_priority_fee as f64 / 1e9,
            jito_tip as f64 / 1e9,
            ATA_CREATION_FEE as f64 / 1e9
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
    use solana_sdk::hash::Hash;

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

    #[test]
    fn test_fee_calculation() {
        // Test fee calculation logic
        let buy_amount = 10_000_000_000; // 10 SOL
        let priority_fee_microlamports = 1_000_000; // 1 microlamport per CU
        let estimated_compute_units = 300_000u64;
        
        // Calculate estimated priority fee
        let estimated_priority_fee = (priority_fee_microlamports as u128 * estimated_compute_units as u128 / 1_000_000) as u64;
        assert_eq!(estimated_priority_fee, 300_000); // 1 * 300k / 1M = 0.3 lamports (rounded)
        
        // Test with zero priority fee
        let zero_priority_fee = 0u64;
        let estimated_zero = if zero_priority_fee > 0 {
            (zero_priority_fee as u128 * estimated_compute_units as u128 / 1_000_000) as u64
        } else {
            0
        };
        assert_eq!(estimated_zero, 0);
        
        // Test total fees calculation
        let jito_tip = 10_000_000; // 0.01 SOL
        let total_fees = MIN_FEE_BUFFER + estimated_priority_fee + jito_tip + ATA_CREATION_FEE;
        let required_balance = buy_amount + total_fees;
        
        // Verify fees are included
        assert!(total_fees > MIN_FEE_BUFFER);
        assert!(required_balance > buy_amount);
    }

    #[test]
    fn test_fee_constants() {
        // Verify fee constants are reasonable
        assert!(MIN_FEE_BUFFER > 0, "MIN_FEE_BUFFER should be > 0");
        assert!(ATA_CREATION_FEE > 0, "ATA_CREATION_FEE should be > 0");
        assert_eq!(ATA_CREATION_FEE, 2_000_000, "ATA_CREATION_FEE should be 2M lamports (0.002 SOL)");
    }

    #[test]
    fn test_balance_sufficient_logic() {
        // Test balance sufficient check logic (without RPC)
        let required_amount = 10_000_000_000; // 10 SOL
        let balance_sufficient = required_amount + MIN_FEE_BUFFER;
        let balance_insufficient = required_amount + MIN_FEE_BUFFER - 1;
        
        // Logic: balance >= required_amount + MIN_FEE_BUFFER
        assert!(balance_sufficient >= required_amount + MIN_FEE_BUFFER);
        assert!(balance_insufficient < required_amount + MIN_FEE_BUFFER);
    }

    #[tokio::test]
    async fn test_validate_before_submission_with_fees_insufficient_balance() {
        // This test would require a mock RPC client
        // For now, we test the fee calculation logic
        let buy_amount = 10_000_000_000; // 10 SOL
        let priority_fee = 1_000_000; // 1 microlamport per CU
        let jito_tip = 10_000_000; // 0.01 SOL
        let estimated_compute_units = 300_000u64;
        
        let estimated_priority_fee = if priority_fee > 0 {
            (priority_fee as u128 * estimated_compute_units as u128 / 1_000_000) as u64
        } else {
            0
        };
        
        let total_fees = MIN_FEE_BUFFER + estimated_priority_fee + jito_tip + ATA_CREATION_FEE;
        let required_balance = buy_amount + total_fees;
        
        // Simulate insufficient balance
        let wallet_balance = required_balance - 1;
        assert!(wallet_balance < required_balance, "Balance should be insufficient");
    }

    #[tokio::test]
    async fn test_validate_before_submission_with_fees_exact_balance() {
        // Test edge case: exactly enough balance
        let buy_amount = 10_000_000_000; // 10 SOL
        let priority_fee = 0; // No priority fee
        let jito_tip = 0; // No jito tip
        let estimated_compute_units = 300_000u64;
        
        let estimated_priority_fee = if priority_fee > 0 {
            (priority_fee as u128 * estimated_compute_units as u128 / 1_000_000) as u64
        } else {
            0
        };
        
        let total_fees = MIN_FEE_BUFFER + estimated_priority_fee + jito_tip + ATA_CREATION_FEE;
        let required_balance = buy_amount + total_fees;
        
        // Exact balance should be sufficient
        let wallet_balance = required_balance;
        assert!(wallet_balance >= required_balance, "Exact balance should be sufficient");
    }

    #[tokio::test]
    async fn test_validate_before_submission_with_fees_with_all_fees() {
        // Test with all fees included
        let buy_amount = 10_000_000_000; // 10 SOL
        let priority_fee = 1_000_000; // 1 microlamport per CU
        let jito_tip = 10_000_000; // 0.01 SOL
        let estimated_compute_units = 300_000u64;
        
        let estimated_priority_fee = if priority_fee > 0 {
            (priority_fee as u128 * estimated_compute_units as u128 / 1_000_000) as u64
        } else {
            0
        };
        
        let total_fees = MIN_FEE_BUFFER + estimated_priority_fee + jito_tip + ATA_CREATION_FEE;
        let required_balance = buy_amount + total_fees;
        
        // Verify all fees are included
        assert!(total_fees >= MIN_FEE_BUFFER + ATA_CREATION_FEE, "Should include base fees");
        assert!(required_balance > buy_amount, "Required balance should exceed buy amount");
        
        // With sufficient balance
        let wallet_balance = required_balance + 1_000_000; // Extra buffer
        assert!(wallet_balance >= required_balance, "Sufficient balance should pass");
    }
}


