// pda_derivation.rs - PDA DERIVATION FUNCTIONS FOR PUMP.FUN
// This module contains all PDA derivation functions that can be tested independently
// before integration into the main buy/sell flow.

use solana_sdk::pubkey::Pubkey;
use std::str::FromStr;
use crate::constants::PUMP_PROGRAM_ID;

/// Derive Global PDA (Account 0 in Buy instruction)
/// Seeds: ["global"]
pub fn derive_global_pda() -> (Pubkey, u8) {
    let pump_program = Pubkey::from_str(PUMP_PROGRAM_ID)
        .expect("Invalid PUMP_PROGRAM_ID constant");
    Pubkey::find_program_address(&[b"global"], &pump_program)
}

/// Derive Bonding Curve PDA (Account 3 in Buy instruction)
/// Seeds: ["bonding-curve", mint.to_bytes()]
pub fn derive_bonding_curve_pda(mint: &Pubkey) -> (Pubkey, u8) {
    let pump_program = Pubkey::from_str(PUMP_PROGRAM_ID)
        .expect("Invalid PUMP_PROGRAM_ID constant");
    Pubkey::find_program_address(
        &[b"bonding-curve", &mint.to_bytes()],
        &pump_program,
    )
}

/// Derive Event Authority PDA (Account 10 in Buy instruction)
/// Seeds: ["__event_authority"]
pub fn derive_event_authority_pda() -> (Pubkey, u8) {
    let pump_program = Pubkey::from_str(PUMP_PROGRAM_ID)
        .expect("Invalid PUMP_PROGRAM_ID constant");
    Pubkey::find_program_address(
        &[b"__event_authority"],
        &pump_program,
    )
}

/// Derive User Volume PDA (Account 13 in Buy instruction)
/// Seeds: ["user-trade-history", user_wallet.as_ref()]
pub fn derive_user_volume_pda(user_wallet: &Pubkey) -> (Pubkey, u8) {
    let pump_program = Pubkey::from_str(PUMP_PROGRAM_ID)
        .expect("Invalid PUMP_PROGRAM_ID constant");
    Pubkey::find_program_address(
        &[b"user-trade-history", user_wallet.as_ref()],
        &pump_program,
    )
}

/// Get hardcoded Global Volume Accumulator address
/// This is NOT a PDA - it's a fixed address
pub fn get_global_volume_address() -> Pubkey {
    Pubkey::from_str("Hq2wp8uJ9jCPsYgNHex8RtqdvMPfVGoYwjvF1ATiwn2Y")
        .expect("Invalid hardcoded Global Volume address")
}

/// Get hardcoded Fee Recipient address
pub fn get_fee_recipient_address() -> Pubkey {
    Pubkey::from_str("CebN5WGQ4jvEPvsVU4EoHEpgzq1VV7AbicfhtW4xC9iM")
        .expect("Invalid fee recipient address")
}

/// Get hardcoded Fee Config address
pub fn get_fee_config_address() -> Pubkey {
    Pubkey::from_str("8Wf5TiAheLUqBrKXeYg2JtAFFMWtKdG2BSFgqUcPVwTt")
        .expect("Invalid fee config address")
}

/// Get hardcoded Fee Program address
pub fn get_fee_program_address() -> Pubkey {
    Pubkey::from_str("pfeeUxB6jkeY1Hxd7CsFCAjcbHA9rWtchMGdZ6VojVZ")
        .expect("Invalid fee program address")
}

/// Get hardcoded Global account address
pub fn get_global_account_address() -> Pubkey {
    Pubkey::from_str("4wTV1YmiEkRvAtNtsSGPtUrqRYQMe5SKy2uB4Jjaxnjf")
        .expect("Invalid global account address")
}

/// Get hardcoded Event Authority address (for comparison)
pub fn get_event_authority_address() -> Pubkey {
    Pubkey::from_str("Ce6TQqeHC9p8KetsN6JsjHK7UTZk7nasjjnr7XxXp9F1")
        .expect("Invalid event authority address")
}

