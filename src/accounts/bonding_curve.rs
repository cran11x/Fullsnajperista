// bonding_curve.rs - BONDING CURVE ACCOUNT & MC CALCULATION (WITH RETRY)
#![allow(unused_imports, dead_code)]

use anyhow::Result;
use borsh::{BorshDeserialize, BorshSerialize};
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::pubkey::Pubkey;
use std::time::Duration;

/// Bonding curve account structure (from pump.fun program)
#[derive(Debug, Clone, Default, BorshSerialize, BorshDeserialize)]
pub struct BondingCurveAccount {
    /// Discriminator (8 bytes)
    pub discriminator: u64,
    /// Virtual token reserves (for AMM calculations)
    pub virtual_token_reserves: u64,
    /// Virtual SOL reserves (for AMM calculations)
    pub virtual_sol_reserves: u64,
    /// Real token reserves (actual tokens available)
    pub real_token_reserves: u64,
    /// Real SOL reserves (actual SOL in curve)
    pub real_sol_reserves: u64,
    /// Token total supply
    pub token_total_supply: u64,
    /// Is migration complete
    pub complete: bool,
}

impl BondingCurveAccount {
    /// Calculate current market cap in SOL
    /// Formula: MC = (virtual_sol / virtual_token) * total_supply
    pub fn calculate_mc_sol(&self) -> f64 {
        if self.virtual_token_reserves == 0 {
            return 0.0;
        }

        // Current price per token in SOL (using virtual reserves)
        let price_per_token = self.virtual_sol_reserves as f64 / self.virtual_token_reserves as f64;

        // MC = price * total supply
        let mc_lamports = price_per_token * self.token_total_supply as f64;

        // Convert to SOL
        mc_lamports / 1e9
    }

    /// Calculate current market cap in USD
    pub fn calculate_mc_usd(&self, sol_price_usd: f64) -> f64 {
        self.calculate_mc_sol() * sol_price_usd
    }

    /// Get current token price in SOL
    pub fn get_token_price_sol(&self) -> f64 {
        if self.virtual_token_reserves == 0 {
            return 0.0;
        }
        // Price per token in SOL (not in lamports)
        // Formula: (virtual_sol_reserves / virtual_token_reserves) / 1000
        // Explanation: 
        // - virtual_sol_reserves is in lamports (1e9)
        // - virtual_token_reserves is in raw units (1e6)
        // - Ratio R = lamports / raw_units
        // - Price (SOL/Token) = (lamports / 1e9) / (raw_units / 1e6)
        // - Price = (lamports / raw_units) * (1e6 / 1e9)
        // - Price = R / 1000
        (self.virtual_sol_reserves as f64 / self.virtual_token_reserves as f64) / 1000.0
    }

    /// Calculate token amount for given SOL amount using CURRENT bonding curve reserves
    /// Uses the same formula as pump.fun program
    /// 
    /// # Arguments
    /// * `sol_lamports` - Amount of SOL to spend (in lamports)
    /// 
    /// # Returns
    /// Amount of tokens that would be received at current bonding curve price
    pub fn calculate_token_amount_for_sol(&self, sol_lamports: u64) -> u64 {
        if sol_lamports == 0 || self.virtual_token_reserves == 0 || self.virtual_sol_reserves == 0 {
            return 0;
        }
        
        // Pump.fun formula (same as get_initial_buy_price but using current reserves):
        // n = virtual_sol_reserves * virtual_token_reserves
        // new_virtual_sol = virtual_sol_reserves + sol_lamports
        // new_virtual_token = n / new_virtual_sol + 1
        // tokens_out = virtual_token_reserves - new_virtual_token
        
        let n: u128 = (self.virtual_sol_reserves as u128) * (self.virtual_token_reserves as u128);
        let new_virtual_sol: u128 = (self.virtual_sol_reserves as u128) + (sol_lamports as u128);
        let new_virtual_token: u128 = n / new_virtual_sol + 1;
        let tokens_out: u128 = (self.virtual_token_reserves as u128) - new_virtual_token;
        
        // Cap at real_token_reserves
        if tokens_out < (self.real_token_reserves as u128) {
            tokens_out as u64
        } else {
            self.real_token_reserves
        }
    }

