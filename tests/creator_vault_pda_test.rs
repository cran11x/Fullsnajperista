// creator_vault_pda_test.rs - Test Creator Vault PDA derivation with all possible seeds combinations
// Run with: cargo test --test creator_vault_pda_test -- --nocapture

use anyhow::Result;
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::{pubkey::Pubkey, signature::Signature, commitment_config::CommitmentConfig};
use solana_transaction_status::UiTransactionEncoding;
use std::str::FromStr;

const PUMP_PROGRAM_ID: &str = "6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P";
const BUY_DISCRIMINATOR: &[u8; 8] = &[0x33, 0xE6, 0x85, 0x5A, 0x5B, 0x6B, 0xBD, 0x5B];

#[tokio::test]
async fn test_creator_vault_pda_combinations() -> Result<()> {
    // Replace with actual Pump.fun token initialize signature
    // Get this from your bot logs when it detects a new token
    let tx_signature = std::env::var("TEST_TX_SIGNATURE")
        .unwrap_or_else(|_| "".to_string());
    
    if tx_signature.is_empty() {
        println!("⚠️  Set TEST_TX_SIGNATURE environment variable with a Pump.fun token initialize signature");
        println!("   Example: $env:TEST_TX_SIGNATURE=\"your_signature_here\"");
        return Ok(());
    }
    
    let rpc = RpcClient::new("https://api.mainnet-beta.solana.com".to_string());
    let sig = Signature::from_str(&tx_signature)?;
    
    println!("🔍 Fetching transaction: {}", tx_signature);
    
    let tx = rpc.get_transaction_with_config(
        &sig,
        solana_client::rpc_config::RpcTransactionConfig {
            encoding: Some(UiTransactionEncoding::Base64),
            max_supported_transaction_version: Some(0),
            commitment: Some(CommitmentConfig::confirmed()),
        }
    ).await?;
    
    // Parse transaction
    use base64::{engine::general_purpose, Engine as _};
    use solana_sdk::message::VersionedMessage;
    use solana_sdk::transaction::VersionedTransaction;
    
    if let solana_transaction_status::EncodedTransaction::Binary(encoded, _) = &tx.transaction.transaction {
        let tx_bytes = general_purpose::STANDARD.decode(encoded)?;
        let versioned_tx: VersionedTransaction = bincode::deserialize(&tx_bytes)?;
        
        let account_keys = match &versioned_tx.message {
            solana_sdk::message::VersionedMessage::Legacy(msg) => &msg.account_keys,
            solana_sdk::message::VersionedMessage::V0(msg) => &msg.account_keys,
        };
        
        let instructions = match &versioned_tx.message {
            solana_sdk::message::VersionedMessage::Legacy(msg) => &msg.instructions,
            solana_sdk::message::VersionedMessage::V0(msg) => &msg.instructions,
        };
        
        let pump_program = Pubkey::from_str(PUMP_PROGRAM_ID)?;
        let creator = account_keys.get(0).ok_or_else(|| anyhow::anyhow!("No signer found"))?;
        
        println!("\n📋 Transaction Info:");
        println!("   Creator: {}", creator);
        println!("   Accounts: {}", account_keys.len());
        println!("   Instructions: {}", instructions.len());
        
        // Extract actual Creator Vault from BUY instruction (index 9)
        let mut actual_creator_vault: Option<Pubkey> = None;
        
        for ix in instructions.iter() {
            let program_id_idx = ix.program_id_index as usize;
            if let Some(&program_id) = account_keys.get(program_id_idx) {
                if program_id == pump_program && ix.data.len() >= 8 {
                    let discriminator = &ix.data[0..8];
                    
                    if discriminator == BUY_DISCRIMINATOR {
                        let ix_accounts: Vec<Pubkey> = ix.accounts
                            .iter()
                            .filter_map(|&idx| account_keys.get(idx as usize).copied())
                            .collect();
                        
                        if ix_accounts.len() >= 10 {
                            actual_creator_vault = Some(ix_accounts[9]);
                            println!("\n✅ Found BUY instruction with {} accounts", ix_accounts.len());
                            println!("   Actual Creator Vault (index 9): {}", actual_creator_vault.unwrap());
                            break;
                        }
                    }
                }
            }
        }
        
        if actual_creator_vault.is_none() {
            println!("\n⚠️  No BUY instruction found in transaction");
            println!("   Cannot determine actual Creator Vault");
            return Ok(());
        }
        
        let actual = actual_creator_vault.unwrap();
        
        // Test all possible seeds combinations
        println!("\n🧪 Testing all possible seeds combinations:");
        println!("   Target: {}\n", actual);
        
        let pump_program_id = Pubkey::from_str(PUMP_PROGRAM_ID)?;
        
        let combinations = vec![
            ("creator_vault + as_ref", &[b"creator_vault", creator.as_ref()]),
            ("creator-vault + as_ref", &[b"creator-vault", creator.as_ref()]),
            ("creator_vault + to_bytes", &[b"creator_vault", &creator.to_bytes()]),
            ("creator-vault + to_bytes", &[b"creator-vault", &creator.to_bytes()]),
            ("vault + as_ref", &[b"vault", creator.as_ref()]),
            ("vault + to_bytes", &[b"vault", &creator.to_bytes()]),
            ("creator_vault reversed", &[creator.as_ref(), b"creator_vault"]),
            ("creator-vault reversed", &[creator.as_ref(), b"creator-vault"]),
        ];
        
        let mut found_match = false;
        
        for (name, seeds) in combinations {
            let (pda, bump) = Pubkey::find_program_address(seeds, &pump_program_id);
            let matches = pda == actual;
            
            if matches {
                println!("   ✅ MATCH: {} -> {}", name, pda);
                println!("      Seeds: {:?}", seeds.iter().map(|s| {
                    if s.len() == 13 && s.starts_with(b"creator") {
                        String::from_utf8_lossy(s).to_string()
                    } else if s.len() == 12 && s.starts_with(b"creator-vault") {
                        String::from_utf8_lossy(s).to_string()
                    } else if s.len() == 5 && s.starts_with(b"vault") {
                        String::from_utf8_lossy(s).to_string()
                    } else if s.len() == 32 {
                        format!("Pubkey({}...)", bs58::encode(&s[..4]).into_string())
                    } else {
                        format!("{:?}", s)
                    }
                }).collect::<Vec<_>>());
                println!("      Bump: {}", bump);
                found_match = true;
            } else {
                println!("   ❌ {} -> {} (no match)", name, pda);
            }
        }
        
        if !found_match {
            println!("\n⚠️  No matching seeds combination found!");
            println!("   Actual Creator Vault might not be a PDA");
            println!("   Or seeds combination is different");
        } else {
            println!("\n✅ Found matching seeds combination!");
        }
    }
    
    Ok(())
}

#[test]
fn test_creator_vault_seeds_patterns() {
    let pump_program_id = Pubkey::from_str(PUMP_PROGRAM_ID).unwrap();
    let creator = Pubkey::from_str("11111111111111111111111111111111").unwrap();
    
    println!("\n🧪 Testing seeds patterns with test creator:");
    println!("   Creator: {}\n", creator);
    
    let patterns = vec![
        ("creator_vault + as_ref", &[b"creator_vault", creator.as_ref()]),
        ("creator-vault + as_ref", &[b"creator-vault", creator.as_ref()]),
        ("creator_vault + to_bytes", &[b"creator_vault", &creator.to_bytes()]),
        ("creator-vault + to_bytes", &[b"creator-vault", &creator.to_bytes()]),
    ];
    
    for (name, seeds) in patterns {
        let (pda, bump) = Pubkey::find_program_address(seeds, &pump_program_id);
        println!("   {}: {} (bump: {})", name, pda, bump);
    }
}

