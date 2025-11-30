// constants.rs - CENTRALIZED CONSTANTS FOR PUMP.FUN SNIPER BOT
#![allow(unused, dead_code)]

use solana_sdk::pubkey::Pubkey;
use std::str::FromStr;

/// Pump.fun Program ID
pub const PUMP_PROGRAM_ID: &str = "6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P";

/// Buy instruction discriminator
pub const BUY_DISCRIMINATOR: [u8; 8] = [0x66, 0x06, 0x3d, 0x12, 0x01, 0xda, 0xeb, 0xea];

/// Sell instruction discriminator
pub const SELL_DISCRIMINATOR: [u8; 8] = [0x33, 0xe6, 0x85, 0x57, 0x77, 0x35, 0x8a, 0x92];

/// Global Volume Leaderboard (Account 13 in Buy instruction)
/// Used instead of User Volume PDA in recent program versions
pub const GLOBAL_VOLUME_LEADERBOARD: &str = "4o2mH9Fwq56UhD1nRa3mVsy8y1TeBPD8WoDzGmZ6DZC7";

/// Jito tip accounts for MEV protection
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

/// Jito block engine endpoints
pub const JITO_ENDPOINTS: [&str; 4] = [
    "https://ny.mainnet.block-engine.jito.wtf",
    "https://mainnet.block-engine.jito.wtf",
    "https://frankfurt.mainnet.block-engine.jito.wtf",
    "https://tokyo.mainnet.block-engine.jito.wtf",
];

/// Helius tip accounts (legacy, may not be used)
pub const HELIUS_TIP_ACCOUNTS: [&str; 10] = [
    "4ACfpUFoaSD9bfPdeu6DBt89gB6ENTeHBXCAi87NhDEE",
    "D2L6yPZ2FmmmTKPgzaMKdhu6EWZcTpLy1Vhx8uvZe7NZ",
    "9bnz4RShgq1hAnLnZbP8kbgBg1kEmcJBYQq3gQbmnSta",
    "5VY91ws6B2hMmBFRsXkoAAdsPHBJwRfBht4DXox3xkwn",
    "2nyhqdwKcJZR2vcqCyrYsaPVdAnFoJjiksCXJ7hfEYgD",
    "2q5pghRs6arqVjRvT5gfgWfWcHWmw1ZuCzphgd5KfWGJ",
    "wyvPkWjVZz1M8fHQnMMCDTQDbkManefNNhweYk5WkcF",
    "3KCKozbAaF75qEU33jtzozcJ29yJuaLJTy2jFdzUY8bT",
    "4vieeGHPYPG2MmyPRcYjdiDmmhN3ww7hsFNap8pVN3Ey",
    "4TQLFNWK8AovT1gFvda5jfw2oJeRMKEmw7aH6MGBJ3or",
];

/// Helius fast sender endpoint
pub const SENDER_ENDPOINT: &str = "https://sender.helius-rpc.com/fast";

/// Helper function to get Pump Program ID as Pubkey
pub fn pump_program_id() -> Pubkey {
    Pubkey::from_str(PUMP_PROGRAM_ID).expect("Invalid PUMP_PROGRAM_ID constant")
}

/// Helper function to get a random Jito tip account as Pubkey
pub fn random_jito_tip_account() -> Pubkey {
    use rand::seq::SliceRandom;
    let mut rng = rand::thread_rng();
    let account_str = JITO_TIP_ACCOUNTS.choose(&mut rng).unwrap();
    Pubkey::from_str(account_str).expect("Invalid JITO_TIP_ACCOUNT")
}

/// Helper function to get a random Jito endpoint
pub fn random_jito_endpoint() -> &'static str {
    use rand::seq::SliceRandom;
    let mut rng = rand::thread_rng();
    JITO_ENDPOINTS.choose(&mut rng).unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pump_program_id_valid() {
        let pubkey = pump_program_id();
        assert_eq!(pubkey.to_string(), PUMP_PROGRAM_ID);
    }

    #[test]
    fn test_buy_discriminator_length() {
        assert_eq!(BUY_DISCRIMINATOR.len(), 8);
    }

    #[test]
    fn test_sell_discriminator_length() {
        assert_eq!(SELL_DISCRIMINATOR.len(), 8);
    }

    #[test]
    fn test_jito_tip_accounts_valid() {
        for account_str in JITO_TIP_ACCOUNTS.iter() {
            let pubkey = Pubkey::from_str(account_str);
            assert!(pubkey.is_ok(), "Invalid Jito tip account: {}", account_str);
        }
    }

    #[test]
    fn test_jito_endpoints_valid() {
        for endpoint in JITO_ENDPOINTS.iter() {
            assert!(endpoint.starts_with("https://"), "Invalid endpoint: {}", endpoint);
        }
    }
}