/// Structure containing all recalculated PDAs for a buy instruction
#[derive(Debug, Clone)]
pub struct PumpPdas {
    pub global: Pubkey,
    pub bonding_curve: Pubkey,
    pub event_authority: Pubkey,
    pub user_volume: Pubkey,
    pub global_volume: Pubkey,
    pub fee_recipient: Pubkey,
    pub fee_config: Pubkey,
    pub fee_program: Pubkey,
}

impl PumpPdas {
    /// Recalculate all PDAs for a specific mint and user
    /// This ensures all PDAs are fresh and correct for the current transaction
    pub fn recalculate_all(mint: &Pubkey, user_wallet: &Pubkey) -> Self {
        let (global, _) = derive_global_pda();
        let (bonding_curve, _) = derive_bonding_curve_pda(mint);
        let (event_authority, _) = derive_event_authority_pda();
        let (user_volume, _) = derive_user_volume_pda(user_wallet);
        
        Self {
            global,
            bonding_curve,
            event_authority,
            user_volume,
            global_volume: get_global_volume_address(),
            fee_recipient: get_fee_recipient_address(),
            fee_config: get_fee_config_address(),
            fee_program: get_fee_program_address(),
        }
    }
    
    /// Verify that recalculated PDAs match expected values
    /// Returns a report of any mismatches
    pub fn verify_against_expected(
        &self,
        expected_global: &Pubkey,
        expected_bonding_curve: &Pubkey,
        expected_event_authority: &Pubkey,
    ) -> PdaVerificationReport {
        PdaVerificationReport {
            global_matches: self.global == *expected_global,
            bonding_curve_matches: self.bonding_curve == *expected_bonding_curve,
            event_authority_matches: self.event_authority == *expected_event_authority,
        }
    }
}

/// Report of PDA verification results
#[derive(Debug)]
pub struct PdaVerificationReport {
    pub global_matches: bool,
    pub bonding_curve_matches: bool,
    pub event_authority_matches: bool,
}

impl PdaVerificationReport {
    pub fn all_match(&self) -> bool {
        self.global_matches && self.bonding_curve_matches && self.event_authority_matches
    }
    
