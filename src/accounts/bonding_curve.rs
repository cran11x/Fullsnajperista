// bonding_curve.rs - BONDING CURVE ACCOUNT & MC CALCULATION (WITH RETRY)
#![allow(unused_imports, dead_code)]

use anyhow::Result;
use borsh::{BorshDeserialize, BorshSerialize};
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::pubkey::Pubkey;
use std::time::{Duration, Instant};
use std::collections::HashMap;

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
    /// Formula: MC = price_per_token * (token_total_supply / 1e6)
    /// Uses get_token_price_sol() for consistency
    pub fn calculate_mc_sol(&self) -> f64 {
        if self.virtual_token_reserves == 0 {
            return 0.0;
        }

        // Get price per token using the same formula as get_token_price_sol()
        let price_per_token = self.get_token_price_sol();

        // token_total_supply is in raw units (6 decimals), convert to actual tokens
        let tokens_actual = self.token_total_supply as f64 / 1e6;

        // MC = price * actual token supply
        price_per_token * tokens_actual
    }

    /// Calculate current market cap in USD (uses cached SOL price)
    pub fn calculate_mc_usd(&self) -> f64 {
        use crate::utils::get_cached_sol_price;
        self.calculate_mc_sol() * get_cached_sol_price()
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
    pub fn display(&self) {
        use crate::utils::{get_cached_sol_price, sol_to_usd};
        let _sol_price = get_cached_sol_price();
        println!("      📊 Bonding Curve State:");
        println!("         Virtual: {} SOL / {} tokens",
                 self.virtual_sol_reserves as f64 / 1e9,
                 self.virtual_token_reserves as f64 / 1e6); // ✅ FIX: 6 decimals for tokens, not 9
        println!("         Real: {} SOL / {} tokens",
                 self.real_sol_reserves as f64 / 1e9,
                 self.real_token_reserves as f64 / 1e6); // ✅ FIX: 6 decimals for tokens, not 9
        println!("         💰 Token Price: {:.8} SOL (${:.6})",
                 self.get_token_price_sol(),
                 sol_to_usd(self.get_token_price_sol()));
        println!("         🎯 Market Cap: {:.2} SOL (${:.0})",
                 self.calculate_mc_sol(),
                 self.calculate_mc_usd());
    }
}

/// Bonding curve cache for reducing RPC calls
/// Thread-safe cache with TTL of 2 seconds
pub struct BondingCurveCache {
    data: HashMap<String, (BondingCurveAccount, Instant)>,
    ttl: Duration,
}

impl BondingCurveCache {
    /// Create new cache with default TTL of 2 seconds
    pub fn new() -> Self {
        Self {
            data: HashMap::new(),
            ttl: Duration::from_secs(2),
        }
    }

    /// Create new cache with custom TTL
    pub fn with_ttl(ttl: Duration) -> Self {
        Self {
            data: HashMap::new(),
            ttl,
        }
    }

    /// Get cached bonding curve or fetch from RPC
    /// Returns cached value if available and not expired, otherwise fetches from RPC
    pub async fn get_or_fetch(
        &mut self,
        bonding_curve: &Pubkey,
        rpc: &RpcClient,
    ) -> Result<(BondingCurveAccount, f64)> {
        let key = bonding_curve.to_string();
        let now = Instant::now();

        // Check cache
        if let Some((cached_curve, timestamp)) = self.data.get(&key) {
            if now.duration_since(*timestamp) < self.ttl {
                // Cache hit - return cached value
                let mc_sol = cached_curve.calculate_mc_sol();
                return Ok((cached_curve.clone(), mc_sol));
            } else {
                // Cache expired - remove from cache
                self.data.remove(&key);
            }
        }

        // Cache miss or expired - fetch from RPC
        let (curve, mc_sol) = try_fetch_once(rpc, bonding_curve).await?;

        // Update cache
        self.data.insert(key, (curve.clone(), now));

        Ok((curve, mc_sol))
    }

    /// Clear expired entries from cache
    pub fn cleanup_expired(&mut self) {
        let now = Instant::now();
        self.data.retain(|_, (_, timestamp)| {
            now.duration_since(*timestamp) < self.ttl
        });
    }

