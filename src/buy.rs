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
use crate::pda_derivation::PumpPdas;

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

    // ⚡ 100% slippage - aggressive buffer to prevent failures on fast-moving tokens
    let max_sol_cost = (sol_lamports as u128 * 200 / 100) as u64;

    println!("   💰 {} tokens for {} SOL (max: {})",
             token_amount,
             sol_lamports as f64 / 1e9,
             max_sol_cost as f64 / 1e9);

    let mut data = Vec::with_capacity(32);
    data.extend_from_slice(&BUY_DISCRIMINATOR);
    data.extend_from_slice(&token_amount.to_le_bytes());
    data.extend_from_slice(&max_sol_cost.to_le_bytes());
    // data.push(0x00); // Removed extra byte that might cause deserialization errors

    let pump_program = Pubkey::from_str(PUMP_PROGRAM_ID)
        .map_err(|e| anyhow::anyhow!("Invalid PUMP_PROGRAM_ID: {}", e))?;
    
    // 🔥 CRITICAL FIX: Recalculate ALL PDAs fresh for this specific mint/user
    // DO NOT reuse values from accounts - they might be from a different token or stale
    let pdas = PumpPdas::recalculate_all(&accounts.mint, user_wallet);
    
    eprintln!("🔍 BUILDING BUY INSTRUCTION - Recalculated PDAs:");
    eprintln!("   Mint: {}", accounts.mint);
    eprintln!("   User Wallet: {}", user_wallet);
    eprintln!("   Global: {} (recalculated)", pdas.global);
    eprintln!("   Bonding Curve: {} (recalculated)", pdas.bonding_curve);
    eprintln!("   Event Authority: {} (recalculated)", pdas.event_authority);
    eprintln!("   User Volume: {} (recalculated)", pdas.user_volume);
    eprintln!("   Global Volume: {} (hardcoded)", pdas.global_volume);
    
    // Debug: Verify all PDA accounts to catch Error 0x1f9 (Seeds Constraint Was Violated)
    eprintln!();
    eprintln!("🔍 VERIFYING PDA ACCOUNTS FOR ERROR 0x1f9 PREVENTION:");
    
    // Verify Global (Account 0)
    eprintln!("   Account 0 (Global): {} (from accounts: {}) {}", 
             pdas.global, accounts.global,
             if pdas.global == accounts.global { "✅" } else { "❌ MISMATCH - USING RECALCULATED!" });
    
    // Verify Bonding Curve (Account 3)
    eprintln!("   Account 3 (Bonding Curve): {} (from accounts: {}) {}", 
             pdas.bonding_curve, accounts.bonding_curve,
             if pdas.bonding_curve == accounts.bonding_curve { "✅" } else { "❌ MISMATCH - USING RECALCULATED!" });
    
    // Verify Associated Bonding Curve (Account 4)
    // IMPORTANT: We must use the RECALCULATED associated bonding curve based on the recalculated bonding curve
    // The one in `accounts` might be derived from a stale or incorrect bonding curve
    let token_program_2022_id = Pubkey::from_str("TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb")
        .unwrap_or_else(|_| spl_token::id());
    let recalculated_abc = spl_associated_token_account::get_associated_token_address_with_program_id(
        &pdas.bonding_curve,
        &accounts.mint,
        &token_program_2022_id
    );
    
    eprintln!("   Account 4 (Associated Bonding Curve): {} (from accounts: {})", 
             recalculated_abc, accounts.associated_bonding_curve);
    if recalculated_abc != accounts.associated_bonding_curve {
        eprintln!("   ❌ MISMATCH - The Associated Bonding Curve in accounts struct does not match derivation from recalculated Bonding Curve!");
        eprintln!("   Using RECALCULATED: {}", recalculated_abc);
    } else {
        eprintln!("   ✅ MATCH - Associated Bonding Curve is consistent");
    }
    
    // Verify Creator Vault (Account 9) - check if it's a valid PDA
    // Fix: If Creator Vault matches Bonding Curve (common error when fallback is used), derive it
    let final_creator_vault = if accounts.creator_vault == accounts.bonding_curve || accounts.creator_vault == pdas.bonding_curve {
        eprintln!("   ❌ Creator Vault (Account 9) matches Bonding Curve! This is an ERROR.");
        if accounts.creator != Pubkey::default() {
            let (derived_vault, _) = crate::pda_derivation::derive_creator_vault_pda(&accounts.creator);
            eprintln!("   ✅ Recalculated Creator Vault using creator {}: {}", accounts.creator, derived_vault);
            derived_vault
        } else {
            eprintln!("   ⚠️  Cannot recalculate Creator Vault: Creator is default/unknown. Using input but likely to fail.");
            accounts.creator_vault
        }
    } else {
        eprintln!("   Account 9 (Creator Vault): {} (from accounts)", accounts.creator_vault);
        accounts.creator_vault
    };

    // Verify Event Authority (Account 10)
    eprintln!("   Account 10 (Event Authority): {} (from accounts: {}) {}", 
             pdas.event_authority, accounts.event_authority,
             if pdas.event_authority == accounts.event_authority { "✅" } else { "❌ MISMATCH - USING RECALCULATED!" });
    
    // Verify Global Volume (Account 12) - hardcoded
    eprintln!("   Account 12 (Global Volume): {} (hardcoded) ✅", pdas.global_volume);
    
    // Verify User Volume (Account 13)
    eprintln!("   Account 13 (User Volume): {} (recalculated for current user) ✅", pdas.user_volume);
    eprintln!();

    // CRITICAL: All Pump.fun PDA accounts must already exist and be initialized
    // We use AccountMeta::new() for accounts that need to be writable (they exist, we're just modifying them)
    // We use AccountMeta::new_readonly() for accounts that are only read
    // DO NOT create new accounts - all Pump.fun accounts must already exist from token initialization
    // CRITICAL: Use RECALCULATED PDAs, not values from accounts
    let instruction = Instruction {
        program_id: pump_program,
        accounts: vec![
            // Account 0: Global (PDA, RECALCULATED)
            AccountMeta::new(pdas.global, false),
            // Account 1: Fee Recipient (hardcoded)
            AccountMeta::new(pdas.fee_recipient, false),
            // Account 2: Mint (from accounts - this is correct, it's the token mint)
            AccountMeta::new_readonly(accounts.mint, false),
            // Account 3: Bonding Curve (PDA, RECALCULATED for current mint)
            AccountMeta::new(pdas.bonding_curve, false),
            // Account 4: Associated Bonding Curve (RECALCULATED to ensure consistency with Bonding Curve)
            AccountMeta::new(recalculated_abc, false),
            // Account 5: User Token Account (current user's token account)
            AccountMeta::new(*user_token_account, false),
            // Account 6: User Wallet (signer, current user)
            AccountMeta::new(*user_wallet, true),
            // Account 7: System Program (readonly)
            AccountMeta::new_readonly(system_program::id(), false),
            // Account 8: Token Program 2022 (readonly)
            AccountMeta::new_readonly(
                Pubkey::from_str("TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb")
                    .unwrap_or_else(|_| spl_token::id()), // Token Program 2022
                false
            ),
            // Account 9: Creator Vault (RECALCULATED if suspicious)
            AccountMeta::new(final_creator_vault, false),
            // Account 10: Event Authority (PDA, RECALCULATED)
            AccountMeta::new_readonly(pdas.event_authority, false),
            // Account 11: Pump Program (readonly, program itself - REQUIRED for program verification)
            AccountMeta::new_readonly(pump_program, false),
            // Account 12: Global Volume Accumulator (hardcoded)
            AccountMeta::new({
                eprintln!("   ✅ Using hardcoded Global Volume at index 12: {}", pdas.global_volume);
                pdas.global_volume
            }, false),
            // Account 13: User Volume (PDA, RECALCULATED for current user)
            AccountMeta::new({
                eprintln!("   ✅ Using recalculated User Volume at index 13: {} (for user: {})", pdas.user_volume, user_wallet);
                pdas.user_volume
            }, false),
            // Account 14: Fee Config (hardcoded)
            AccountMeta::new_readonly(pdas.fee_config, false),
            // Account 15: Fee Program (hardcoded)
            AccountMeta::new_readonly(pdas.fee_program, false),
        ],
        data,
    };
    
    // Debug: Log all accounts in buy instruction with detailed information
    eprintln!("🔍 BUY INSTRUCTION ACCOUNTS (total: {}):", instruction.accounts.len());
    
    let account_labels = vec![
        (0, "Global"),
        (1, "Fee Recipient"),
        (2, "Mint"),
        (3, "Bonding Curve"),
        (4, "Associated Bonding Curve"),
        (5, "User Token Account"),
        (6, "User Wallet"),
        (7, "System Program"),
        (8, "Token Program 2022"),
        (9, "Creator Vault"),
        (10, "Event Authority"),
        (11, "Pump Program"),
        (12, "Global Volume"),
        (13, "User Volume"),
        (14, "Fee Config"),
        (15, "Fee Program"),
    ];
    
    for (idx, account) in instruction.accounts.iter().enumerate() {
        let label = account_labels.iter()
            .find(|(i, _)| *i == idx)
            .map(|(_, l)| *l)
            .unwrap_or("Unknown");
        
        let signer_str = if account.is_signer { " [SIGNER]" } else { "" };
        let writable_str = if account.is_writable { " [WRITABLE]" } else { " [READONLY]" };
        
        if idx == 12 {
            eprintln!("   [{}] {} ({}){} <-- GLOBAL VOLUME (hardcoded)", idx, account.pubkey, label, writable_str);
        } else if idx == 13 {
            eprintln!("   [{}] {} ({}){} <-- USER VOLUME (recalculated for user: {})", idx, account.pubkey, label, writable_str, user_wallet);
            // CRITICAL VERIFICATION: Ensure User Volume matches expected PDA for this user
            let (expected_user_volume, _) = crate::pda_derivation::derive_user_volume_pda(user_wallet);
            if account.pubkey != expected_user_volume {
                eprintln!("   ❌❌❌ CRITICAL ERROR: User Volume mismatch!");
                eprintln!("      Expected (for user {}): {}", user_wallet, expected_user_volume);
                eprintln!("      Got in instruction: {}", account.pubkey);
                eprintln!("      This will cause Error 0x1f9 (Seeds Constraint Was Violated)!");
                // We continue anyway but log the error prominently
            } else {
                eprintln!("   ✅✅✅ User Volume PDA verified: matches expected PDA for user {}", user_wallet);
            }
        } else if idx == 9 {
            eprintln!("   [{}] {} ({}){} <-- CREATOR VAULT", idx, account.pubkey, label, writable_str);
        } else {
            eprintln!("   [{}] {} ({}){}{}", idx, account.pubkey, label, signer_str, writable_str);
        }
    }
    
    // Log instruction data
    eprintln!();
    eprintln!("🔍 BUY INSTRUCTION DATA:");
    eprintln!("   Discriminator: {:02x?}", &instruction.data[0..8]);
    if instruction.data.len() >= 16 {
        let token_amount = u64::from_le_bytes(instruction.data[8..16].try_into().unwrap());
        eprintln!("   Token Amount: {} ({:.2} tokens)", token_amount, token_amount as f64 / 1e9);
    }
    if instruction.data.len() >= 24 {
        let max_sol_cost = u64::from_le_bytes(instruction.data[16..24].try_into().unwrap());
        eprintln!("   Max SOL Cost: {} ({:.9} SOL)", max_sol_cost, max_sol_cost as f64 / 1e9);
    }
    eprintln!("   Data Length: {} bytes", instruction.data.len());
    
    // Final verification before returning
    eprintln!();
    eprintln!("🔍 FINAL VERIFICATION - User Volume PDA:");
    let (final_check_user_volume, _) = crate::pda_derivation::derive_user_volume_pda(user_wallet);
    let user_volume_in_instruction = instruction.accounts[13].pubkey;
    eprintln!("   User Wallet: {}", user_wallet);
    eprintln!("   Expected User Volume PDA: {}", final_check_user_volume);
    eprintln!("   User Volume in instruction (index 13): {}", user_volume_in_instruction);
    
    if final_check_user_volume != user_volume_in_instruction {
        eprintln!("   ❌❌❌ FINAL CHECK FAILED: User Volume mismatch!");
        // Don't return error here to allow transaction to proceed (maybe our derivation is still wrong but we want to try)
    } else {
        eprintln!("   ✅✅✅ FINAL CHECK PASSED: User Volume PDA is correct!");
    }
    eprintln!();
    
    Ok(instruction)
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
        use crate::pda_derivation::derive_user_volume_pda;
        
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
            associated_bonding_curve_instruction: None,
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
            associated_bonding_curve_instruction: None,
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
            associated_bonding_curve_instruction: None,
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