    /// Display bonding curve state
    pub fn display(&self, sol_price_usd: f64) {
        println!("      📊 Bonding Curve State:");
        println!("         Virtual: {} SOL / {} tokens",
                 self.virtual_sol_reserves as f64 / 1e9,
                 self.virtual_token_reserves as f64 / 1e9);
        println!("         Real: {} SOL / {} tokens",
                 self.real_sol_reserves as f64 / 1e9,
                 self.real_token_reserves as f64 / 1e9);
        println!("         💰 Token Price: {:.8} SOL (${:.6})",
                 self.get_token_price_sol(),
                 self.get_token_price_sol() * sol_price_usd);
        println!("         🎯 Market Cap: {:.2} SOL (${:.0})",
                 self.calculate_mc_sol(),
                 self.calculate_mc_usd(sol_price_usd));
    }
}

/// Fetch bonding curve account and calculate MC (WITH RETRY for fresh tokens)
pub async fn fetch_bonding_curve_mc(
    rpc: &RpcClient,
    bonding_curve: &Pubkey,
    sol_price_usd: f64,
) -> Result<(BondingCurveAccount, f64, f64)> {
    let max_attempts = 5;
    let mut last_error = None;

    for attempt in 1..=max_attempts {
        match try_fetch_once(rpc, bonding_curve, sol_price_usd).await {
            Ok(result) => {
                if attempt > 1 {
                    println!("      ✅ MC fetched (attempt {})", attempt);
                }
                return Ok(result);
            }
            Err(e) => {
                last_error = Some(e);
                if attempt < max_attempts {
                    // Wait 150ms before retry (account needs time to propagate)
                    tokio::time::sleep(Duration::from_millis(50)).await;
                }
            }
        }
    }

    Err(last_error.unwrap())
}

/// Single fetch attempt (internal)
use solana_sdk::commitment_config::CommitmentConfig;
use solana_client::rpc_config::RpcAccountInfoConfig;

async fn try_fetch_once(
    rpc: &RpcClient,
    bonding_curve: &Pubkey,
    sol_price_usd: f64,
) -> Result<(BondingCurveAccount, f64, f64)> {
    // Fetch with CONFIRMED commitment
    let account = rpc.get_account_with_commitment(
        bonding_curve,
        CommitmentConfig::confirmed()  // ← KEY FIX
    ).await?;

    let account_data = account.value
        .ok_or_else(|| anyhow::anyhow!("Account not found"))?
        .data;

    // Deserialize...
    let curve: BondingCurveAccount = BorshDeserialize::deserialize(&mut &account_data[..])?;

    let mc_sol = curve.calculate_mc_sol();
    let mc_usd = curve.calculate_mc_usd(sol_price_usd);

    Ok((curve, mc_sol, mc_usd))
}

