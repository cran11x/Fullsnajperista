// sell.rs - SELL INSTRUCTION BUILDER
// Updated: Account structure matches new Sell instruction format (14 accounts: Global, Fee Recipient, Mint, Bonding Curve, Associated Bonding Curve, Associated User, User, System Program, Creator Vault, Token Program, Event Authority, Pump Program, Fee Config, Fee Program)

use anyhow::Result;
use solana_sdk::{
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
    system_program,
};
use std::str::FromStr;

use crate::constants::{PUMP_PROGRAM_ID, SELL_DISCRIMINATOR};
use crate::detection::PumpBuyAccounts;
use crate::pda_derivation::PumpPdas;

/// Validate sell instruction parameters
fn validate_sell_params(
    _accounts: &PumpBuyAccounts,
    user_wallet: &Pubkey,
    user_token_account: &Pubkey,
    token_amount: u64,
) -> Result<()> {
    if token_amount == 0 {
        return Err(anyhow::anyhow!("Token amount must be > 0"));
    }

    if user_wallet == user_token_account {
        return Err(anyhow::anyhow!("User wallet and token account cannot be the same"));
    }

    Ok(())
}

/// Build sell instruction for pump.fun tokens
pub async fn build_sell_instruction(
    accounts: &PumpBuyAccounts,
    user_wallet: &Pubkey,
    user_token_account: &Pubkey,
    token_amount: u64, // Amount of tokens to sell
) -> Result<Instruction> {
    // Validate parameters
    validate_sell_params(accounts, user_wallet, user_token_account, token_amount)?;

    let pump_program = Pubkey::from_str(PUMP_PROGRAM_ID)
        .map_err(|e| anyhow::anyhow!("Invalid PUMP_PROGRAM_ID: {}", e))?;
    
    // 🔥 CRITICAL FIX: Recalculate ALL PDAs fresh for this specific mint/user
    // DO NOT reuse values from accounts - they might be from a different token or stale
    let pdas = PumpPdas::recalculate_all(&accounts.mint, user_wallet);

    // Build instruction data: discriminator + token_amount (u64) + min_sol_out (u64) + 1 byte
    // min_sol_out: 0 (accept any amount - better to sell than fail)
    let mut data = Vec::with_capacity(32);
    data.extend_from_slice(&SELL_DISCRIMINATOR);
    data.extend_from_slice(&token_amount.to_le_bytes());
    data.extend_from_slice(&0u64.to_le_bytes()); // min_sol_out = 0
    // data.push(0x00); // Removed extra byte

    // CRITICAL: Use RECALCULATED PDAs, not values from accounts
    // Account order updated to match new Sell instruction structure (14 accounts):
    // 0: Global
    // 1: Fee Recipient (Writable)
    // 2: Mint
    // 3: Bonding Curve (Writable)
    // 4: Associated Bonding Curve (Writable)
    // 5: Associated User / User Token Account (Writable)
    // 6: User Wallet (Writable, Signer, FP)
    // 7: System Program
    // 8: Creator Vault (Writable)
    // 9: Token Program 2022 (Program)
    // 10: Event Authority
    // 11: Pump.fun Program (Program)
    // 12: Fee Config (Writable)
    // 13: Fee Program (Program)

    // Creator Vault (Account 8) - use directly from accounts
    // Already properly set in bot_core.rs from the original buy transaction
    // DO NOT re-derive - use the exact same Creator Vault from the buy transaction
    let final_creator_vault = accounts.creator_vault;

    // Use Token Program 2022 by default as Pump.fun uses it, or fallback to standard
    let token_program_2022_id = Pubkey::from_str("TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb")
        .unwrap_or_else(|_| spl_token::id());

    // Ensure Associated Bonding Curve is consistent with program
    let recalculate_abc = spl_associated_token_account::get_associated_token_address_with_program_id(
        &pdas.bonding_curve,
        &accounts.mint,
        &token_program_2022_id
    );

    let instruction = Instruction {
        program_id: pump_program,
        accounts: vec![
            // Account 0: Global (PDA, RECALCULATED)
            AccountMeta::new(pdas.global, false),
            // Account 1: Fee Recipient (hardcoded, Writable)
            AccountMeta::new(pdas.fee_recipient, false),
            // Account 2: Mint (from accounts)
            AccountMeta::new_readonly(accounts.mint, false),
            // Account 3: Bonding Curve (PDA, RECALCULATED, Writable)
            AccountMeta::new(pdas.bonding_curve, false),
            // Account 4: Associated Bonding Curve (RECALCULATED, Writable)
            AccountMeta::new(recalculate_abc, false),
            // Account 5: Associated User / User Token Account (Writable)
            AccountMeta::new(*user_token_account, false),
            // Account 6: User Wallet (Writable, Signer)
            AccountMeta::new(*user_wallet, true),
            // Account 7: System Program
            AccountMeta::new_readonly(system_program::id(), false),
            // Account 8: Creator Vault (Writable)
            AccountMeta::new(final_creator_vault, false),
            // Account 9: Token Program 2022 (Program)
            AccountMeta::new_readonly(token_program_2022_id, false),
            // Account 10: Event Authority (PDA, RECALCULATED)
            AccountMeta::new_readonly(pdas.event_authority, false),
            // Account 11: Pump.fun Program (Program)
            AccountMeta::new_readonly(pump_program, false),
            // Account 12: Fee Config (Writable)
            AccountMeta::new(pdas.fee_config, false),
            // Account 13: Fee Program (Program - WRITABLE as per successful transaction)
            AccountMeta::new(pdas.fee_program, false),
        ],
        data,
    };

    // Debug: Log sell instruction details
    
    for (_idx, _account) in instruction.accounts.iter().enumerate() {
        // Account details removed for cleaner output
    }
    
    Ok(instruction)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_sell_params() {
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
        assert!(validate_sell_params(&accounts, &user_wallet, &user_token_account, 1000).is_ok());

        // Invalid: zero tokens
        assert!(validate_sell_params(&accounts, &user_wallet, &user_token_account, 0).is_err());

        // Invalid: same wallet and token account
        assert!(validate_sell_params(&accounts, &user_wallet, &user_wallet, 1000).is_err());
    }

    #[tokio::test]
    async fn test_build_sell_instruction() {
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

        let instruction = build_sell_instruction(
            &accounts,
            &user_wallet,
            &user_token_account,
            1000, // tokens
        ).await;

        assert!(instruction.is_ok());
        let ix = instruction.unwrap();

        // Verify instruction structure
        assert_eq!(ix.program_id, Pubkey::from_str(PUMP_PROGRAM_ID).unwrap());
        assert_eq!(ix.accounts.len(), 14); // Updated to match real transactions

        // Verify discriminator in data
        assert_eq!(ix.data[0..8], SELL_DISCRIMINATOR);

        // Verify user wallet is signer
        let user_wallet_meta = ix.accounts.iter()
            .find(|meta| meta.pubkey == user_wallet)
            .unwrap();
        assert!(user_wallet_meta.is_signer);
    }

    #[tokio::test]
    async fn test_build_sell_instruction_validation_errors() {
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

        // Test with zero tokens - should fail validation
        let result = build_sell_instruction(
            &accounts,
            &user_wallet,
            &user_token_account,
            0,
        ).await;
        assert!(result.is_err(), "Expected error for zero token amount");
    }

    #[tokio::test]
    async fn test_build_sell_instruction_account_order() {
        // Test that account order is correct (14 accounts)
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

        let instruction = build_sell_instruction(
            &accounts,
            &user_wallet,
            &user_token_account,
            1000,
        ).await.unwrap();

        // Verify instruction structure - 14 accounts expected
        assert_eq!(instruction.accounts.len(), 14, "Should have 14 accounts");
        
        // Account 0: Global (should not be signer)
        assert!(!instruction.accounts[0].is_signer, "Account 0 (Global) should not be signer");
        
        // Account 6: User Wallet (should be signer)
        assert!(instruction.accounts[6].is_signer, "Account 6 (User Wallet) should be signer");
        assert_eq!(instruction.accounts[6].pubkey, user_wallet, "Account 6 should be user_wallet");
        
        // Account 2: Mint (should be readonly)
        assert!(!instruction.accounts[2].is_signer, "Account 2 (Mint) should not be signer");
        assert_eq!(instruction.accounts[2].pubkey, accounts.mint, "Account 2 should be mint");
        
        // Account 5: User Token Account
        assert_eq!(instruction.accounts[5].pubkey, user_token_account, "Account 5 should be user_token_account");
        
        // Account 8: Creator Vault (should use the one from accounts)
        assert_eq!(instruction.accounts[8].pubkey, accounts.creator_vault, "Account 8 should be creator_vault from accounts");
    }

    #[tokio::test]
    async fn test_build_sell_instruction_creator_vault_handling() {
        // Test that creator vault is properly handled
        let creator = Pubkey::new_unique();
        let creator_vault = Pubkey::new_unique();
        
        let accounts = crate::detection::PumpBuyAccounts {
            mint: Pubkey::new_unique(),
            bonding_curve: Pubkey::new_unique(),
            associated_bonding_curve: Pubkey::new_unique(),
            creator_vault, // Use specific creator vault
            event_authority: Pubkey::new_unique(),
            global_volume: Pubkey::new_unique(),
            global: Pubkey::new_unique(),
            fee_recipient: Pubkey::new_unique(),
            fee_config: Pubkey::new_unique(),
            fee_program: Pubkey::new_unique(),
            dev_buy_sol: 0,
            creator,
            associated_bonding_curve_instruction: None,
        };

        let user_wallet = Pubkey::new_unique();
        let user_token_account = Pubkey::new_unique();

        let instruction = build_sell_instruction(
            &accounts,
            &user_wallet,
            &user_token_account,
            1000,
        ).await.unwrap();

        // Verify creator vault is used from accounts (Account 8)
        assert_eq!(instruction.accounts[8].pubkey, creator_vault, "Creator vault should match accounts.creator_vault");
        assert!(!instruction.accounts[8].is_signer, "Creator vault should not be signer");
    }

    #[tokio::test]
    async fn test_build_sell_instruction_data_format() {
        // Test that instruction data is properly formatted
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
        let token_amount = 1_000_000; // 1 token (6 decimals)

        let instruction = build_sell_instruction(
            &accounts,
            &user_wallet,
            &user_token_account,
            token_amount,
        ).await.unwrap();

        // Verify discriminator in data
        assert_eq!(instruction.data[0..8], SELL_DISCRIMINATOR, "First 8 bytes should be SELL_DISCRIMINATOR");
        
        // Verify token_amount in data (bytes 8-16)
        let token_amount_bytes = &instruction.data[8..16];
        let extracted_token_amount = u64::from_le_bytes([
            token_amount_bytes[0],
            token_amount_bytes[1],
            token_amount_bytes[2],
            token_amount_bytes[3],
            token_amount_bytes[4],
            token_amount_bytes[5],
            token_amount_bytes[6],
            token_amount_bytes[7],
        ]);
        assert_eq!(extracted_token_amount, token_amount, "Token amount should match");
        
        // Verify min_sol_out is 0 (bytes 16-24)
        let min_sol_out_bytes = &instruction.data[16..24];
        let min_sol_out = u64::from_le_bytes([
            min_sol_out_bytes[0],
            min_sol_out_bytes[1],
            min_sol_out_bytes[2],
            min_sol_out_bytes[3],
            min_sol_out_bytes[4],
            min_sol_out_bytes[5],
            min_sol_out_bytes[6],
            min_sol_out_bytes[7],
        ]);
        assert_eq!(min_sol_out, 0, "min_sol_out should be 0 (accept any amount)");
        
        // Verify data length
        assert_eq!(instruction.data.len(), 24, "Instruction data should be 24 bytes");
    }

    #[tokio::test]
    async fn test_build_sell_instruction_pda_recalculation() {
        // Test that PDAs are recalculated correctly
        let mint = Pubkey::new_unique();
        let user_wallet = Pubkey::new_unique();
        
        let accounts = crate::detection::PumpBuyAccounts {
            mint,
            bonding_curve: Pubkey::new_unique(), // This will be ignored, PDA will be recalculated
            associated_bonding_curve: Pubkey::new_unique(),
            creator_vault: Pubkey::new_unique(),
            event_authority: Pubkey::new_unique(), // This will be ignored, PDA will be recalculated
            global_volume: Pubkey::new_unique(),
            global: Pubkey::new_unique(), // This will be ignored, PDA will be recalculated
            fee_recipient: Pubkey::new_unique(),
            fee_config: Pubkey::new_unique(),
            fee_program: Pubkey::new_unique(),
            dev_buy_sol: 0,
            creator: Pubkey::new_unique(),
            associated_bonding_curve_instruction: None,
        };

        let user_token_account = Pubkey::new_unique();

        let instruction = build_sell_instruction(
            &accounts,
            &user_wallet,
            &user_token_account,
            1000,
        ).await.unwrap();

        // Verify that recalculated PDAs are used
        let pdas = crate::pda_derivation::PumpPdas::recalculate_all(&mint, &user_wallet);
        
        // Account 0: Global (should match recalculated PDA)
        assert_eq!(instruction.accounts[0].pubkey, pdas.global, "Account 0 (Global) should match recalculated PDA");
        
        // Account 3: Bonding Curve (should match recalculated PDA)
        assert_eq!(instruction.accounts[3].pubkey, pdas.bonding_curve, "Account 3 (Bonding Curve) should match recalculated PDA");
        
        // Account 10: Event Authority (should match recalculated PDA)
        assert_eq!(instruction.accounts[10].pubkey, pdas.event_authority, "Account 10 (Event Authority) should match recalculated PDA");
    }
}
