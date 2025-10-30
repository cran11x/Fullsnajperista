// constants.rs - Konstante i helper funkcije

use solana_sdk::pubkey::Pubkey;
use std::str::FromStr;

// ==================== PUMP.FUN CONSTANTS ====================

pub const PUMP_PROGRAM_ID: &str = "6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P";

// Static Pump.Fun accounts (hardkodirani)
pub const PUMP_GLOBAL: &str = "4wTV1YmiEkRvAtNtsSGPtUrqRYQMe5SKy2uB4Jjaxnjf";
pub const PUMP_FEE_RECIPIENT: &str = "62qc2CNXwrYqQScmEdiZFFAnJR262PxWEuNQtxfafNgV";
pub const PUMP_EVENT_AUTHORITY: &str = "Ce6TQqeHC9p8KetsN6JsjHK7UTZk7nasjjnr7XxXp9F1";
pub const PUMP_FEE_CONFIG: &str = "8Wf5TiAheLUqBrKXeYg2JtAFFMWtKdG2BSFgqUcPVwTt";
pub const PUMP_FEE_PROGRAM: &str = "pfeeUxB6jkeY1Hxd7CsFCAjcbHA9rWtchMGdZ6VojVZ";
pub const PUMP_GLOBAL_VOLUME: &str = "Hq2wp8uJ9jCPsYgNHex8RtqdvMPfVGoYwjvF1ATiwn2Y";

// Instruction discriminators
pub const BUY_DISCRIMINATOR: [u8; 8] = [0x66, 0x06, 0x3d, 0x12, 0x01, 0xda, 0xeb, 0xea];
pub const SELL_DISCRIMINATOR: [u8; 8] = [0x33, 0xe6, 0x85, 0xa4, 0x01, 0x7f, 0x83, 0xad];

// ==================== RPC ENDPOINTS ====================

pub const MAINNET_RPC: &str = "https://api.mainnet-beta.solana.com";
pub const MAINNET_WSS: &str = "wss://api.mainnet-beta.solana.com";

// Brži RPC endpoints (zahtijevaju API key)
pub const HELIUS_RPC: &str = "https://mainnet.helius-rpc.com/?api-key=YOUR_KEY";
pub const QUICKNODE_RPC: &str = "https://YOUR_ENDPOINT.quiknode.pro/YOUR_KEY/";

// ==================== JITO ====================

pub const JITO_MAINNET: &str = "https://mainnet.block-engine.jito.wtf/api/v1/bundles";
pub const JITO_TIP_ACCOUNTS: [&str; 8] = [
    "96gYZGLnJYVFmbjzopPSU6QiEV5fGqZNyN9nmNhvrZU5",
    "HFqU5x63VTqvQss8hp11i4wVV8bD44PvwucfZ2bU7gRe",
    "Cw8CFyM9FkoMi7K7Crf6HNQqf4uEMzpKw6QNghXLvLkY",
    "ADaUMid9yfUytqMBgopwjb2DTLSokTSzL1zt6iGPaS49",
    "DfXygSm4jCyNCybVYYK6DwvWqjKee8pbDmJGcLWNDXjh",
    "ADuUkR4vqLUMWXxW9gh6D6L8pMSawimctcNZ5pGwDcEt",
    "DttWaMuVvTiduZRnguLF7jNxTgiMBZ1hyAumKUiL2KRL",
    "3AVi9Tg9Uo68tJfuvoKvqKNWKkC5wPdSSdeBnizKZ6jT",
];

// ==================== TRADE SETTINGS ====================

// Compute units
pub const MIN_COMPUTE_UNITS: u32 = 200_000;
pub const RECOMMENDED_COMPUTE_UNITS: u32 = 300_000;
pub const MAX_COMPUTE_UNITS: u32 = 400_000;