/// Quick MC check (no account return, just numbers)
pub async fn quick_mc_check(
    rpc: &RpcClient,
    bonding_curve: &Pubkey,
    sol_price_usd: f64,
) -> Result<(f64, f64)> {
    let (_, mc_sol, mc_usd) = fetch_bonding_curve_mc(rpc, bonding_curve, sol_price_usd).await?;
    Ok((mc_sol, mc_usd))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mc_calculation() {
        let curve = BondingCurveAccount {
            discriminator: 1,
            virtual_token_reserves: 1_000_000_000_000, // 1000 tokens
            virtual_sol_reserves: 30_000_000_000,      // 30 SOL
            real_token_reserves: 500_000_000_000,
            real_sol_reserves: 15_000_000_000,
            token_total_supply: 1_000_000_000_000_000, // 1M tokens
            complete: false,
        };

        // Price = 30 / 1000 = 0.03 SOL per token
        // MC = 0.03 * 1M = 30,000 SOL
        let mc_sol = curve.calculate_mc_sol();
        assert!((mc_sol - 30_000.0).abs() < 0.1);

        let mc_usd = curve.calculate_mc_usd(100.0);
        assert!((mc_usd - 3_000_000.0).abs() < 100.0);
    }

    #[test]
    fn test_token_price() {
        let curve = BondingCurveAccount {
            discriminator: 1,
            virtual_token_reserves: 1_000_000_000_000, // 1,000,000 tokens (6 decimals)
            virtual_sol_reserves: 50_000_000_000, // 50 SOL
            real_token_reserves: 500_000_000_000,
            real_sol_reserves: 25_000_000_000,
            token_total_supply: 1_000_000_000_000_000,
            complete: false,
        };

        // Price = 50 / 1,000,000 = 0.00005 SOL per token
        let price = curve.get_token_price_sol();
        assert!((price - 0.00005).abs() < 0.0000001, "Expected 0.00005 but got {}", price);
    }

    #[test]
    fn test_mc_calculation_edge_cases() {
        // Test with zero virtual token reserves
        let curve = BondingCurveAccount {
            discriminator: 1,
            virtual_token_reserves: 0,
            virtual_sol_reserves: 50_000_000_000,
            real_token_reserves: 500_000_000_000,
            real_sol_reserves: 25_000_000_000,
            token_total_supply: 1_000_000_000_000_000,
            complete: false,
        };

        let mc_sol = curve.calculate_mc_sol();
        assert_eq!(mc_sol, 0.0);

        let mc_usd = curve.calculate_mc_usd(100.0);
        assert_eq!(mc_usd, 0.0);

        let price = curve.get_token_price_sol();
        assert_eq!(price, 0.0);

        // Test with very small reserves
        let curve = BondingCurveAccount {
            discriminator: 1,
            virtual_token_reserves: 1,
            virtual_sol_reserves: 1,
            real_token_reserves: 1,
            real_sol_reserves: 1,
            token_total_supply: 1_000_000_000_000_000,
            complete: false,
        };

        let mc_sol = curve.calculate_mc_sol();
        assert!(mc_sol > 0.0);
        assert!(mc_sol.is_finite());

        // Test with zero total supply
        let curve = BondingCurveAccount {
            discriminator: 1,
            virtual_token_reserves: 1_000_000_000_000,
            virtual_sol_reserves: 50_000_000_000,
            real_token_reserves: 500_000_000_000,
            real_sol_reserves: 25_000_000_000,
            token_total_supply: 0,
            complete: false,
        };

        let mc_sol = curve.calculate_mc_sol();
        assert_eq!(mc_sol, 0.0);
    }

    #[test]
    fn test_token_price_calculation() {
        // Test various price scenarios (assuming 6 decimals for tokens)
        let test_cases = vec![
            (1_000_000_000_000, 10_000_000_000, 0.00001), // 10 SOL / 1M tokens = 0.00001 SOL per token
            (1_000_000_000_000, 100_000_000_000, 0.0001), // 100 SOL / 1M tokens = 0.0001 SOL per token
            (500_000_000_000, 25_000_000_000, 0.00005),   // 25 SOL / 0.5M tokens = 0.00005 SOL per token
        ];

        for (token_reserves, sol_reserves, expected_price) in test_cases {
            let curve = BondingCurveAccount {
                discriminator: 1,
                virtual_token_reserves: token_reserves,
                virtual_sol_reserves: sol_reserves,
                real_token_reserves: token_reserves / 2,
                real_sol_reserves: sol_reserves / 2,
                token_total_supply: 1_000_000_000_000_000,
                complete: false,
            };

            let price = curve.get_token_price_sol();
            assert!((price - expected_price).abs() < 0.0000001, 
                "Expected price {} but got {} for reserves {} / {}", 
                expected_price, price, sol_reserves, token_reserves);
        }
    }

    #[test]
    fn test_mc_with_different_sol_prices() {
        let curve = BondingCurveAccount {
            discriminator: 1,
            virtual_token_reserves: 1_000_000_000_000,
            virtual_sol_reserves: 30_000_000_000, // 30 SOL
            real_token_reserves: 500_000_000_000,
            real_sol_reserves: 15_000_000_000,
            token_total_supply: 1_000_000_000_000_000, // 1M tokens
            complete: false,
        };

        // MC = 30,000 SOL
        let mc_sol = curve.calculate_mc_sol();
        assert!((mc_sol - 30_000.0).abs() < 0.1);

        // Test with different SOL prices
        assert!((curve.calculate_mc_usd(100.0) - 3_000_000.0).abs() < 100.0);
        assert!((curve.calculate_mc_usd(200.0) - 6_000_000.0).abs() < 100.0);
        assert!((curve.calculate_mc_usd(50.0) - 1_500_000.0).abs() < 100.0);
    }

    #[test]
    fn test_display_method() {
        let curve = BondingCurveAccount {
            discriminator: 1,
            virtual_token_reserves: 1_000_000_000_000,
            virtual_sol_reserves: 30_000_000_000,
            real_token_reserves: 500_000_000_000,
            real_sol_reserves: 15_000_000_000,
            token_total_supply: 1_000_000_000_000_000,
            complete: false,
        };

        // Just verify it doesn't panic
        curve.display(162.0);
    }

    #[test]
    #[ignore]
    fn test_fetch_bonding_curve_with_mock_rpc() {
        // Integration test - would require mock RPC client
        // This would test the actual fetch_bonding_curve_mc function
        // with a mock RPC that returns known bonding curve data
    }
}

