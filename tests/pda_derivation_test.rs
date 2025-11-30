// Integration tests for PDA derivation
// Run with: cargo test --test pda_derivation_test

use Fullsnajperista::pda_derivation::*;
use solana_sdk::pubkey::Pubkey;
use std::str::FromStr;

#[test]
fn test_real_world_pda_derivation() {
    // Test with a real mint address from Pump.fun
    let real_mint = Pubkey::from_str("EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v").unwrap(); // USDC for testing
    let user_wallet = Pubkey::new_unique();
    
    let pdas = PumpPdas::recalculate_all(&real_mint, &user_wallet);
    
    println!("\n🔍 Real-world PDA Derivation Test:");
    println!("   Mint: {}", real_mint);
    println!("   User Wallet: {}", user_wallet);
    println!("   Global: {}", pdas.global);
    println!("   Bonding Curve: {}", pdas.bonding_curve);
    println!("   Event Authority: {}", pdas.event_authority);
    println!("   User Volume: {}", pdas.user_volume);
    println!("   Global Volume: {}", pdas.global_volume);
    
    // Verify all PDAs are different from each other
    assert_ne!(pdas.global, pdas.bonding_curve);
    assert_ne!(pdas.global, pdas.event_authority);
    assert_ne!(pdas.bonding_curve, pdas.user_volume);
}

#[test]
fn test_pda_against_known_values() {
    // Test that PDAs match known hardcoded values
    let (global, _) = derive_global_pda();
    let expected_global = get_global_account_address();
    assert_eq!(global, expected_global, "Global PDA should match known address");
    
    let (event_authority, _) = derive_event_authority_pda();
    let expected_ea = get_event_authority_address();
    assert_eq!(event_authority, expected_ea, "Event Authority PDA should match known address");
}

#[test]
fn test_multiple_mints_different_pdas() {
    // Test that different mints produce different bonding curves
    let mint1 = Pubkey::new_unique();
    let mint2 = Pubkey::new_unique();
    
    let (bc1, _) = derive_bonding_curve_pda(&mint1);
    let (bc2, _) = derive_bonding_curve_pda(&mint2);
    
    assert_ne!(bc1, bc2, "Different mints should produce different bonding curves");
}

#[test]
fn test_multiple_users_different_pdas() {
    // Test that different users produce different user volume PDAs
    let user1 = Pubkey::new_unique();
    let user2 = Pubkey::new_unique();
    
    let (uv1, _) = derive_user_volume_pda(&user1);
    let (uv2, _) = derive_user_volume_pda(&user2);
    
    assert_ne!(uv1, uv2, "Different users should produce different user volume PDAs");
}

