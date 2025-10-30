// buy.rs - Build Buy instrukcija za Pump.Fun

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

/// Build Pump.Fun Buy instrukciju sa tačnim redosledom account-a
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

    // TAČAN REDOSLED ACCOUNT-A (iz InitializeBondingCurve / Buy TX-a)
    let buy_ix = Instruction {
        program_id: pump_program,
        accounts: vec![
            AccountMeta::new(accounts.global, false),                           // #0
            AccountMeta::new(accounts.fee_recipient, false),                    // #1
            AccountMeta::new(accounts.mint, false),                             // #2
            AccountMeta::new(accounts.bonding_curve, false),                    // #3
            AccountMeta::new(accounts.associated_bonding_curve, false),         // #4
            AccountMeta::new(*user_token_account, false),                       // #5
            AccountMeta::new(*user_wallet, true),                               // #6 (signer)
            AccountMeta::new_readonly(system_program::id(), false),             // #7
            AccountMeta::new_readonly(spl_token::id(), false),                  // #8
            AccountMeta::new(accounts.creator_vault, false),                    // #9 (SYSTEM ACCOUNT!)
            AccountMeta::new(accounts.event_authority, false),                  // #10
            AccountMeta::new(accounts.global_volume, false),                    // #11
            AccountMeta::new(accounts.user_volume, false),                      // #12
            AccountMeta::new_readonly(accounts.fee_config, false),              // #13
            AccountMeta::new_readonly(accounts.fee_program, false),             // #14
        ],
        data,
    };

    Ok(buy_ix)
}

/// Build Buy instrukciju sa custom min token amount (za limit order)
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
            AccountMeta::new(accounts.global_volume, false),
            AccountMeta::new(accounts.user_volume, false),
            AccountMeta::new_readonly(accounts.fee_config, false),
            AccountMeta::new_readonly(accounts.fee_program, false),
        ],
        data,
    };

    Ok(buy_ix)
}