/// Token-specific global account structure (from pump.fun program)
/// Note: This is different from the program-level GlobalAccount in global.rs
#[derive(Debug, Clone, Default, BorshSerialize, BorshDeserialize)]
pub struct TokenGlobalAccount {
    pub discriminator: u64,
    pub initialized: bool,
    pub mint: Pubkey,
    pub mint_authority: Pubkey,
    pub virtual_token_reserves: u64,
    pub virtual_sol_reserves: u64,
    pub real_token_reserves: u64,
    pub real_sol_reserves: u64,
    pub token_total_supply: u64,
    pub bonding_curve_bump: u8,
    pub associated_bonding_curve: Pubkey,
    pub complete: bool,
    pub global_authority: u8,
}

impl TokenGlobalAccount {
    pub fn new(
        discriminator: u64,
        initialized: bool,
        mint: Pubkey,
        mint_authority: Pubkey,
        virtual_token_reserves: u64,
        virtual_sol_reserves: u64,
        real_token_reserves: u64,
        real_sol_reserves: u64,
        token_total_supply: u64,
        bonding_curve_bump: u8,
        associated_bonding_curve: Pubkey,
        complete: bool,
        global_authority: u8,
    ) -> Self {
        Self {
            discriminator,
            initialized,
            mint,
            mint_authority,
            virtual_token_reserves,
            virtual_sol_reserves,
            real_token_reserves,
            real_sol_reserves,
            token_total_supply,
            bonding_curve_bump,
            associated_bonding_curve,
            complete,
            global_authority,
        }
    }

    /// Get initial buy price (token amount for given SOL amount)
    /// This is a simplified calculation - adjust based on actual pump.fun formula
    pub fn get_initial_buy_price(&self, sol_lamports: u64) -> u64 {
        if self.virtual_sol_reserves == 0 || self.virtual_token_reserves == 0 {
            return 0;
        }
        // Simplified calculation: tokens = (sol * virtual_token_reserves) / virtual_sol_reserves
        (sol_lamports as u128 * self.virtual_token_reserves as u128 / self.virtual_sol_reserves as u128) as u64
    }
}