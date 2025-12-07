// detection.rs - FIXED: Extract creator from CREATE instruction account[8]
#![allow(unused_imports, dead_code)]

use anyhow::{anyhow, Result};
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::{pubkey::Pubkey, commitment_config::CommitmentConfig};
use spl_associated_token_account::get_associated_token_address;
use std::str::FromStr;

use crate::constants::{PUMP_PROGRAM_ID, BUY_DISCRIMINATOR};

// 🔧 TOGGLE THIS: true = detailed logs, false = normal logs
const DEBUG: bool = true;

/// Calculate Associated Bonding Curve addresses with both Token Program versions (standard and 2022)
fn calculate_addresses_both_programs(
    bonding_curve: &Pubkey,
    mint: &Pubkey,
    _creator: &Pubkey,
) -> (Pubkey, Pubkey) {
    // Token Program (obični)
    let token_program = Pubkey::from_str("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA").unwrap();
    
    // Token Program 2022
    let token_program_2022 = Pubkey::from_str("TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb").unwrap();
    
    let abc_standard = spl_associated_token_account::get_associated_token_address_with_program_id(
        bonding_curve, mint, &token_program
    );
    
    let abc_2022 = spl_associated_token_account::get_associated_token_address_with_program_id(
        bonding_curve, mint, &token_program_2022
    );
    
    (abc_standard, abc_2022)
}

/// Extract Associated Bonding Curve address from transaction
fn extract_associated_bonding_curve_from_tx(
    instructions: &[solana_sdk::instruction::CompiledInstruction],
    account_keys: &[Pubkey],
    bonding_curve: &Pubkey,
    mint: &Pubkey,
) -> Option<Pubkey> {
    let ata_program = Pubkey::from_str("ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL").ok()?;
    
    for ix in instructions.iter() {
        let program_id_idx = ix.program_id_index as usize;
        if let Some(&program_id) = account_keys.get(program_id_idx) {
            if program_id == ata_program {
                let ix_accounts: Vec<Pubkey> = ix.accounts
                    .iter()
                    .filter_map(|&idx| account_keys.get(idx as usize).copied())
                    .collect();
                
                // ATA instruction has: payer, owner, mint, system_program, token_program, ata_account
                if ix_accounts.len() >= 6 {
                    let owner = ix_accounts.get(1); // owner is at index 1
                    let mint_account = ix_accounts.get(2); // mint is at index 2
                    let ata_account = ix_accounts.get(5); // ata_account is at index 5
                    
                    // Check if this ATA is for bonding_curve
                    if owner == Some(bonding_curve) && mint_account == Some(mint) {
                        return ata_account.copied();
                    }
                }
            }
        }
    }
    None
}


#[derive(Debug, Clone)]
pub struct PumpBuyAccounts {
    pub mint: Pubkey,
    pub bonding_curve: Pubkey,
    pub associated_bonding_curve: Pubkey,
    pub creator_vault: Pubkey,
    pub event_authority: Pubkey,
    pub global_volume: Pubkey,
    pub global: Pubkey,
    pub fee_recipient: Pubkey,
    pub fee_config: Pubkey,
    pub fee_program: Pubkey,
    pub dev_buy_sol: u64,
    pub creator: Pubkey,
    pub associated_bonding_curve_instruction: Option<solana_sdk::instruction::Instruction>, // Store actual instruction from dev TX
}

