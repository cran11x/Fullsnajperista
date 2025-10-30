// lib.rs - Glavni modul

pub mod detection;
pub mod buy;
pub mod constants;

// Re-export najvažnije tipove
pub use detection::PumpBuyAccounts;
pub use buy::{build_buy_instruction, build_buy_instruction_with_min_amount};
pub use constants::*;