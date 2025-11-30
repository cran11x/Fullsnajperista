// sell.rs - SELL INSTRUCTION BUILDER

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
    accounts: &PumpBuyAccounts,
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
    Ok(Instruction {
        program_id: pump_program,
        accounts: vec![
            // Account 0: Global (PDA, RECALCULATED)
            AccountMeta::new(pdas.global, false),
            // Account 1: Fee Recipient (hardcoded)
            AccountMeta::new(pdas.fee_recipient, false),
            // Account 2: Mint (from accounts - this is correct, it's the token mint)
            AccountMeta::new(accounts.mint, false),
            // Account 3: Bonding Curve (PDA, RECALCULATED for current mint)
            AccountMeta::new(pdas.bonding_curve, false),
            // Account 4: Associated Bonding Curve (from accounts - should be correct)
            AccountMeta::new(accounts.associated_bonding_curve, false),
            // Account 5: User Token Account (current user's token account)
            AccountMeta::new(*user_token_account, false),
            // Account 6: User Wallet (signer, current user)
            AccountMeta::new(*user_wallet, true),
            // Account 7: System Program (readonly)
            AccountMeta::new_readonly(system_program::id(), false),
            // Account 8: Token Program (readonly)
            AccountMeta::new_readonly(spl_token::id(), false),
            // Account 9: Creator Vault (from accounts - extracted from BUY instruction, should be correct)
            AccountMeta::new(accounts.creator_vault, false),
            // Account 10: Event Authority (PDA, RECALCULATED)
            AccountMeta::new(pdas.event_authority, false),
            // Account 11: Pump Program (readonly, program itself)
            AccountMeta::new_readonly(pump_program, false),
            // Account 12: Global Volume Accumulator (hardcoded)
            AccountMeta::new(pdas.global_volume, false),
            // Account 13: User Volume (PDA, RECALCULATED for current user)
            AccountMeta::new(pdas.user_volume, false),
            // Account 14: Fee Config (hardcoded)
            AccountMeta::new_readonly(pdas.fee_config, false),
            // Account 15: Fee Program (hardcoded)
            AccountMeta::new_readonly(pdas.fee_program, false),
        ],
        data,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(ix.accounts.len(), 16);

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
}