// Priority fees (micro-lamports per CU)
pub const LOW_PRIORITY_FEE: u64 = 10_000;      // ~0.002 SOL
pub const MEDIUM_PRIORITY_FEE: u64 = 100_000;  // ~0.02 SOL
pub const HIGH_PRIORITY_FEE: u64 = 500_000;    // ~0.1 SOL
pub const TURBO_PRIORITY_FEE: u64 = 1_000_000; // ~0.2 SOL

// Jito tips
pub const LOW_JITO_TIP: u64 = 10_000;     // 0.00001 SOL
pub const MEDIUM_JITO_TIP: u64 = 50_000;  // 0.00005 SOL
pub const HIGH_JITO_TIP: u64 = 100_000;   // 0.0001 SOL

// Slippage (u bps - basis points)
pub const SLIPPAGE_1_PERCENT: u64 = 100;
pub const SLIPPAGE_5_PERCENT: u64 = 500;
pub const SLIPPAGE_10_PERCENT: u64 = 1000;

// ==================== HELPER FUNCTIONS ====================

/// Convert SOL to lamports
pub fn sol_to_lamports(sol: f64) -> u64 {
    (sol * 1_000_000_000.0) as u64
}

/// Convert lamports to SOL
pub fn lamports_to_sol(lamports: u64) -> f64 {
    lamports as f64 / 1_000_000_000.0
}

/// Get Pump.Fun program ID
pub fn pump_program_id() -> Pubkey {
    Pubkey::from_str(PUMP_PROGRAM_ID).unwrap()
}

/// Get static Pump.Fun accounts
pub struct PumpStaticAccounts {
    pub global: Pubkey,
    pub fee_recipient: Pubkey,
    pub event_authority: Pubkey,
    pub fee_config: Pubkey,
    pub fee_program: Pubkey,
    pub global_volume: Pubkey,
}

impl PumpStaticAccounts {
    pub fn load() -> Self {
        Self {
            global: Pubkey::from_str(PUMP_GLOBAL).unwrap(),
            fee_recipient: Pubkey::from_str(PUMP_FEE_RECIPIENT).unwrap(),
            event_authority: Pubkey::from_str(PUMP_EVENT_AUTHORITY).unwrap(),
            fee_config: Pubkey::from_str(PUMP_FEE_CONFIG).unwrap(),
            fee_program: Pubkey::from_str(PUMP_FEE_PROGRAM).unwrap(),
            global_volume: Pubkey::from_str(PUMP_GLOBAL_VOLUME).unwrap(),
        }
    }
}

/// Calculate bonding curve PDA
pub fn derive_bonding_curve(mint: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[b"bonding-curve", &mint.to_bytes()],
        &pump_program_id(),
    )
}

/// Calculate associated bonding curve (token account za BC)
pub fn derive_associated_bonding_curve(mint: &Pubkey, bonding_curve: &Pubkey) -> Pubkey {
    spl_associated_token_account::get_associated_token_address(bonding_curve, mint)
}

// ==================== FORMATTING ====================

/// Format pubkey za display (skraćeno)
pub fn format_pubkey(pubkey: &Pubkey) -> String {
    let s = pubkey.to_string();
    format!("{}...{}", &s[..4], &s[s.len()-4..])
}

/// Format SOL amount
pub fn format_sol(lamports: u64) -> String {
    format!("{:.4} SOL", lamports_to_sol(lamports))
}

// ==================== EXAMPLE USAGE ====================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sol_conversion() {
        assert_eq!(sol_to_lamports(1.0), 1_000_000_000);
        assert_eq!(lamports_to_sol(1_000_000_000), 1.0);
        assert_eq!(sol_to_lamports(0.01), 10_000_000);
    }

    #[test]
    fn test_derive_pda() {
        let mint = Pubkey::new_unique();
        let (bonding_curve, _bump) = derive_bonding_curve(&mint);
        println!("Bonding curve: {}", bonding_curve);
    }

    #[test]
    fn test_static_accounts() {
        let accounts = PumpStaticAccounts::load();
        println!("Global: {}", accounts.global);
        println!("Fee recipient: {}", accounts.fee_recipient);
    }
}