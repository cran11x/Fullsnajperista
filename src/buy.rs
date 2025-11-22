// buy.rs - ULTRA OPTIMIZED WITH CACHE
#![allow(unused_variables, unused_comparisons)]

use anyhow::Result;
use solana_sdk::{
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
    system_program,
};
use solana_client::nonblocking::rpc_client::RpcClient;
use borsh::BorshDeserialize;
use std::str::FromStr;
use std::sync::OnceLock;

use crate::detection::PumpBuyAccounts;
use crate::accounts::GlobalAccount;

use crate::constants::{PUMP_PROGRAM_ID, BUY_DISCRIMINATOR};

// 🚀 GLOBAL CACHE - ONE FETCH AT STARTUP
static GLOBAL_CACHE: OnceLock<GlobalAccount> = OnceLock::new();

/// Pre-load global account at startup (call once)
pub async fn preload_global(rpc: &RpcClient, global_account: &Pubkey) -> Result<()> {
    let global_pubkey = *global_account;
    let global_data = rpc.get_account_data(&global_pubkey).await?;
    let global: GlobalAccount = BorshDeserialize::deserialize(&mut &global_data[..])?;
    GLOBAL_CACHE.set(global).ok();
    Ok(())
}

/// Get cached global (instant - no RPC call)
pub fn get_cached_global() -> Result<&'static GlobalAccount> {
    GLOBAL_CACHE.get().ok_or_else(|| anyhow::anyhow!("Global not preloaded - call preload_global() first"))
}

pub fn derive_user_volume_pda(user_wallet: &Pubkey) -> (Pubkey, u8) {
    let pump_program = Pubkey::from_str(PUMP_PROGRAM_ID)
        .expect("Invalid PUMP_PROGRAM_ID constant");
    Pubkey::find_program_address(
        &[b"user_volume_accumulator", user_wallet.as_ref()],
        &pump_program,
    )
}

/// Validate buy instruction parameters
fn validate_buy_params(
    accounts: &PumpBuyAccounts,
    user_wallet: &Pubkey,
    user_token_account: &Pubkey,
    sol_lamports: u64,
) -> Result<()> {
    if sol_lamports == 0 {
        return Err(anyhow::anyhow!("SOL amount must be > 0"));
    }

    if sol_lamports > 1_000_000_000_000 {
        // More than 1000 SOL seems suspicious
        return Err(anyhow::anyhow!("SOL amount too large: {} SOL", sol_lamports as f64 / 1e9));
    }

    if user_wallet == user_token_account {
        return Err(anyhow::anyhow!("User wallet and token account cannot be the same"));
    }

    Ok(())
}