    /// Clear all cache entries
    pub fn clear(&mut self) {
        self.data.clear();
    }

    /// Get cache size
    pub fn len(&self) -> usize {
        self.data.len()
    }
}

impl Default for BondingCurveCache {
    fn default() -> Self {
        Self::new()
    }
}

/// Fetch bonding curve account and calculate MC (WITH RETRY for fresh tokens)
/// Optionally uses cache if provided
pub async fn fetch_bonding_curve_mc(
    rpc: &RpcClient,
    bonding_curve: &Pubkey,
) -> Result<(BondingCurveAccount, f64)> {
    fetch_bonding_curve_mc_with_cache(rpc, bonding_curve, None).await
}

/// Fetch bonding curve account and calculate MC with optional cache
pub async fn fetch_bonding_curve_mc_with_cache(
    rpc: &RpcClient,
    bonding_curve: &Pubkey,
    cache: Option<&mut BondingCurveCache>,
) -> Result<(BondingCurveAccount, f64)> {
    // Try cache first if available
    if let Some(cache_ref) = cache {
        match cache_ref.get_or_fetch(bonding_curve, rpc).await {
            Ok(result) => return Ok(result),
            Err(_) => {
                // Cache fetch failed, fall through to retry logic
            }
        }
    }

    // Original retry logic (if cache not available or cache fetch failed)
    let max_attempts = 3; // Reduced from 5 to 3 for premium RPC
    let mut last_error = None;

    for attempt in 1..=max_attempts {
        match try_fetch_once(rpc, bonding_curve).await {
            Ok(result) => {
                if attempt > 1 {
                    println!("      ✅ MC fetched (attempt {})", attempt);
                }
                return Ok(result);
            }
            Err(e) => {
                last_error = Some(e);
                if attempt < max_attempts {
                    // Reduced delay for premium RPC (account propagates faster)
                    tokio::time::sleep(Duration::from_millis(20)).await; // Reduced from 50ms to 20ms
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
) -> Result<(BondingCurveAccount, f64)> {
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

    Ok((curve, mc_sol))
}

/// Quick MC check (no account return, just SOL value)
pub async fn quick_mc_check(
    rpc: &RpcClient,
    bonding_curve: &Pubkey,
) -> Result<f64> {
    let (_, mc_sol) = fetch_bonding_curve_mc(rpc, bonding_curve).await?;
    Ok(mc_sol)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mc_calculation() {
        let curve = BondingCurveAccount {
            discriminator: 1,
            virtual_token_reserves: 1_000_000_000_000, // 1M tokens (6 decimals)
            virtual_sol_reserves: 30_000_000_000,      // 30 SOL
            real_token_reserves: 500_000_000_000,
            real_sol_reserves: 15_000_000_000,
            token_total_supply: 1_000_000_000_000, // 1M tokens (6 decimals) = 1M * 1e6
            complete: false,
        };

        // Price = (30e9 / 1e12) / 1000 = 0.00003 SOL per token
        // MC = 0.00003 * 1M = 30 SOL
        let price = curve.get_token_price_sol();
        assert!((price - 0.00003).abs() < 0.0000001, "Price should be 0.00003 but got {}", price);
        
        let mc_sol = curve.calculate_mc_sol();
        assert!((mc_sol - 30.0).abs() < 0.1, "MC should be 30 SOL but got {}", mc_sol);

        let mc_usd = curve.calculate_mc_usd();
        // Note: This test depends on cached SOL price, so we just verify it's positive
        assert!(mc_usd > 0.0, "MC USD should be positive but got {}", mc_usd);
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

        let mc_usd = curve.calculate_mc_usd();
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
            token_total_supply: 1_000_000_000_000, // 1M tokens (6 decimals) = 1M * 1e6
            complete: false,
        };

        // MC = 30 SOL (price 0.00003 * 1M tokens)
        let mc_sol = curve.calculate_mc_sol();
        assert!((mc_sol - 30.0).abs() < 0.1, "MC should be 30 SOL but got {}", mc_sol);

        // Test USD calculation (depends on cached SOL price)
        let mc_usd = curve.calculate_mc_usd();
        assert!(mc_usd > 0.0, "MC USD should be positive");
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
        curve.display();
    }

    #[test]
    #[ignore]
    fn test_fetch_bonding_curve_with_mock_rpc() {
        // Integration test - would require mock RPC client
        // This would test the actual fetch_bonding_curve_mc function
        // with a mock RPC that returns known bonding curve data
    }

    #[test]
    fn test_bonding_curve_cache_basic() {
        let cache = BondingCurveCache::new();
        assert_eq!(cache.len(), 0);
        
        // Test default TTL
        assert_eq!(cache.ttl, Duration::from_secs(2));
    }

    #[test]
    fn test_bonding_curve_cache_with_custom_ttl() {
        let custom_ttl = Duration::from_secs(5);
        let cache = BondingCurveCache::with_ttl(custom_ttl);
        assert_eq!(cache.ttl, custom_ttl);
    }

    #[test]
    fn test_bonding_curve_cache_clear() {
        let mut cache = BondingCurveCache::new();
        cache.clear();
        assert_eq!(cache.len(), 0);
    }

    #[test]
    fn test_bonding_curve_cache_cleanup_expired() {
        let mut cache = BondingCurveCache::with_ttl(Duration::from_millis(100));
        let bonding_curve = Pubkey::new_unique();
        let curve = BondingCurveAccount::default();
        
        // Manually insert expired entry (by manipulating internal state)
        // Note: This is a bit of a hack since we can't easily create expired entries
        // In real usage, entries expire naturally after TTL
        cache.data.insert(bonding_curve.to_string(), (curve, Instant::now() - Duration::from_secs(10)));
        
        cache.cleanup_expired();
        // Expired entry should be removed
        assert_eq!(cache.len(), 0);
    }

    #[test]
    fn test_bonding_curve_cache_ttl_behavior() {
        // Test that cache respects TTL
        let mut cache = BondingCurveCache::with_ttl(Duration::from_millis(100));
        let bonding_curve1 = Pubkey::new_unique();
        let bonding_curve2 = Pubkey::new_unique();
        let curve = BondingCurveAccount::default();
        
        let now = Instant::now();
        
        // Insert fresh entry
        cache.data.insert(bonding_curve1.to_string(), (curve.clone(), now));
        
        // Insert expired entry
        cache.data.insert(bonding_curve2.to_string(), (curve, now - Duration::from_secs(10)));
        
        // Only fresh entry should remain after cleanup
        cache.cleanup_expired();
        assert_eq!(cache.len(), 1);
        assert!(cache.data.contains_key(&bonding_curve1.to_string()));
        assert!(!cache.data.contains_key(&bonding_curve2.to_string()));
    }

    #[test]
    fn test_bonding_curve_cache_multiple_entries() {
        let mut cache = BondingCurveCache::new();
        let curves: Vec<Pubkey> = (0..10).map(|_| Pubkey::new_unique()).collect();
        let curve_data = BondingCurveAccount::default();
        
        // Insert multiple entries
        for curve_pubkey in &curves {
            cache.data.insert(curve_pubkey.to_string(), (curve_data.clone(), Instant::now()));
        }
        
        assert_eq!(cache.len(), 10);
        
        // Clear all
        cache.clear();
        assert_eq!(cache.len(), 0);
    }

    #[test]
    fn test_bonding_curve_cache_ordering() {
        // Test that cache preserves order when checking multiple entries
        let mut cache = BondingCurveCache::new();
        let curves: Vec<Pubkey> = (0..5).map(|_| Pubkey::new_unique()).collect();
        let curve_data = BondingCurveAccount::default();
        
        for curve_pubkey in &curves {
            cache.data.insert(curve_pubkey.to_string(), (curve_data.clone(), Instant::now()));
        }
        
        // All entries should be present
        for curve_pubkey in &curves {
            assert!(cache.data.contains_key(&curve_pubkey.to_string()));
        }
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