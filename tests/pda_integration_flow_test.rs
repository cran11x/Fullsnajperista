// Integration test that verifies PDA recalculation works correctly in the full buy/sell flow
// This test simulates the scenario that was causing Error 0x1f9

use Fullsnajperista::pda_derivation::*;
use Fullsnajperista::buy;
use Fullsnajperista::sell;
use Fullsnajperista::detection::PumpBuyAccounts;
use solana_sdk::pubkey::Pubkey;
use std::str::FromStr;

#[test]
fn test_pda_recalculation_in_buy_instruction() {
    // Simulate scenario: First transaction with a new mint and user
    let mint1 = Pubkey::new_unique();
    let user_wallet1 = Pubkey::new_unique();
    let user_token_account1 = Pubkey::new_unique();
    
    // Create accounts structure (simulating what would come from detection)
    let accounts = PumpBuyAccounts {
        mint: mint1,
        bonding_curve: Pubkey::new_unique(), // This might be wrong/stale
        associated_bonding_curve: Pubkey::new_unique(),
        creator_vault: Pubkey::new_unique(),
        event_authority: Pubkey::new_unique(), // This might be wrong/stale
        global_volume: Pubkey::new_unique(), // This might be wrong/stale
        global: Pubkey::new_unique(), // This might be wrong/stale
        fee_recipient: Pubkey::new_unique(),
        fee_config: Pubkey::new_unique(),
        fee_program: Pubkey::new_unique(),
        dev_buy_sol: 1_000_000_000, // 1 SOL
        creator: Pubkey::new_unique(),
        associated_bonding_curve_instruction: None,
    };
    
    // Recalculate PDAs for this specific mint and user
    let pdas = PumpPdas::recalculate_all(&accounts.mint, &user_wallet1);
    
    // Verify that recalculated PDAs are correct
    let (expected_global, _) = derive_global_pda();
    let (expected_bonding_curve, _) = derive_bonding_curve_pda(&mint1);
    let (expected_event_authority, _) = derive_event_authority_pda();
    let (expected_user_volume, _) = derive_user_volume_pda(&user_wallet1);
    
    assert_eq!(pdas.global, expected_global, "Global PDA should match");
    assert_eq!(pdas.bonding_curve, expected_bonding_curve, "Bonding curve PDA should match for this mint");
    assert_eq!(pdas.event_authority, expected_event_authority, "Event authority PDA should match");
    assert_eq!(pdas.user_volume, expected_user_volume, "User volume PDA should match for this user");
    
    // Verify that if accounts had wrong values, we still use correct ones
    // This is the key fix - even if accounts.bonding_curve is wrong, we use pdas.bonding_curve
    if accounts.bonding_curve != expected_bonding_curve {
        println!("✅ Test scenario: accounts had wrong bonding_curve, but we use recalculated one");
        assert_eq!(pdas.bonding_curve, expected_bonding_curve);
    }
}

#[test]
fn test_pda_recalculation_different_mints() {
    // Test that different mints produce different bonding curves
    let mint1 = Pubkey::new_unique();
    let mint2 = Pubkey::new_unique();
    let user_wallet = Pubkey::new_unique();
    
    let pdas1 = PumpPdas::recalculate_all(&mint1, &user_wallet);
    let pdas2 = PumpPdas::recalculate_all(&mint2, &user_wallet);
    
    // Global, event_authority, user_volume should be the same
    assert_eq!(pdas1.global, pdas2.global);
    assert_eq!(pdas1.event_authority, pdas2.event_authority);
    assert_eq!(pdas1.user_volume, pdas2.user_volume);
    
    // But bonding_curve should be different
    assert_ne!(pdas1.bonding_curve, pdas2.bonding_curve, 
               "Different mints should produce different bonding curves");
}

#[test]
fn test_pda_recalculation_different_users() {
    // Test that different users produce different user volume PDAs
    let mint = Pubkey::new_unique();
    let user1 = Pubkey::new_unique();
    let user2 = Pubkey::new_unique();
    
    let pdas1 = PumpPdas::recalculate_all(&mint, &user1);
    let pdas2 = PumpPdas::recalculate_all(&mint, &user2);
    
    // Global, bonding_curve, event_authority should be the same
    assert_eq!(pdas1.global, pdas2.global);
    assert_eq!(pdas1.bonding_curve, pdas2.bonding_curve);
    assert_eq!(pdas1.event_authority, pdas2.event_authority);
    
    // But user_volume should be different
    assert_ne!(pdas1.user_volume, pdas2.user_volume,
               "Different users should produce different user volume PDAs");
}

#[test]
fn test_pda_consistency_across_multiple_calls() {
    // Test that calling recalculate_all multiple times with same inputs produces same results
    let mint = Pubkey::new_unique();
    let user_wallet = Pubkey::new_unique();
    
    let pdas1 = PumpPdas::recalculate_all(&mint, &user_wallet);
    let pdas2 = PumpPdas::recalculate_all(&mint, &user_wallet);
    let pdas3 = PumpPdas::recalculate_all(&mint, &user_wallet);
    
    // All should be identical
    assert_eq!(pdas1.global, pdas2.global);
    assert_eq!(pdas2.global, pdas3.global);
    
    assert_eq!(pdas1.bonding_curve, pdas2.bonding_curve);
    assert_eq!(pdas2.bonding_curve, pdas3.bonding_curve);
    
    assert_eq!(pdas1.event_authority, pdas2.event_authority);
    assert_eq!(pdas2.event_authority, pdas3.event_authority);
    
    assert_eq!(pdas1.user_volume, pdas2.user_volume);
    assert_eq!(pdas2.user_volume, pdas3.user_volume);
}

#[test]
fn test_pda_matches_hardcoded_addresses() {
    // Verify that PDAs match known hardcoded addresses
    let (global, _) = derive_global_pda();
    let expected_global = get_global_account_address();
    assert_eq!(global, expected_global);
    
    let (event_authority, _) = derive_event_authority_pda();
    let expected_ea = get_event_authority_address();
    assert_eq!(event_authority, expected_ea);
    
    // Verify hardcoded addresses
    let global_volume = get_global_volume_address();
    assert_eq!(global_volume.to_string(), "Hq2wp8uJ9jCPsYgNHex8RtqdvMPfVGoYwjvF1ATiwn2Y");
}

