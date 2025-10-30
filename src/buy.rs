// buy.rs - USE GIT REPO

use anyhow::Result;
use solana_sdk::{
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
    system_program,
};
use solana_client::nonblocking::rpc_client::RpcClient;
use borsh::BorshDeserialize;
use std::str::FromStr;
use std::sync::Arc;
use tokio::sync::OnceCell;

use crate::detection::PumpBuyAccounts;

// ✅ Import from git repo files you sent
use crate::accounts::GlobalAccount;

const PUMP_PROGRAM_ID: &str = "6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P";
const BUY_DISCRIMINATOR: [u8; 8] = [0x66, 0x06, 0x3d, 0x12, 0x01, 0xda, 0xeb, 0xea];
const GLOBAL_ACCOUNT: &str = "4wTV1YmiEkRvAtNtsSGPtUrqRYQMe5SKy2uB4Jjaxnjf";

pub fn derive_user_volume_pda(user_wallet: &Pubkey) -> (Pubkey, u8) {
    let pump_program = Pubkey::from_str(PUMP_PROGRAM_ID).expect("Invalid pump program");
    Pubkey::find_program_address(
        &[b"user_volume_accumulator", user_wallet.as_ref()],
        &pump_program,
    )
}

pub async fn calculate_initial_buy_amount(
    rpc: &RpcClient,
    sol_lamports: u64,
) -> Result<u64> {
    let global_pubkey = Pubkey::from_str(GLOBAL_ACCOUNT)?;
    let global_data = rpc.get_account_data(&global_pubkey).await?;
    let global: GlobalAccount = BorshDeserialize::deserialize(&mut &global_data[..])?;
    Ok(global.get_initial_buy_price(sol_lamports))
}

static GLOBAL_CACHE: OnceCell<GlobalAccount> = OnceCell::const_new();

pub async fn get_or_fetch_global(rpc: &RpcClient) -> Result<GlobalAccount> {
    GLOBAL_CACHE.get_or_try_init(|| async {
        let global_pubkey = Pubkey::from_str(GLOBAL_ACCOUNT)?;
        let global_data = rpc.get_account_data(&global_pubkey).await?;
        let global: GlobalAccount = BorshDeserialize::deserialize(&mut &global_data[..])?;
        Ok::<_, anyhow::Error>(global)
    }).await.cloned()
}

pub async fn build_buy_instruction(
    rpc: &RpcClient,
    accounts: &PumpBuyAccounts,
    user_wallet: &Pubkey,
    user_token_account: &Pubkey,
    sol_lamports: u64,
) -> Result<Instruction> {
    // ✅ Use cached global (1 fetch on first run, then instant)
    let global = get_or_fetch_global(rpc).await?;
    let token_amount = global.get_initial_buy_price(sol_lamports);

    let max_sol_cost = (sol_lamports as u128 * 150 / 100) as u64;  // 50% slippage

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