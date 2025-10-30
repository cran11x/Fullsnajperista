// lib.rs - CLEAN

pub mod detection;
pub mod buy;
pub mod constants;
pub mod accounts;  // ✅ Sada accounts/ folder postoji

// Re-export
pub use detection::PumpBuyAccounts;
pub use buy::build_buy_instruction;
pub use constants::*;