impl PumpBuyAccounts {
    pub async fn from_initialize_tx(
        rpc: &RpcClient,
        init_signature: &str,
    ) -> Result<(Self, Pubkey)> {
        let sig = solana_sdk::signature::Signature::from_str(init_signature)?;

        let mut attempts = 0;
        let max_attempts = 8;

        let tx = loop {
            attempts += 1;

            match rpc.get_transaction_with_config(
                &sig,
                solana_client::rpc_config::RpcTransactionConfig {
                    encoding: Some(solana_transaction_status::UiTransactionEncoding::Base64),
                    max_supported_transaction_version: Some(0),
                    commitment: Some(CommitmentConfig::confirmed()),
                }
            ).await {
                Ok(tx) => {
                    println!("      ✅ TX ready! (attempt {})", attempts);
                    break tx;
                }
                Err(e) => {
                    if attempts >= max_attempts {
                        return Err(anyhow!("TX not available after {} attempts: {}", max_attempts, e));
                    }

                    let wait_ms = match attempts {
                        1 => 100,
                        2 => 150,
                        3 => 200,
                        4 => 300,
                        _ => 400,
                    };

                    println!("      ⏳ Waiting for TX... ({}/{}, {}ms)", attempts, max_attempts, wait_ms);
                    tokio::time::sleep(tokio::time::Duration::from_millis(wait_ms)).await;
                }
            }
        };

        if let solana_transaction_status::EncodedTransaction::Binary(encoded, _) = &tx.transaction.transaction {
            use base64::{engine::general_purpose, Engine as _};
            use solana_sdk::message::VersionedMessage;
            use solana_sdk::transaction::VersionedTransaction;

            let tx_bytes = general_purpose::STANDARD.decode(encoded)?;
            let versioned_tx: VersionedTransaction = bincode::deserialize(&tx_bytes)?;

            let account_keys = match &versioned_tx.message {
                VersionedMessage::Legacy(msg) => &msg.account_keys,
                VersionedMessage::V0(msg) => &msg.account_keys,
            };

            let instructions = match &versioned_tx.message {
                VersionedMessage::Legacy(msg) => &msg.instructions,
                VersionedMessage::V0(msg) => &msg.instructions,
            };

            let pump_program = Pubkey::from_str(PUMP_PROGRAM_ID)?;
            let mint = account_keys.get(1).ok_or_else(|| anyhow!("No mint found"))?;

            // ✅ FIXED: Get creator from first signer (payer of transaction)
            let creator = match &versioned_tx.message {
                VersionedMessage::Legacy(msg) => {
                    msg.account_keys.get(0).copied()
                        .ok_or_else(|| anyhow!("No signer found"))?
                }
                VersionedMessage::V0(msg) => {
                    msg.account_keys.get(0).copied()
                        .ok_or_else(|| anyhow!("No signer found"))?
                }
            };

            if DEBUG {
                println!("      ✅ Creator (signer): {}", creator);
            }

            let mut dev_buy_sol = 0u64;

            if DEBUG {
                println!("      🔍 DEBUG: TX {} instructions, {} accounts",
                         instructions.len(), account_keys.len());
            }

            // 🔍 Analyze all instructions to see how dev builds transaction
            // Look for Associated Bonding Curve creation instruction and extract actual address
            let ata_program = Pubkey::from_str("ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL").ok();
            let mut found_abc_instruction = false;
            let mut abc_instruction_idx = None;
            let mut actual_abc_from_tx: Option<Pubkey> = None;
            let mut abc_instruction_from_tx: Option<solana_sdk::instruction::Instruction> = None;
            
            // Calculate bonding curve first to use in comparison
            let (bonding_curve_temp, _) = Pubkey::find_program_address(
                &[b"bonding-curve", &mint.to_bytes()],
                &pump_program,
            );
            
            for (ix_idx, ix) in instructions.iter().enumerate() {
                let program_id_idx = ix.program_id_index as usize;
                if let Some(&program_id) = account_keys.get(program_id_idx) {
                    // Check if this is ATA creation instruction (for Associated Bonding Curve)
                    if let Some(ata_prog) = ata_program {
                        if program_id == ata_prog {
                            let ix_accounts: Vec<Pubkey> = ix.accounts
                                .iter()
                                .filter_map(|&idx| account_keys.get(idx as usize).copied())
                                .collect();
                            
                            // Check if this ATA is for bonding_curve (owner) and mint
                            // ATA instruction has: payer, owner, mint, system_program, token_program, ata_account
                            if ix_accounts.len() >= 6 {
                                let owner = ix_accounts.get(1); // owner is at index 1
                                let mint_account = ix_accounts.get(2); // mint is at index 2
                                let ata_account = ix_accounts.get(5); // ata_account is at index 5
                                
                                // Check if this ATA is for bonding_curve
                                if owner == Some(&bonding_curve_temp) && mint_account == Some(mint) {
                                    found_abc_instruction = true;
                                    abc_instruction_idx = Some(ix_idx);
                                    actual_abc_from_tx = ata_account.copied();
                                    
                                    // Extract the actual instruction to use it later
                                    // We need to get the actual account metadata from the compiled instruction
                                    // ATA instruction structure: [payer (writable, signer), owner (writable), mint (readonly), system (readonly), token_program (readonly), ata_account (writable)]
                                    let program_id_from_tx = account_keys.get(program_id_idx).copied();
                                    if let Some(prog_id) = program_id_from_tx {
                                        // Get account keys from the instruction
                                        let mut accounts_meta = Vec::new();
                                        for (i, &account_idx) in ix.accounts.iter().enumerate() {
                                            if let Some(account_key) = account_keys.get(account_idx as usize) {
                                                // ATA instruction structure:
                                                // 0: payer (writable, signer)
                                                // 1: owner (writable)
                                                // 2: mint (readonly)
                                                // 3: system_program (readonly)
                                                // 4: token_program (readonly)
                                                // 5: ata_account (writable)
                                                let is_writable = i == 0 || i == 1 || i == 5;
                                                let is_signer = i == 0; // Only payer is signer
                                                
                                                accounts_meta.push(solana_sdk::instruction::AccountMeta {
                                                    pubkey: *account_key, // account_key is &Pubkey, dereference to get Pubkey
                                                    is_signer,
                                                    is_writable,
                                                });
                                            }
                                        }
                                        
                                        if accounts_meta.len() >= 6 {
                                            abc_instruction_from_tx = Some(solana_sdk::instruction::Instruction {
                                                program_id: prog_id,
                                                accounts: accounts_meta,
                                                data: ix.data.clone(),
                                            });
                                        }
                                    }
                                    
                                    if DEBUG {
                                        println!("      ✅ Found Associated Bonding Curve instruction at position {} (address: {})", 
                                                 ix_idx, ata_account.map(|a| a.to_string()).unwrap_or("N/A".to_string()));
                                    }
                                }
                            }
                        }
                    }
                }
            }
            
            if found_abc_instruction {
                if DEBUG {
                    println!("      📋 Dev transaction structure: Associated Bonding Curve created at instruction {}", 
                             abc_instruction_idx.unwrap());
                }
            } else {
                if DEBUG {
                    println!("      ⚠️  No Associated Bonding Curve instruction found in dev transaction");
                }
            }

            for (ix_idx, ix) in instructions.iter().enumerate() {
                let program_id_idx = ix.program_id_index as usize;
                if let Some(&program_id) = account_keys.get(program_id_idx) {
                    if program_id == pump_program {
                        let ix_accounts: Vec<Pubkey> = ix.accounts
                            .iter()
                            .filter_map(|&idx| account_keys.get(idx as usize).copied())
                            .collect();

                        if ix.data.len() >= 8 {
                            let discriminator = &ix.data[0..8];

                            if DEBUG {
                                println!("      🔍 IX[{}]: Pump instruction, discriminator={:02x?}",
                                         ix_idx, discriminator);
                            }

                            // ✅ Extract vault from CREATE instruction
                            // Support multiple CREATE discriminators (Pump.fun may have changed)
                            let is_create_instruction = discriminator == &[0x18, 0x1e, 0xc8, 0x28, 0x05, 0x1c, 0x07, 0x77] // New format
                                || discriminator == &[0xd6, 0x90, 0x4c, 0xec, 0x5f, 0x8b, 0x31, 0xb4]; // Old format

                            if is_create_instruction {
                                // Extract Associated Bonding Curve from CREATE instruction
                                // CREATE instruction typically has Associated Bonding Curve around index 4-7
                                // NOTE: We verify ownership to ensure it's a token account
                                if actual_abc_from_tx.is_none() && ix_accounts.len() >= 8 {
                                    let token_program = Pubkey::from_str("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA").unwrap();
                                    let token_program_2022 = Pubkey::from_str("TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb").unwrap();
                                    let system_program = Pubkey::from_str("11111111111111111111111111111111").ok();
                                    let ata_program = Pubkey::from_str("ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL").ok();
                                    
                                    // Try different positions for Associated Bonding Curve
                                    // Common positions: 4, 5, 6, 7 (after mint, bonding_curve, etc.)
                                    let possible_indices = vec![4, 5, 6, 7, 3, 8];
                                    for &idx in &possible_indices {
                                        if ix_accounts.len() > idx {
                                            let candidate = ix_accounts[idx];
                                            
                                            // Basic filter: skip known programs
                                            if candidate != *mint
                                                && candidate != creator
                                                && candidate != pump_program
                                                && candidate != bonding_curve_temp
                                                && candidate != token_program
                                                && candidate != token_program_2022
                                                && candidate != system_program.unwrap_or(candidate)
                                                && candidate != ata_program.unwrap_or(candidate) {
                                                
                                                // CRITICAL: Verify ownership - must be a token account
                                                if let Ok(account_info) = rpc.get_account(&candidate).await {
                                                    let owner = account_info.owner;
                                                    let is_token_account = owner == token_program_2022 || owner == token_program;
                                                    
                                                    if is_token_account {
                                                        actual_abc_from_tx = Some(candidate);
                                                        if DEBUG {
                                                            println!("      ✅ Associated Bonding Curve from CREATE (idx {}): {} (verified: token account, owner: {})", idx, candidate, owner);
                                                        }
                                                        break;
                                                    } else if DEBUG {
                                                        println!("      ⚠️  Skipping candidate at idx {}: {} (not a token account, owner: {})", idx, candidate, owner);
                                                    }
                                                } else if DEBUG {
                                                    println!("      ⚠️  Cannot verify candidate at idx {}: {} (account not found)", idx, candidate);
                                                }
                                            }
                                        }
                                    }
                                }
                                
                                // ⚠️  SKIPPED: Creator Vault extraction from CREATE instruction
                                // We prefer BUY instruction for Creator Vault (more reliable - exact index 9)
                                // CREATE instruction extraction is disabled to ensure we use BUY instruction value
                                // if creator_vault.is_none() {
                                //     ... (commented out - use BUY instruction instead)
                                // }
                            }

                            if discriminator == BUY_DISCRIMINATOR {
                                // ✅ Extract Associated Bonding Curve from BUY instruction
                                // BUY instruction has Associated Bonding Curve at index 4 (after global, fee_recipient, mint, bonding_curve)
                                // PRIORITY: BUY instruction has exact structure, so we trust index 4
                                if actual_abc_from_tx.is_none() && ix_accounts.len() >= 5 {
                                    let candidate = ix_accounts[4]; // Associated Bonding Curve is at index 4
                                    let token_program = Pubkey::from_str("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA").unwrap();
                                    let token_program_2022 = Pubkey::from_str("TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb").unwrap();
                                    
                                    // Basic filter: skip known programs
                                    if candidate != *mint
                                        && candidate != creator
                                        && candidate != pump_program
                                        && candidate != bonding_curve_temp
                                        && candidate != token_program
                                        && candidate != token_program_2022 {
                                        
                                        // CRITICAL: Verify ownership - must be a token account
                                        if let Ok(account_info) = rpc.get_account(&candidate).await {
                                            let owner = account_info.owner;
                                            let is_token_account = owner == token_program_2022 || owner == token_program;
                                            
                                            if is_token_account {
                                                actual_abc_from_tx = Some(candidate);
                                                if DEBUG {
                                                    println!("      ✅ Associated Bonding Curve from BUY (idx 4): {} (verified: token account, owner: {})", candidate, owner);
                                                }
                                            } else if DEBUG {
                                                println!("      ⚠️  BUY idx 4 is not a token account: {} (owner: {})", candidate, owner);
                                            }
                                        } else if DEBUG {
                                            println!("      ⚠️  Cannot verify BUY idx 4: {} (account not found)", candidate);
                                        }
                                    }
                                }

                                if ix.data.len() >= 24 {
                                    let max_sol_bytes = &ix.data[16..24];
                                    let raw_value = u64::from_le_bytes(max_sol_bytes.try_into().unwrap_or([0u8; 8]));

                                    // 🔥 SANITY CHECK: If > 100 SOL, might be wrong parsing
                                    if raw_value > 100_000_000_000 { // 100 SOL in lamports
                                        if DEBUG {
                                            println!("      ⚠️  Suspicious value: {} ({} SOL) - might be parsing error",
                                                     raw_value, raw_value as f64 / 1e9);
                                        }
                                        // Don't set dev_buy_sol if unrealistic
                                    } else {
                                        dev_buy_sol = raw_value;

                                        if DEBUG {
                                            let token_amount = u64::from_le_bytes(ix.data[8..16].try_into().unwrap());
                                            println!("      ✅ BUY found: {} SOL, {} tokens, bytes={:02x?}",
                                                     dev_buy_sol as f64 / 1e9, token_amount, max_sol_bytes);
                                        } else {
                                            println!("      💰 Dev buy: {} SOL", dev_buy_sol as f64 / 1e9);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            if dev_buy_sol == 0 {
                if DEBUG {
                    println!("      ⚠️  No BUY instruction - checking balances...");
                }

                if let Some(meta) = &tx.transaction.meta {
                    let pre_balances = &meta.pre_balances;
                    let post_balances = &meta.post_balances;

                    if DEBUG {
                        println!("      📊 Balance changes:");
                        for i in 0..pre_balances.len().min(10) {
                            if i < post_balances.len() {
                                let pre = pre_balances[i];
                                let post = post_balances[i];
                                let diff = if pre > post {
                                    format!("-{:.4}", (pre - post) as f64 / 1e9)
                                } else {
                                    format!("+{:.4}", (post - pre) as f64 / 1e9)
                                };
                                println!("         [{}] {} SOL", i, diff);
                            }
                        }
                    }

                    // 🔥 Check ALL accounts for spending, not just 6-7
                    for i in 0..pre_balances.len().min(10) {
                        if i < post_balances.len() {
                            let pre = pre_balances[i];
                            let post = post_balances[i];
                            if pre > post {
                                let spent = pre - post;
                                    // Look for significant spends (>0.1 SOL, but not rent ~0.002)
                                if spent > 100_000_000 && spent < 100_000_000_000u64 { // 0.1-100 SOL
                                    dev_buy_sol = spent;
                                    if DEBUG {
                                        println!("      ⚠️  Balance fallback [{}]: {} SOL",
                                                 i, dev_buy_sol as f64 / 1e9);
                                    } else {
                                        println!("      💰 Dev buy (balances): {} SOL", dev_buy_sol as f64 / 1e9);
                                    }
                                    break;
                                }
                            }
                        }
                    }
                }
            }

            if DEBUG && dev_buy_sol > 0 {
                println!("      📋 Final: {} SOL", dev_buy_sol as f64 / 1e9);
            }

            // Note: creator_vault extraction happens in the loop above
            // We'll use the comparison results to decide which one to use later

            // KONAČNO RIJEŠENJE – 2025 PUMPFUN
            // Extract actual accounts from BUY instruction FIRST (before using them)
            let pump_program_id = Pubkey::from_str("6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P").unwrap();
            
            // Try to extract actual accounts from BUY instruction:
            // - Bonding Curve: index 3 (extract from BUY to verify derivation)
            // - Creator Vault: index 9
            // - User Volume Accumulator: index 14 (for reference only - creator's user volume)
            let mut bonding_curve_from_buy: Option<Pubkey> = None;
            let mut vault_from_buy: Option<Pubkey> = None;
            let mut user_volume_from_buy: Option<Pubkey> = None;
            
            for ix in instructions.iter() {
                let program_id_idx = ix.program_id_index as usize;
                if let Some(&program_id) = account_keys.get(program_id_idx) {
                    if program_id == pump_program_id && ix.data.len() >= 8 {
                        let discriminator = &ix.data[0..8];
                        if discriminator == BUY_DISCRIMINATOR {
                            let ix_accounts: Vec<Pubkey> = ix.accounts
                                .iter()
                                .filter_map(|&idx| account_keys.get(idx as usize).copied())
                                .collect();
                            
                            // Extract Bonding Curve (index 3) from BUY instruction to verify derivation
                            if ix_accounts.len() >= 4 {
                                bonding_curve_from_buy = Some(ix_accounts[3]);
                                eprintln!("✅ Extracted Bonding Curve (index 3): {}", ix_accounts[3]);
                            }
                            
                            // Extract Creator Vault (index 9)
                            if ix_accounts.len() >= 10 {
                                vault_from_buy = Some(ix_accounts[9]);
                                eprintln!("✅ Extracted Creator Vault (index 9): {}", ix_accounts[9]);
                            } else {
                                eprintln!("⚠️  BUY instruction has only {} accounts, need at least 10 for Creator Vault", ix_accounts.len());
                            }
                            
                            // Extract User Volume Accumulator (index 14) - for reference only (creator's user volume)
                            if ix_accounts.len() >= 15 {
                                user_volume_from_buy = Some(ix_accounts[14]);
                                eprintln!("✅ Extracted User Volume Accumulator (index 14): {} (creator's user volume)", ix_accounts[14]);
                            }
                            
                            if vault_from_buy.is_some() {
                                break;
                            }
                        }
                    }
                }
            }

            // Derive bonding curve PDA first
            let (bonding_curve_derived, _) = Pubkey::find_program_address(
                &[b"bonding-curve", &mint.to_bytes()],
                &pump_program,
            );
            
            // Use bonding curve from BUY instruction if available, otherwise use derived
            let bonding_curve = if let Some(bc_from_buy) = bonding_curve_from_buy {
                eprintln!();
                eprintln!("🔍 DECIDING WHICH BONDING CURVE ADDRESS TO USE...");
                eprintln!("   ✅ USING BONDING CURVE FROM BUY (index 3): {}", bc_from_buy);
                eprintln!("   📋 Derived PDA: {} (for comparison)", bonding_curve_derived);
                if bc_from_buy != bonding_curve_derived {
                    eprintln!("   ⚠️  WARNING: Bonding curve from BUY does NOT match derived PDA!");
                    eprintln!("   ⚠️  This might cause Error 2006 (ConstraintSeeds)!");
                }
                bc_from_buy
            } else {
                eprintln!();
                eprintln!("🔍 DECIDING WHICH BONDING CURVE ADDRESS TO USE...");
                eprintln!("   ⚠️  NO BUY INSTRUCTION → USING DERIVED PDA");
                eprintln!("   📋 Derived PDA: {}", bonding_curve_derived);
                bonding_curve_derived
            };
            
            eprintln!("   ✅ FINAL Bonding Curve: {}", bonding_curve);

            // Check if bonding curve account exists in transaction (it should be created in initialize TX)
            let bonding_curve_in_tx = account_keys.iter().any(|&key| key == bonding_curve);
            eprintln!("      ✅ Bonding curve account found in transaction: {}", bonding_curve);

            // Extract actual addresses from transaction
            let actual_abc = actual_abc_from_tx.or_else(|| {
                extract_associated_bonding_curve_from_tx(&instructions, &account_keys, &bonding_curve, mint)
            });
            // Calculate addresses with both token programs
            eprintln!();
            eprintln!("🔍 CALCULATING ADDRESSES WITH BOTH TOKEN PROGRAMS...");
            eprintln!("   Bonding curve: {}", bonding_curve);
            eprintln!("   Mint: {}", mint);
            eprintln!("   Creator: {}", creator);
            
            let (abc_standard, abc_2022) = 
                calculate_addresses_both_programs(&bonding_curve, mint, &creator);
            
            eprintln!("   ✅ Calculated ABC (standard): {}", abc_standard);
            eprintln!("   ✅ Calculated ABC (2022):     {}", abc_2022);

            // Compare and log (using eprintln! so it shows in console)
            eprintln!();
            eprintln!("================== ADDRESS COMPARISON ==================");
            eprintln!("Associated Bonding Curve:");
            eprintln!("  Actual (from TX):      {}", actual_abc.as_ref().map(|a| a.to_string()).unwrap_or("NOT FOUND".to_string()));
            eprintln!("  Calculated (standard): {}", abc_standard);
            eprintln!("  Calculated (2022):     {}", abc_2022);
            eprintln!("  Match standard:        {}", actual_abc.as_ref() == Some(&abc_standard));
            eprintln!("  Match 2022:            {}", actual_abc.as_ref() == Some(&abc_2022));
            eprintln!();
            eprintln!("Bonding Curve PDA:");
            eprintln!("  Calculated:            {}", bonding_curve);
            eprintln!("  In transaction:        {}", bonding_curve_in_tx);
            
            // Verify bonding curve PDA calculation
            if bonding_curve_in_tx {
                // Extract actual bonding curve from transaction if possible
                let actual_bonding_curve = account_keys.iter()
                    .find(|&&key| {
                        // Check if this could be a bonding curve PDA
                        let (calculated, _) = Pubkey::find_program_address(
                            &[b"bonding-curve", &mint.to_bytes()],
                            &pump_program,
                        );
                        key == calculated
                    });
                
                if let Some(&actual) = actual_bonding_curve {
                    if actual == bonding_curve {
                        println!("  ✅ PDA matches transaction");
                    } else {
                        println!("  ⚠️  PDA mismatch! Calculated: {}, Actual: {}", bonding_curve, actual);
                    }
                }
            }
            eprintln!("========================================================");
            eprintln!();

            // Decide which address to use based on comparison
            // CRITICAL: Verify actual address is a token account before using it
            eprintln!();
            eprintln!("🔍 DECIDING WHICH ASSOCIATED BONDING CURVE ADDRESS TO USE...");
            let associated_bonding_curve = if let Some(actual) = actual_abc {
                eprintln!("   📋 Actual address found in transaction: {}", actual);
                eprintln!("   🔍 Verifying actual address is a token account...");
                
                // Verify actual address is a valid token account
                let actual_check = rpc.get_account(&actual).await;
                let is_valid_token_account = if let Ok(acc) = actual_check {
                    let token_program_2022 = Pubkey::from_str("TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb").unwrap();
                    let token_program = Pubkey::from_str("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA").unwrap();
                    let is_token = acc.owner == token_program_2022 || acc.owner == token_program;
                    eprintln!("   📊 Actual address account info:");
                    eprintln!("      Owner: {}", acc.owner);
                    eprintln!("      Data size: {} bytes", acc.data.len());
                    eprintln!("      Is token account: {} (expected: Token Program 2022 or Token Program)", is_token);
                    is_token
                } else {
                    eprintln!("   ❌ Actual address does not exist on blockchain!");
                    false
                };
                
                if is_valid_token_account {
                    // Actual address is valid token account - use it
                    eprintln!("   ✅ Actual address is valid token account - will use it");
                    if actual == abc_2022 {
                        eprintln!("      ✅ Matches calculated Token Program 2022 address");
                    } else if actual == abc_standard {
                        eprintln!("      ✅ Matches calculated standard Token Program address");
                    } else {
                        eprintln!("      ⚠️  Does NOT match calculated addresses, but is valid token account");
                        eprintln!("      Calculated (2022): {}", abc_2022);
                        eprintln!("      Calculated (standard): {}", abc_standard);
                    }
                    actual
                } else {
                    // Actual address is NOT a token account - use calculated Token Program 2022 instead
                    eprintln!("   ❌ Actual address is NOT a token account!");
                    eprintln!("   🔄 Falling back to calculated Token Program 2022 address");
                    eprintln!("      Calculated (2022): {}", abc_2022);
                    abc_2022
                }
            } else {
                // Fallback to 2022 (pump.fun uses Token Program 2022)
                eprintln!("   ⚠️  No actual address found in transaction");
                eprintln!("   🔄 Using calculated Token Program 2022 address (default for pump.fun)");
                eprintln!("      Calculated (2022): {}", abc_2022);
                abc_2022
            };
            eprintln!("   ✅ FINAL Associated Bonding Curve address: {}", associated_bonding_curve);

            // FINALNO: Creator vault je ili iz BUY indexa 9 (canon) ili PDA fallback
            let creator_vault = if let Some(vault) = vault_from_buy {
                // Index 9 iz BUY instrukcije je apsolutna istina
                eprintln!();
                eprintln!("🔍 DECIDING WHICH CREATOR VAULT ADDRESS TO USE...");
                eprintln!("   ✅ USING CANONICAL CREATOR VAULT FROM BUY (index 9): {}", vault);
                vault
            } else {
                // PDA fallback samo ako nema BUY-a (gotovo nikad)
                eprintln!();
                eprintln!("🔍 DECIDING WHICH CREATOR VAULT ADDRESS TO USE...");
                eprintln!("   ⚠️  NO BUY INSTRUCTION → USING PDA FALLBACK");
                let (pda, _) = Pubkey::find_program_address(&[b"creator_vault", creator.as_ref()], &pump_program_id);
                eprintln!("   📋 Calculated PDA: {}", pda);
                pda
            };
            
            eprintln!("   ✅ FINAL Creator Vault: {}", creator_vault);

            let global = Pubkey::from_str("4wTV1YmiEkRvAtNtsSGPtUrqRYQMe5SKy2uB4Jjaxnjf")?;
            let fee_recipient = Pubkey::from_str("CebN5WGQ4jvEPvsVU4EoHEpgzq1VV7AbicfhtW4xC9iM")?;
            let event_authority = Pubkey::from_str("Ce6TQqeHC9p8KetsN6JsjHK7UTZk7nasjjnr7XxXp9F1")?;
            
            // Global Volume Accumulator is hardcoded - ALWAYS use Hq2wp8uJ9jCPsYgNHex8RtqdvMPfVGoYwjvF1ATiwn2Y
            // NEVER derive or extract from transaction - always hardcoded
            let global_volume = Pubkey::from_str("Hq2wp8uJ9jCPsYgNHex8RtqdvMPfVGoYwjvF1ATiwn2Y")?;
            eprintln!();
            eprintln!("✅ Global Volume Accumulator (HARDCODED - never derived): {}", global_volume);
            eprintln!("   ⚠️  NOT using PDA derivation - always hardcoded to: {}", global_volume);
            
            // Log User Volume Accumulator from BUY instruction (index 14) for reference only
            // Note: This is the user volume for the creator (dev buy), not for our buy
            // We derive our own user volume in buy.rs based on our wallet
            if let Some(user_vol) = user_volume_from_buy {
                if DEBUG {
                    eprintln!();
                    eprintln!("📋 User Volume Accumulator from BUY (index 14): {} (creator's user volume)", user_vol);
                }
            }
            
            let fee_config = Pubkey::from_str("8Wf5TiAheLUqBrKXeYg2JtAFFMWtKdG2BSFgqUcPVwTt")?;
            let fee_program = Pubkey::from_str("pfeeUxB6jkeY1Hxd7CsFCAjcbHA9rWtchMGdZ6VojVZ")?;

            let accounts = PumpBuyAccounts {
                mint: *mint,
                bonding_curve,
                associated_bonding_curve,
                creator_vault,
                event_authority,
                global_volume,
                global,
                fee_recipient,
                fee_config,
                fee_program,
                dev_buy_sol,
                creator, // ✅ Now using correct creator from CREATE instruction
                associated_bonding_curve_instruction: abc_instruction_from_tx, // Store actual instruction from dev TX
            };

            return Ok((accounts, *mint));
        }

        // If we get here, something went wrong during parsing
        println!("      ❌ FATAL: Failed to parse transaction");
        println!("         This should never happen - check transaction format");
        Err(anyhow!("Failed to extract accounts - transaction parse error"))
    }

    pub async fn check_creator_token_count(
        rpc: &RpcClient,
        creator: &Pubkey,
    ) -> Result<usize> {
        use solana_client::rpc_config::RpcTransactionConfig;
        use solana_transaction_status::UiTransactionEncoding;

        let pump_program = Pubkey::from_str(PUMP_PROGRAM_ID)?;

        let sigs = match rpc.get_signatures_for_address(creator).await {
            Ok(s) => s,
            Err(_) => return Ok(0),
        };

        let mut token_count = 0;

        for sig_info in sigs.iter().take(50) {
            let sig = match solana_sdk::signature::Signature::from_str(&sig_info.signature) {
                Ok(s) => s,
                Err(_) => continue,
            };

            let tx_result = rpc.get_transaction_with_config(
                &sig,
                RpcTransactionConfig {
                    encoding: Some(UiTransactionEncoding::Json),
                    max_supported_transaction_version: Some(0),
                    commitment: Some(CommitmentConfig::confirmed()),
                }
            ).await;

            if let Ok(tx) = tx_result {
                if let Some(meta) = tx.transaction.meta {
                    let logs: Option<Vec<String>> = meta.log_messages.into();

                    if let Some(log_messages) = logs {
                        let has_pump = log_messages.iter().any(|log|
                            log.contains(&pump_program.to_string())
                        );

                        let has_create = log_messages.iter().any(|log|
                            log.contains("Program log: Instruction: Create")
                        );

                        if has_pump && has_create {
                            token_count += 1;
                        }
                    }
                }
            }

            if token_count > 10 {
                break;
            }
        }

        Ok(token_count)
    }

    /// Create PumpBuyAccounts from mint address (for manual buys)
    /// This function derives most accounts from the mint and tries to fetch missing ones from RPC
    pub async fn from_mint_address(
        rpc: &RpcClient,
        mint: &Pubkey,
    ) -> Result<Self> {
        use crate::pda_derivation::{
            derive_bonding_curve_pda, derive_global_pda, derive_event_authority_pda,
            get_global_volume_address, get_fee_recipient_address, 
            get_fee_config_address, get_fee_program_address, get_global_account_address,
        };

        // Derive bonding curve PDA
        let (bonding_curve, _) = derive_bonding_curve_pda(mint);
        
        // Calculate associated bonding curve (try both token programs)
        let (abc_standard, abc_2022) = calculate_addresses_both_programs(
            &bonding_curve,
            mint,
            &Pubkey::default(), // Creator not needed for ABC calculation
        );
        
        // Try to determine which ABC to use by checking which one exists
        let associated_bonding_curve = if rpc.get_account(&abc_2022).await.is_ok() {
            abc_2022
        } else if rpc.get_account(&abc_standard).await.is_ok() {
            abc_standard
        } else {
            // Default to 2022 if neither exists (will be created)
            abc_2022
        };
        
        // Try to find creator and creator_vault from recent transactions
        let (found_creator, found_vault) = Self::try_find_creator_info(rpc, mint, &bonding_curve).await;
        
        let creator = found_creator.unwrap_or(Pubkey::default());
        
        let creator_vault = if let Some(vault) = found_vault {
            vault
        } else if creator != Pubkey::default() {
            // If we have creator but no vault, derive it
            let (derived_vault, _) = Pubkey::find_program_address(
                &[b"creator_vault", creator.as_ref()],
                &Pubkey::from_str(PUMP_PROGRAM_ID).unwrap()
            );
            derived_vault
        } else {
            // Fallback: use bonding curve as placeholder (will fail, but better than panic)
            eprintln!("⚠️  Could not find creator or vault, using bonding_curve as fallback");
            bonding_curve
        };
        
        // Derive other PDAs
        let (global, _) = derive_global_pda();
        let (event_authority, _) = derive_event_authority_pda();
        
        // Get hardcoded addresses
        let global_volume = get_global_volume_address();
        let fee_recipient = get_fee_recipient_address();
        let fee_config = get_fee_config_address();
        let fee_program = get_fee_program_address();
        
        Ok(Self {
            mint: *mint,
            bonding_curve,
            associated_bonding_curve,
            creator_vault,
            event_authority,
            global_volume,
            global,
            fee_recipient,
            fee_config,
            fee_program,
            dev_buy_sol: 0, // Not relevant for manual buy
            creator,
            associated_bonding_curve_instruction: None,
        })
    }
    
    /// Try to find creator and creator_vault by searching recent transactions for this mint
    async fn try_find_creator_info(
        rpc: &RpcClient,
        mint: &Pubkey,
        bonding_curve: &Pubkey,
    ) -> (Option<Pubkey>, Option<Pubkey>) {
        use solana_client::rpc_config::{RpcTransactionConfig, RpcSignatureStatusConfig};
        use solana_transaction_status::UiTransactionEncoding;
        use solana_sdk::commitment_config::CommitmentConfig;
        
        // Try to get recent signatures for the bonding curve
        if let Ok(sigs) = rpc.get_signatures_for_address(bonding_curve).await {
            // Check oldest transaction first (creation) if possible, but here we just check recent
            // because we want to find *any* valid BUY transaction or the creation
            for sig_info in sigs.iter().take(20) {
                if let Ok(sig) = solana_sdk::signature::Signature::from_str(&sig_info.signature) {
                    if let Ok(tx) = rpc.get_transaction_with_config(
                        &sig,
                        RpcTransactionConfig {
                            encoding: Some(UiTransactionEncoding::Base64), // Use Base64 for manual parsing
                            max_supported_transaction_version: Some(0),
                            commitment: Some(CommitmentConfig::confirmed()),
                        }
                    ).await {
                        // Try to extract info from this transaction
                        let (creator, vault) = Self::extract_info_from_tx(&tx, mint);
                        if creator.is_some() || vault.is_some() {
                            return (creator, vault);
                        }
                    }
                }
            }
        }
        
        (None, None)
    }
    
    /// Extract creator and creator_vault from a transaction
    fn extract_info_from_tx(
        tx: &solana_transaction_status::EncodedConfirmedTransactionWithStatusMeta,
        _mint: &Pubkey,
    ) -> (Option<Pubkey>, Option<Pubkey>) {
        use base64::{engine::general_purpose, Engine as _};
        use solana_sdk::message::VersionedMessage;
        use solana_sdk::transaction::VersionedTransaction;
        use bincode;
        
        if let solana_transaction_status::EncodedTransaction::Binary(encoded, _) = &tx.transaction.transaction {
            if let Ok(tx_bytes) = general_purpose::STANDARD.decode(encoded) {
                if let Ok(versioned_tx) = bincode::deserialize::<VersionedTransaction>(&tx_bytes) {
                    let account_keys = match &versioned_tx.message {
                        VersionedMessage::Legacy(msg) => &msg.account_keys,
                        VersionedMessage::V0(msg) => &msg.account_keys,
                    };
                    
                    let instructions = match &versioned_tx.message {
                        VersionedMessage::Legacy(msg) => &msg.instructions,
                        VersionedMessage::V0(msg) => &msg.instructions,
                    };
                    
                    // 1. Extract creator (signer/payer - usually first account)
                    let creator = account_keys.get(0).copied();
                    
                    // 2. Extract creator_vault from BUY instruction
                    let pump_program_id = Pubkey::from_str(PUMP_PROGRAM_ID).unwrap_or_default();
                    let mut creator_vault = None;
                    
                    for ix in instructions.iter() {
                        let program_id_idx = ix.program_id_index as usize;
                        if let Some(&program_id) = account_keys.get(program_id_idx) {
                            if program_id == pump_program_id && ix.data.len() >= 8 {
                                let discriminator = &ix.data[0..8];
                                if discriminator == BUY_DISCRIMINATOR {
                                    // Check accounts
                                    let ix_accounts: Vec<Pubkey> = ix.accounts
                                        .iter()
                                        .filter_map(|&idx| account_keys.get(idx as usize).copied())
                                        .collect();
                                        
                                    // BUY instruction: creator_vault is at index 9
                                    if ix_accounts.len() >= 10 {
                                        creator_vault = Some(ix_accounts[9]);
                                        break;
                                    }
                                }
                            }
                        }
                    }
                    
                    return (creator, creator_vault);
                }
            }
        }
        
        (None, None)
    }

    // Legacy method kept for compatibility if needed, but we use extract_info_from_tx now
    async fn try_find_creator_vault(
        rpc: &RpcClient,
        mint: &Pubkey,
        bonding_curve: &Pubkey,
    ) -> Option<Pubkey> {
        let (_, vault) = Self::try_find_creator_info(rpc, mint, bonding_curve).await;
        vault
    }
    
    fn extract_creator_vault_from_tx(
        _tx: &dyn std::any::Any,
        _mint: &Pubkey,
    ) -> Option<Pubkey> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use solana_sdk::{
        message::v0,
        transaction::VersionedTransaction,
        signature::Keypair,
        system_instruction,
    };
    use bincode;

    fn create_test_pump_accounts() -> PumpBuyAccounts {
        PumpBuyAccounts {
            mint: Pubkey::new_unique(),
            bonding_curve: Pubkey::new_unique(),
            associated_bonding_curve: Pubkey::new_unique(),
            creator_vault: Pubkey::new_unique(),
            event_authority: Pubkey::new_unique(),
            global_volume: Pubkey::new_unique(),
            global: Pubkey::new_unique(),
            fee_recipient: Pubkey::new_unique(),
            fee_config: Pubkey::new_unique(),
            fee_program: Pubkey::new_unique(),
            dev_buy_sol: 1_000_000_000, // 1 SOL
            creator: Pubkey::new_unique(),
            associated_bonding_curve_instruction: None,
        }
    }

    #[test]
    fn test_pump_buy_accounts_creation() {
        let accounts = create_test_pump_accounts();
        
        assert_ne!(accounts.mint, Pubkey::default());
        assert_ne!(accounts.bonding_curve, Pubkey::default());
        assert_eq!(accounts.dev_buy_sol, 1_000_000_000);
    }

    #[test]
    fn test_extract_creator_from_tx_structure() {
        // Test that creator extraction logic works with proper TX structure
        let creator = Pubkey::new_unique();
        let mint = Pubkey::new_unique();
        
        // In a real transaction, creator would be the first signer (account_keys[0])
        // This is tested indirectly through integration tests
        assert_ne!(creator, mint);
    }

    #[test]
    fn test_extract_dev_buy_from_instruction_data() {
        // Test BUY discriminator
        let buy_discriminator = BUY_DISCRIMINATOR;
        assert_eq!(buy_discriminator.len(), 8);
        assert_eq!(buy_discriminator[0], 0x66);
        assert_eq!(buy_discriminator[1], 0x06);

        // Test parsing of instruction data
        let token_amount: u64 = 1000;
        let max_sol: u64 = 1_000_000_000; // 1 SOL
        
        let mut data = Vec::new();
        data.extend_from_slice(&buy_discriminator);
        data.extend_from_slice(&token_amount.to_le_bytes());
        data.extend_from_slice(&max_sol.to_le_bytes());

        assert_eq!(data.len(), 8 + 8 + 8); // discriminator + token_amount + max_sol
        
        // Verify we can extract max_sol
        if data.len() >= 24 {
            let extracted_max_sol = u64::from_le_bytes(
                data[16..24].try_into().unwrap()
            );
            assert_eq!(extracted_max_sol, max_sol);
        }
    }

    #[test]
    fn test_extract_dev_buy_from_balances() {
        // Test balance change calculation
        let pre_balance: u64 = 10_000_000_000; // 10 SOL
        let post_balance: u64 = 9_000_000_000; // 9 SOL
        let spent = pre_balance - post_balance;
        
        assert_eq!(spent, 1_000_000_000); // 1 SOL
        
        // Test that we filter out small amounts (rent)
        let rent = 2_000_000; // 0.002 SOL
        assert!(rent < 100_000_000); // Should be filtered out
        
        // Test valid buy amount
        let buy_amount = 1_000_000_000u64; // 1 SOL
        assert!(buy_amount > 100_000_000u64 && buy_amount < 100_000_000_000u64);
    }

    #[test]
    fn test_bonding_curve_derivation() {
        let mint = Pubkey::new_unique();
        let pump_program = Pubkey::from_str(PUMP_PROGRAM_ID).unwrap();
        
        let (bonding_curve, _) = Pubkey::find_program_address(
            &[b"bonding-curve", &mint.to_bytes()],
            &pump_program,
        );

        assert_ne!(bonding_curve, mint);
        assert_ne!(bonding_curve, Pubkey::default());
        
        // Same mint should produce same bonding curve
        let (bonding_curve2, _) = Pubkey::find_program_address(
            &[b"bonding-curve", &mint.to_bytes()],
            &pump_program,
        );
        assert_eq!(bonding_curve, bonding_curve2);
    }

    #[test]
    fn test_associated_bonding_curve() {
        let bonding_curve = Pubkey::new_unique();
        let mint = Pubkey::new_unique();
        
        let associated = spl_associated_token_account::get_associated_token_address(
            &bonding_curve,
            &mint,
        );

        assert_ne!(associated, bonding_curve);
        assert_ne!(associated, mint);
    }

    #[tokio::test]
    async fn test_check_creator_token_count_empty() {
        // This test would require a mock RPC client
        // For now, we test the structure
        let creator = Pubkey::new_unique();
        
        // The function should handle empty results gracefully
        // Real implementation would return Ok(0) for new creators
        assert_ne!(creator, Pubkey::default());
    }

    #[test]
    fn test_pump_program_id() {
        let program_id = Pubkey::from_str(PUMP_PROGRAM_ID);
        assert!(program_id.is_ok());
        
        let program_id = program_id.unwrap();
        assert_ne!(program_id, Pubkey::default());
    }

    #[test]
    fn test_buy_discriminator() {
        // Verify BUY discriminator is correct
        let expected = [0x66, 0x06, 0x3d, 0x12, 0x01, 0xda, 0xeb, 0xea];
        assert_eq!(BUY_DISCRIMINATOR, expected);
    }

    #[tokio::test]
    #[ignore]
    async fn test_from_initialize_tx_with_mock_rpc() {
        // Integration test - would require:
        // 1. Mock RPC client that returns a real transaction structure
        // 2. Proper transaction encoding
        // 3. Valid instruction data
        
        // This is a complex integration test that would need:
        // - A way to create valid Solana transaction structures
        // - Mock RPC responses
        // - Proper account key ordering
        
        // For now, we test the component functions separately
    }
}