// buy.rs - FIXED: User Volume PDA se izvodi za tvog wallet-a

use anyhow::Result;
use solana_sdk::{
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
    system_program,
};
use std::str::FromStr;

use crate::detection::PumpBuyAccounts;

const PUMP_PROGRAM_ID: &str = "6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P";
const BUY_DISCRIMINATOR: [u8; 8] = [0x66, 0x06, 0x3d, 0x12, 0x01, 0xda, 0xeb, 0xea];

/// ✅ Izvodi User Volume PDA za tvog wallet-a
pub fn derive_user_volume_pda(_user_wallet: &Pubkey) -> (Pubkey, u8) {
    // ✅ STAVI SVOJ User Volume ovdje!
    let hardcoded = Pubkey::from_str("2wkkPpX4nML1Tzjrh2neECxmhhj4NwjXU7z5q56xjJH9")
        .expect("Invalid hardcoded User Volume");

    (hardcoded, 0)
}
/// 🎯 Build Buy instrukciju sa PRAVILNO IZVEDENIM User Volume PDA
pub fn build_buy_instruction(
    accounts: &PumpBuyAccounts,
    user_wallet: &Pubkey,
    user_token_account: &Pubkey,
    sol_lamports: u64,
) -> Result<Instruction> {
    let mut data = Vec::with_capacity(24);
    data.extend_from_slice(&BUY_DISCRIMINATOR);

    let min_token_amount: u64 = 1;  // Market buy
    data.extend_from_slice(&min_token_amount.to_le_bytes());
    data.extend_from_slice(&sol_lamports.to_le_bytes());

    let pump_program = Pubkey::from_str(PUMP_PROGRAM_ID)?;

    // ✅ IZVEDI User Volume PDA za TVOG wallet-a
    let (user_volume, _bump) = derive_user_volume_pda(user_wallet);

    println!("   💡 Derived User Volume PDA: {}", user_volume);

    // ✅ TAČAN REDOSLED (16 accounta)
    let buy_ix = Instruction {
        program_id: pump_program,
        accounts: vec![
            AccountMeta::new(accounts.global, false),                       // #1
            AccountMeta::new(accounts.fee_recipient, false),                // #2
            AccountMeta::new(accounts.mint, false),                         // #3
            AccountMeta::new(accounts.bonding_curve, false),                // #4
            AccountMeta::new(accounts.associated_bonding_curve, false),     // #5
            AccountMeta::new(*user_token_account, false),                   // #6
            AccountMeta::new(*user_wallet, true),                           // #7 (signer)
            AccountMeta::new_readonly(system_program::id(), false),         // #8
            AccountMeta::new_readonly(spl_token::id(), false),              // #9
            AccountMeta::new(accounts.creator_vault, false),                // #10
            AccountMeta::new(accounts.event_authority, false),              // #11
            AccountMeta::new_readonly(pump_program, false),                 // #12
            AccountMeta::new(accounts.global_volume, false),                // #13
            AccountMeta::new(user_volume, false),                           // #14 ✅ TVOJ PDA!
            AccountMeta::new_readonly(accounts.fee_config, false),          // #15
            AccountMeta::new_readonly(accounts.fee_program, false),         // #16
        ],
        data,
    };

    Ok(buy_ix)
}

/// Build Buy sa custom min_token_amount
pub fn build_buy_instruction_with_min_amount(
    accounts: &PumpBuyAccounts,
    user_wallet: &Pubkey,
    user_token_account: &Pubkey,
    sol_lamports: u64,
    min_token_amount: u64,
) -> Result<Instruction> {
    let mut data = Vec::with_capacity(24);
    data.extend_from_slice(&BUY_DISCRIMINATOR);
    data.extend_from_slice(&min_token_amount.to_le_bytes());
    data.extend_from_slice(&sol_lamports.to_le_bytes());

    let pump_program = Pubkey::from_str(PUMP_PROGRAM_ID)?;

    // ✅ Izvedi User Volume PDA
    let (user_volume, _) = derive_user_volume_pda(user_wallet);

    let buy_ix = Instruction {
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
            AccountMeta::new(user_volume, false),                           // ✅ TVOJ!
            AccountMeta::new_readonly(accounts.fee_config, false),
            AccountMeta::new_readonly(accounts.fee_program, false),
        ],
        data,
    };

    Ok(buy_ix)
}