    pub fn print_report(&self) {
        println!("🔍 PDA Verification Report:");
        println!("   Global: {}", if self.global_matches { "✅" } else { "❌ MISMATCH" });
        println!("   Bonding Curve: {}", if self.bonding_curve_matches { "✅" } else { "❌ MISMATCH" });
        println!("   Event Authority: {}", if self.event_authority_matches { "✅" } else { "❌ MISMATCH" });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_derive_global_pda() {
        let (global, bump) = derive_global_pda();
        
        // Verify it's a valid PDA (not default)
        assert_ne!(global, Pubkey::default());
        assert!(bump <= 255);
        
        // Verify it matches expected hardcoded address
        let expected = get_global_account_address();
        assert_eq!(global, expected, "Global PDA should match hardcoded address");
    }

    #[test]
    fn test_derive_bonding_curve_pda() {
        let mint = Pubkey::new_unique();
        let (bonding_curve, bump) = derive_bonding_curve_pda(&mint);
        
        // Verify it's a valid PDA
        assert_ne!(bonding_curve, Pubkey::default());
        assert_ne!(bonding_curve, mint);
        assert!(bump <= 255);
        
        // Same mint should produce same bonding curve
        let (bonding_curve2, bump2) = derive_bonding_curve_pda(&mint);
        assert_eq!(bonding_curve, bonding_curve2);
        assert_eq!(bump, bump2);
        
        // Different mint should produce different bonding curve
        let mint2 = Pubkey::new_unique();
        let (bonding_curve3, _) = derive_bonding_curve_pda(&mint2);
        assert_ne!(bonding_curve, bonding_curve3);
    }

    #[test]
    fn test_derive_event_authority_pda() {
        let (event_authority, bump) = derive_event_authority_pda();
        
        // Verify it's a valid PDA
        assert_ne!(event_authority, Pubkey::default());
        assert!(bump <= 255);
        
        // Should always produce same result
        let (event_authority2, bump2) = derive_event_authority_pda();
        assert_eq!(event_authority, event_authority2);
        assert_eq!(bump, bump2);
        
        // Verify it matches expected hardcoded address
        let expected = get_event_authority_address();
        assert_eq!(event_authority, expected, "Event Authority PDA should match hardcoded address");
    }

    #[test]
    fn test_derive_user_volume_pda() {
        let user_wallet = Pubkey::new_unique();
        let (user_volume, bump) = derive_user_volume_pda(&user_wallet);
        
        // Verify it's a valid PDA
        assert_ne!(user_volume, Pubkey::default());
        assert_ne!(user_volume, user_wallet);
        assert!(bump <= 255);
        
        // Same wallet should produce same user volume
        let (user_volume2, bump2) = derive_user_volume_pda(&user_wallet);
        assert_eq!(user_volume, user_volume2);
        assert_eq!(bump, bump2);
        
        // Different wallet should produce different user volume
        let user_wallet2 = Pubkey::new_unique();
        let (user_volume3, _) = derive_user_volume_pda(&user_wallet2);
        assert_ne!(user_volume, user_volume3);
    }

    #[test]
    fn test_hardcoded_addresses() {
        // Test that hardcoded addresses are valid Pubkeys
        let global_volume = get_global_volume_address();
        assert_ne!(global_volume, Pubkey::default());
        
        let fee_recipient = get_fee_recipient_address();
        assert_ne!(fee_recipient, Pubkey::default());
        
        let fee_config = get_fee_config_address();
        assert_ne!(fee_config, Pubkey::default());
        
        let fee_program = get_fee_program_address();
        assert_ne!(fee_program, Pubkey::default());
    }

    #[test]
    fn test_pump_pdas_recalculate_all() {
        let mint = Pubkey::new_unique();
        let user_wallet = Pubkey::new_unique();
        
        let pdas = PumpPdas::recalculate_all(&mint, &user_wallet);
        
        // Verify all PDAs are set
        assert_ne!(pdas.global, Pubkey::default());
        assert_ne!(pdas.bonding_curve, Pubkey::default());
        assert_ne!(pdas.event_authority, Pubkey::default());
        assert_ne!(pdas.user_volume, Pubkey::default());
        assert_ne!(pdas.global_volume, Pubkey::default());
        assert_ne!(pdas.fee_recipient, Pubkey::default());
        assert_ne!(pdas.fee_config, Pubkey::default());
        assert_ne!(pdas.fee_program, Pubkey::default());
        
        // Verify bonding curve is derived from mint
        let (expected_bc, _) = derive_bonding_curve_pda(&mint);
        assert_eq!(pdas.bonding_curve, expected_bc);
        
        // Verify user volume is derived from user wallet
        let (expected_uv, _) = derive_user_volume_pda(&user_wallet);
        assert_eq!(pdas.user_volume, expected_uv);
    }

    #[test]
    fn test_pump_pdas_verification() {
        let mint = Pubkey::new_unique();
        let user_wallet = Pubkey::new_unique();
        
        let pdas = PumpPdas::recalculate_all(&mint, &user_wallet);
        
        // Verify against correct expected values
        let (expected_global, _) = derive_global_pda();
        let (expected_bc, _) = derive_bonding_curve_pda(&mint);
        let (expected_ea, _) = derive_event_authority_pda();
        
        let report = pdas.verify_against_expected(&expected_global, &expected_bc, &expected_ea);
        assert!(report.all_match());
        
        // Verify against wrong expected values
        let wrong_mint = Pubkey::new_unique();
        let (wrong_bc, _) = derive_bonding_curve_pda(&wrong_mint);
        let report2 = pdas.verify_against_expected(&expected_global, &wrong_bc, &expected_ea);
        assert!(!report2.bonding_curve_matches);
    }

    #[test]
    fn test_pda_consistency() {
        // Test that PDAs are consistent across multiple calls
        let mint = Pubkey::new_unique();
        let user_wallet = Pubkey::new_unique();
        
        let pdas1 = PumpPdas::recalculate_all(&mint, &user_wallet);
        let pdas2 = PumpPdas::recalculate_all(&mint, &user_wallet);
        
        assert_eq!(pdas1.global, pdas2.global);
        assert_eq!(pdas1.bonding_curve, pdas2.bonding_curve);
        assert_eq!(pdas1.event_authority, pdas2.event_authority);
        assert_eq!(pdas1.user_volume, pdas2.user_volume);
        assert_eq!(pdas1.global_volume, pdas2.global_volume);
    }
}