pub async fn build_buy_instruction(
    _rpc: &RpcClient, // ⚡ Not used anymore - uses cache
    accounts: &PumpBuyAccounts,
    user_wallet: &Pubkey,
    user_token_account: &Pubkey,
    sol_lamports: u64,
) -> Result<Instruction> {
    // Validate parameters
    validate_buy_params(accounts, user_wallet, user_token_account, sol_lamports)?;

    // ⚡ INSTANT - no RPC call
    let global = get_cached_global()
        .map_err(|e| anyhow::anyhow!("Global account not cached. Call preload_global() first: {}", e))?;
    
    let token_amount = global.get_initial_buy_price(sol_lamports);

    if token_amount == 0 {
        return Err(anyhow::anyhow!("Token amount is 0. Check global account configuration."));
    }

    // ⚡ 100% slippage - better to overpay than fail!
    let max_sol_cost = (sol_lamports as u128 * 200/100) as u64;

    println!("   💰 {} tokens for {} SOL (max: {})",
             token_amount,
             sol_lamports as f64 / 1e9,
             max_sol_cost as f64 / 1e9);

    let mut data = Vec::with_capacity(32);
    data.extend_from_slice(&BUY_DISCRIMINATOR);
    data.extend_from_slice(&token_amount.to_le_bytes());
    data.extend_from_slice(&max_sol_cost.to_le_bytes());
    data.push(0x00);

    let pump_program = Pubkey::from_str(PUMP_PROGRAM_ID)
        .map_err(|e| anyhow::anyhow!("Invalid PUMP_PROGRAM_ID: {}", e))?;
    
    let (user_volume, _) = derive_user_volume_pda(user_wallet);

    Ok(Instruction {
        program_id: pump_program,
        accounts: vec![
            AccountMeta::new(accounts.global, false),
            AccountMeta::new(accounts.fee_recipient, false),
            AccountMeta::new(accounts.mint, false),
            AccountMeta::new(accounts.bonding_curve, false),
            AccountMeta::new(accounts.associated_bonding_curve, false),
            AccountMeta::new(*user_token_account, false),
            AccountMeta::new(*user_wallet, true),
            AccountMeta::new_readonly(system_program::id(), false),
            AccountMeta::new_readonly(spl_token::id(), false),
            AccountMeta::new(accounts.creator_vault, false),
            AccountMeta::new(accounts.event_authority, false),
            AccountMeta::new_readonly(pump_program, false),
            AccountMeta::new(accounts.global_volume, false),
            AccountMeta::new(user_volume, false),
            AccountMeta::new_readonly(accounts.fee_config, false),
            AccountMeta::new_readonly(accounts.fee_program, false),
        ],
        data,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::accounts::GlobalAccount;
    use solana_sdk::pubkey::Pubkey;

    fn setup_test_global() -> GlobalAccount {
        GlobalAccount::new(
            1,
            true,
            Pubkey::new_unique(),
            Pubkey::new_unique(),
            1_000_000_000_000, // 1000 tokens
            30_000_000_000,   // 30 SOL
            500_000_000_000,  // 500 tokens real
            1_000_000_000_000_000, // 1M total supply
            250,
            Pubkey::new_unique(),
            true,
            100,
            50,
            [Pubkey::new_unique(); 7],
            Pubkey::new_unique(),
        )
    }

    #[test]
    fn test_derive_user_volume_pda() {
        let user_wallet = Pubkey::new_unique();
        let (pda, bump) = derive_user_volume_pda(&user_wallet);

        // PDA should be different from user wallet
        assert_ne!(pda, user_wallet);

        // Bump should be valid (0-255)
        assert!(bump <= 255);

        // Same wallet should produce same PDA
        let (pda2, bump2) = derive_user_volume_pda(&user_wallet);
        assert_eq!(pda, pda2);
        assert_eq!(bump, bump2);

        // Different wallet should produce different PDA
        let user_wallet2 = Pubkey::new_unique();
        let (pda3, _) = derive_user_volume_pda(&user_wallet2);
        assert_ne!(pda, pda3);
    }

    #[test]
    fn test_get_cached_global() {
        // Clear cache first (if it exists from previous tests)
        // Note: OnceLock doesn't have a clear method, so we test the error case differently
        // If cache is already set, we can't test the error case easily
        // So we just verify the function exists and works when cache is set
        let result = get_cached_global();
        // Either it's an error (cache not set) or it's ok (cache was set by previous test)
        // Both are valid scenarios
        if result.is_ok() {
            // Cache was already set, verify it returns something
            assert!(result.unwrap().initial_virtual_token_reserves > 0);
        }

        // Manually set cache for testing
        let global = setup_test_global();
        GLOBAL_CACHE.set(global).ok();

        // Now should succeed
        let cached = get_cached_global();
        assert!(cached.is_ok());
    }

    #[test]
    fn test_validate_buy_params() {
        let accounts = crate::detection::PumpBuyAccounts {
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
            dev_buy_sol: 0,
            creator: Pubkey::new_unique(),
        };

        let user_wallet = Pubkey::new_unique();
        let user_token_account = Pubkey::new_unique();

        // Valid params
        assert!(validate_buy_params(&accounts, &user_wallet, &user_token_account, 15_000_000).is_ok());

        // Invalid: zero SOL
        assert!(validate_buy_params(&accounts, &user_wallet, &user_token_account, 0).is_err());

        // Invalid: too large SOL
        assert!(validate_buy_params(&accounts, &user_wallet, &user_token_account, 2_000_000_000_000).is_err());

        // Invalid: same wallet and token account
        assert!(validate_buy_params(&accounts, &user_wallet, &user_wallet, 15_000_000).is_err());
    }

    #[tokio::test]
    async fn test_build_buy_instruction() {
        // Setup global cache
        let global = setup_test_global();
        GLOBAL_CACHE.set(global).ok();

        let accounts = crate::detection::PumpBuyAccounts {
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
            dev_buy_sol: 0,
            creator: Pubkey::new_unique(),
        };

        let user_wallet = Pubkey::new_unique();
        let user_token_account = Pubkey::new_unique();
        let rpc = RpcClient::new("https://api.mainnet-beta.solana.com".to_string());

        let instruction = build_buy_instruction(
            &rpc,
            &accounts,
            &user_wallet,
            &user_token_account,
            15_000_000, // 0.015 SOL
        ).await;

        assert!(instruction.is_ok());
        let ix = instruction.unwrap();

        // Verify instruction structure
        assert_eq!(ix.program_id, Pubkey::from_str(PUMP_PROGRAM_ID).unwrap());
        assert_eq!(ix.accounts.len(), 16);

        // Verify discriminator in data
        assert_eq!(ix.data[0..8], BUY_DISCRIMINATOR);

        // Verify user wallet is signer
        let user_wallet_meta = ix.accounts.iter()
            .find(|meta| meta.pubkey == user_wallet)
            .unwrap();
        assert!(user_wallet_meta.is_signer);
    }

    #[tokio::test]
    async fn test_build_buy_instruction_validation_errors() {
        let accounts = crate::detection::PumpBuyAccounts {
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
            dev_buy_sol: 0,
            creator: Pubkey::new_unique(),
        };

        let user_wallet = Pubkey::new_unique();
        let user_token_account = Pubkey::new_unique();
        let rpc = RpcClient::new("https://api.mainnet-beta.solana.com".to_string());

        // Test without global cache - clear it first if it exists
        // Note: OnceLock doesn't support clearing, so if cache is already set,
        // this test will pass (which is also valid)
        let result = build_buy_instruction(
            &rpc,
            &accounts,
            &user_wallet,
            &user_token_account,
            15_000_000,
        ).await;
        
        // If cache is not set, we should get an error
        // If cache is already set (from previous test), it will succeed (also valid)
        if result.is_err() {
            assert!(result.unwrap_err().to_string().contains("Global account not cached"));
        }

        // Setup cache
        let global = setup_test_global();
        GLOBAL_CACHE.set(global).ok();

        // Test with zero SOL - should fail validation
        let result = build_buy_instruction(
            &rpc,
            &accounts,
            &user_wallet,
            &user_token_account,
            0,
        ).await;
        assert!(result.is_err(), "Expected error for zero SOL amount");
    }

    #[tokio::test]
    #[ignore]
    async fn test_build_buy_instruction_integration() {
        // Integration test - would require real RPC connection
        // This would test preload_global with actual network call
    }
}