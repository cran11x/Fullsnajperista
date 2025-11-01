// buy.rs - ULTRA OPTIMIZED WITH CACHE

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

const PUMP_PROGRAM_ID: &str = "6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P";
const BUY_DISCRIMINATOR: [u8; 8] = [0x66, 0x06, 0x3d, 0x12, 0x01, 0xda, 0xeb, 0xea];
const GLOBAL_ACCOUNT: &str = "4wTV1YmiEkRvAtNtsSGPtUrqRYQMe5SKy2uB4Jjaxnjf";

// 🚀 GLOBAL CACHE - ONE FETCH AT STARTUP
static GLOBAL_CACHE: OnceLock<GlobalAccount> = OnceLock::new();

/// Pre-load global account at startup (call once)
pub async fn preload_global(rpc: &RpcClient) -> Result<()> {
    let global_pubkey = Pubkey::from_str(GLOBAL_ACCOUNT)?;
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
    let pump_program = Pubkey::from_str(PUMP_PROGRAM_ID).expect("Invalid pump program");
    Pubkey::find_program_address(
        &[b"user_volume_accumulator", user_wallet.as_ref()],
        &pump_program,
    )
}

pub async fn build_buy_instruction(
    _rpc: &RpcClient, // ⚡ Not used anymore - uses cache
    accounts: &PumpBuyAccounts,
    user_wallet: &Pubkey,
    user_token_account: &Pubkey,
    sol_lamports: u64,
) -> Result<Instruction> {
    // ⚡ INSTANT - no RPC call
    let global = get_cached_global()?;
    let token_amount = global.get_initial_buy_price(sol_lamports);

    // ⚡ 20% slippage for speed (agresivno)
    let max_sol_cost = (sol_lamports as u128 * 120 / 100) as u64;

    println!("   💰 {} tokens for {} SOL (max: {})",
             token_amount,
             sol_lamports as f64 / 1e9,
             max_sol_cost as f64 / 1e9);

    let mut data = Vec::with_capacity(32);
    data.extend_from_slice(&BUY_DISCRIMINATOR);
    data.extend_from_slice(&token_amount.to_le_bytes());
    data.extend_from_slice(&max_sol_cost.to_le_bytes());
    data.push(0x00);

    let pump_program = Pubkey::from_str(PUMP_PROGRAM_ID)?;
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