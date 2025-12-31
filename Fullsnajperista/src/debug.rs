// debug.rs - Debug utilities for comparing Pump.fun buy transactions
#![allow(dead_code, unused_imports)]

use anyhow::{Result, anyhow};
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::{
    pubkey::Pubkey,
    instruction::{AccountMeta, Instruction},
    commitment_config::CommitmentConfig,
    signature::Signature,
};
use solana_transaction_status::UiTransactionEncoding;
use std::str::FromStr;
use base64::{engine::general_purpose, Engine as _};
use solana_sdk::message::VersionedMessage;
use solana_sdk::transaction::VersionedTransaction;

use crate::constants::{PUMP_PROGRAM_ID, BUY_DISCRIMINATOR, SELL_DISCRIMINATOR};
use crate::pda_derivation;
use solana_sdk::system_program;

/// Extracted buy instruction information from a transaction
#[derive(Debug, Clone)]
pub struct BuyInstructionInfo {
    pub program_id: Pubkey,
    pub accounts: Vec<AccountInfo>,
    pub data: Vec<u8>,
    pub discriminator: [u8; 8],
    pub token_amount: Option<u64>,
    pub max_sol_cost: Option<u64>,
}

/// Account information with metadata
#[derive(Debug, Clone)]
pub struct AccountInfo {
    pub index: usize,
    pub pubkey: Pubkey,
    pub is_signer: bool,
    pub is_writable: bool,
    pub label: Option<String>, // Human-readable label if known
}

impl BuyInstructionInfo {
    /// Get account by index
    pub fn get_account(&self, index: usize) -> Option<&AccountInfo> {
        self.accounts.get(index)
    }

    /// Get account pubkey by index
    pub fn get_account_pubkey(&self, index: usize) -> Option<Pubkey> {
        self.accounts.get(index).map(|a| a.pubkey)
    }

    /// Get PDA accounts
    pub fn get_pda_accounts(&self) -> Vec<(usize, Pubkey, &str)> {
        let mut pdas = Vec::new();
        
        // Known PDA indices based on Pump.fun structure
        if let Some(global) = self.get_account_pubkey(0) {
            pdas.push((0, global, "Global"));
        }
        if let Some(bc) = self.get_account_pubkey(3) {
            pdas.push((3, bc, "Bonding Curve"));
        }
        if let Some(ea) = self.get_account_pubkey(10) {
            pdas.push((10, ea, "Event Authority"));
        }
        if let Some(uv) = self.get_account_pubkey(13) {
            pdas.push((13, uv, "User Volume"));
        }
        
        pdas
    }
}

/// Extract buy instruction from a transaction signature
/// Accepts both transaction signatures and wallet addresses (will try to find recent transactions)
pub async fn extract_buy_instruction_from_tx(
    rpc: &RpcClient,
    signature: &str,
) -> Result<BuyInstructionInfo> {
    // Try to parse as signature first
    let sig = match Signature::from_str(signature) {
        Ok(s) => s,
        Err(_) => {
            // If it's not a valid signature, it might be a wallet address
            // Try to find recent transactions from this wallet
            return Err(anyhow!(
                "Invalid transaction signature: '{}'\n\
                Transaction signatures are 87-88 characters long.\n\
                This looks like a wallet address (44 characters).\n\
                Please provide the transaction signature from:\n\
                - Application logs (look for 'Signature:' or 'Link:')\n\
                - Solscan.io (open your wallet and find the transaction)\n\
                - Previous terminal output",
                signature
            ));
        }
    };
    
    let tx = rpc.get_transaction_with_config(
        &sig,
        solana_client::rpc_config::RpcTransactionConfig {
            encoding: Some(UiTransactionEncoding::Base64),
            max_supported_transaction_version: Some(0),
            commitment: Some(CommitmentConfig::confirmed()),
        }
    ).await?;

    if let solana_transaction_status::EncodedTransaction::Binary(encoded, _) = &tx.transaction.transaction {
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

        eprintln!("   Looking for Pump program: {}", pump_program);
        eprintln!("   Total instructions in transaction: {}", instructions.len());
        
        // Log all programs in transaction
        eprintln!("   Programs in transaction:");
        for (ix_idx, ix) in instructions.iter().enumerate() {
            let program_id_idx = ix.program_id_index as usize;
            if let Some(&program_id) = account_keys.get(program_id_idx) {
                eprintln!("      [{}] Program: {} (data len: {})", ix_idx, program_id, ix.data.len());
            }
        }

        // Find buy instruction
        let mut found_pump_instructions = 0;
        for (ix_idx, ix) in instructions.iter().enumerate() {
            let program_id_idx = ix.program_id_index as usize;
            if let Some(&program_id) = account_keys.get(program_id_idx) {
                if program_id == pump_program {
                    found_pump_instructions += 1;
                    if ix.data.len() >= 8 {
                        let discriminator = &ix.data[0..8];
                        
                        eprintln!("   Found Pump instruction [{}]: discriminator={:02x?}", ix_idx, discriminator);
                        
                        // Check if this is a buy instruction
                        // Note: We'll check both possible discriminators
                        let is_buy = discriminator == &BUY_DISCRIMINATOR
                            || discriminator == &[0x33, 0xE6, 0x85, 0x5A, 0x5B, 0x6B, 0xBD, 0x5B]; // Alternative discriminator
                        
                        eprintln!("   Expected BUY discriminator: {:02x?}", BUY_DISCRIMINATOR);
                        eprintln!("   Is buy: {}", is_buy);
                    
                    if is_buy {
                        // Extract accounts
                        // Determine signers from transaction message
                        let num_required_signatures = match &versioned_tx.message {
                            VersionedMessage::Legacy(msg) => msg.header.num_required_signatures as usize,
                            VersionedMessage::V0(msg) => msg.header.num_required_signatures as usize,
                        };
                        
                        let mut accounts = Vec::new();
                        for (idx, &account_idx) in ix.accounts.iter().enumerate() {
                            if let Some(&pubkey) = account_keys.get(account_idx as usize) {
                                // Determine if signer: first N accounts in transaction are signers
                                let is_signer = (account_idx as usize) < num_required_signatures;
                                
                                // Determine writable from instruction structure
                                // In Solana, writable accounts are typically those that can be modified
                                // We'll use heuristics: known readonly accounts are System Program, Token Program, etc.
                                let readonly_accounts = vec![
                                    system_program::id(),
                                    Pubkey::from_str("TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb").unwrap_or_default(),
                                    Pubkey::from_str("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA").unwrap_or_default(),
                                    pump_program,
                                ];
                                let is_writable = !readonly_accounts.contains(&pubkey);
                                
                                let label = get_account_label(idx, &pubkey);
                                accounts.push(AccountInfo {
                                    index: idx,
                                    pubkey,
                                    is_signer,
                                    is_writable,
                                    label,
                                });
                            }
                        }

                        // Extract instruction data
                        let data = ix.data.clone();
                        let mut discriminator = [0u8; 8];
                        discriminator.copy_from_slice(&data[0..8]);
                        
                        let token_amount = if data.len() >= 16 {
                            Some(u64::from_le_bytes(data[8..16].try_into().unwrap()))
                        } else {
                            None
                        };
                        
                        let max_sol_cost = if data.len() >= 24 {
                            Some(u64::from_le_bytes(data[16..24].try_into().unwrap()))
                        } else {
                            None
                        };

                        return Ok(BuyInstructionInfo {
                            program_id: pump_program,
                            accounts,
                            data,
                            discriminator,
                            token_amount,
                            max_sol_cost,
                        });
                    } else {
                        eprintln!("   Not a buy instruction (discriminator mismatch)");
                    }
                    } else {
                        eprintln!("   Pump instruction [{}] has insufficient data (len: {})", ix_idx, ix.data.len());
                    }
                }
            }
        }
        
        eprintln!("   Total Pump instructions found: {}", found_pump_instructions);
        Err(anyhow!("Buy instruction not found in transaction (found {} Pump instructions)", found_pump_instructions))
    } else {
        Err(anyhow!("Transaction encoding not supported"))
    }
}

/// Get human-readable label for account at given index
fn get_account_label(index: usize, pubkey: &Pubkey) -> Option<String> {
    let labels: Vec<(&str, &str)> = vec![
        ("4wTV1YmiEkRvAtNtsSGPtUrqRYQMe5SKy2uB4Jjaxnjf", "Global"),
        ("CebN5WGQ4jvEPvsVU4EoHEpgzq1VV7AbicfhtW4xC9iM", "Fee Recipient"),
        ("11111111111111111111111111111111", "System Program"),
        ("TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb", "Token Program 2022"),
        ("Ce6TQqeHC9p8KetsN6JsjHK7UTZk7nasjjnr7XxXp9F1", "Event Authority"),
        ("Hq2wp8uJ9jCPsYgNHex8RtqdvMPfVGoYwjvF1ATiwn2Y", "Global Volume"),
        ("8Wf5TiAheLUqBrKXeYg2JtAFFMWtKdG2BSFgqUcPVwTt", "Fee Config"),
        ("pfeeUxB6jkeY1Hxd7CsFCAjcbHA9rWtchMGdZ6VojVZ", "Fee Program"),
        (PUMP_PROGRAM_ID, "Pump Program"),
    ];
    
    let pubkey_str = pubkey.to_string();
    for (addr, label) in labels {
        if addr == pubkey_str {
            return Some(label.to_string());
        }
    }
    
    // Index-based labels
    match index {
        0 => Some("Global".to_string()),
        1 => Some("Fee Recipient".to_string()),
        2 => Some("Mint".to_string()),
        3 => Some("Bonding Curve".to_string()),
        4 => Some("Associated Bonding Curve".to_string()),
        5 => Some("User Token Account".to_string()),
        6 => Some("User Wallet".to_string()),
        7 => Some("System Program".to_string()),
        8 => Some("Token Program".to_string()),
        9 => Some("Creator Vault".to_string()),
        10 => Some("Event Authority".to_string()),
        11 => Some("Pump Program".to_string()),
        12 => Some("Global Volume".to_string()),
        13 => Some("User Volume".to_string()),
        14 => Some("Fee Config".to_string()),
        15 => Some("Fee Program".to_string()),
        _ => None,
    }
}

/// Compare two buy instructions and report differences
pub fn compare_instructions(
    successful: &BuyInstructionInfo,
    failed: &BuyInstructionInfo,
) -> ComparisonReport {
    let mut report = ComparisonReport::new();
    
    // Compare account count
    if successful.accounts.len() != failed.accounts.len() {
        report.add_difference(format!(
            "Account count mismatch: successful={}, failed={}",
            successful.accounts.len(),
            failed.accounts.len()
        ));
    }
    
    // Compare each account
    let max_len = successful.accounts.len().max(failed.accounts.len());
    for i in 0..max_len {
        let succ_acc = successful.accounts.get(i);
        let fail_acc = failed.accounts.get(i);
        
        match (succ_acc, fail_acc) {
            (Some(s), Some(f)) => {
                if s.pubkey != f.pubkey {
                    report.add_account_mismatch(
                        i,
                        s.pubkey,
                        f.pubkey,
                        s.label.clone(),
                    );
                }
                if s.is_signer != f.is_signer {
                    report.add_flag_mismatch(
                        i,
                        "signer",
                        s.is_signer,
                        f.is_signer,
                    );
                }
                if s.is_writable != f.is_writable {
                    report.add_flag_mismatch(
                        i,
                        "writable",
                        s.is_writable,
                        f.is_writable,
                    );
                }
            }
            (Some(s), None) => {
                report.add_difference(format!(
                    "Index {}: Successful has account {} but failed is missing",
                    i, s.pubkey
                ));
            }
            (None, Some(f)) => {
                report.add_difference(format!(
                    "Index {}: Failed has account {} but successful is missing",
                    i, f.pubkey
                ));
            }
            (None, None) => {}
        }
    }
    
    // Compare instruction data
    if successful.discriminator != failed.discriminator {
        report.add_difference(format!(
            "Discriminator mismatch: successful={:02x?}, failed={:02x?}",
            successful.discriminator, failed.discriminator
        ));
    }
    
    if successful.token_amount != failed.token_amount {
        report.add_difference(format!(
            "Token amount mismatch: successful={:?}, failed={:?}",
            successful.token_amount, failed.token_amount
        ));
    }
    
    if successful.max_sol_cost != failed.max_sol_cost {
        report.add_difference(format!(
            "Max SOL cost mismatch: successful={:?}, failed={:?}",
            successful.max_sol_cost, failed.max_sol_cost
        ));
    }
    
    report
}

/// Comparison report with all differences
#[derive(Debug, Clone)]
pub struct ComparisonReport {
    pub differences: Vec<String>,
    pub account_mismatches: Vec<(usize, Pubkey, Pubkey, Option<String>)>,
    pub flag_mismatches: Vec<(usize, String, bool, bool)>,
}

impl ComparisonReport {
    pub fn new() -> Self {
        Self {
            differences: Vec::new(),
            account_mismatches: Vec::new(),
            flag_mismatches: Vec::new(),
        }
    }
    
    pub fn add_difference(&mut self, diff: String) {
        self.differences.push(diff);
    }
    
    pub fn add_account_mismatch(&mut self, index: usize, expected: Pubkey, actual: Pubkey, label: Option<String>) {
        self.account_mismatches.push((index, expected, actual, label));
    }
    
    pub fn add_flag_mismatch(&mut self, index: usize, flag: &str, expected: bool, actual: bool) {
        self.flag_mismatches.push((index, flag.to_string(), expected, actual));
    }
    
    pub fn has_differences(&self) -> bool {
        !self.differences.is_empty()
            || !self.account_mismatches.is_empty()
            || !self.flag_mismatches.is_empty()
    }
    
    pub fn print_report(&self) {
        println!("\n╔═══════════════════════════════════════════════════════════════╗");
        println!("║              TRANSACTION COMPARISON REPORT                    ║");
        println!("╚═══════════════════════════════════════════════════════════════╝\n");
        
        if !self.has_differences() {
            println!("✅ No differences found - transactions are identical!");
            return;
        }
        
        if !self.differences.is_empty() {
            println!("📋 General Differences:");
            for diff in &self.differences {
                println!("   ❌ {}", diff);
            }
            println!();
        }
        
        if !self.account_mismatches.is_empty() {
            println!("🔍 Account Mismatches:");
            for (index, expected, actual, label) in &self.account_mismatches {
                let label_str = label.as_ref()
                    .map(|l| format!(" ({})", l))
                    .unwrap_or_default();
                println!("   Index {}{}:", index, label_str);
                println!("      Expected: {}", expected);
                println!("      Got:      {}", actual);
            }
            println!();
        }
        
        if !self.flag_mismatches.is_empty() {
            println!("🚩 Flag Mismatches:");
            for (index, flag, expected, actual) in &self.flag_mismatches {
                println!("   Index {} ({}): expected={}, got={}", index, flag, expected, actual);
            }
            println!();
        }
    }
}

/// Analyze a single transaction in detail
pub async fn analyze_single_transaction(
    rpc: &RpcClient,
    tx_sig: &str,
) -> Result<()> {
    println!("🔍 Analyzing transaction: {}", tx_sig);
    println!();
    
    let instruction = extract_buy_instruction_from_tx(rpc, tx_sig).await?;
    
    // Extract mint and user wallet
    let mint = instruction.get_account_pubkey(2)
        .ok_or_else(|| anyhow!("Mint not found in transaction"))?;
    let user_wallet = instruction.get_account_pubkey(6)
        .ok_or_else(|| anyhow!("User wallet not found in transaction"))?;
    
    println!("╔═══════════════════════════════════════════════════════════════╗");
    println!("║              TRANSACTION ANALYSIS REPORT                    ║");
    println!("╚═══════════════════════════════════════════════════════════════╝\n");
    
    println!("📋 Transaction Signature: {}", tx_sig);
    println!("   Mint: {}", mint);
    println!("   User Wallet: {}", user_wallet);
    println!();
    
    println!("📊 Accounts ({}):", instruction.accounts.len());
    for acc in &instruction.accounts {
        let label = acc.label.as_ref()
            .map(|l| format!(" ({})", l))
            .unwrap_or_default();
        let signer_str = if acc.is_signer { " [SIGNER]" } else { "" };
        let writable_str = if acc.is_writable { " [WRITABLE]" } else { " [READONLY]" };
        println!("   [{}] {}{}{}{}", acc.index, acc.pubkey, label, signer_str, writable_str);
    }
    println!();
    
    println!("📋 Instruction Data:");
    println!("   Discriminator: {:02x?}", instruction.discriminator);
    if let Some(token_amount) = instruction.token_amount {
        println!("   Token Amount: {} ({} tokens)", token_amount, token_amount as f64 / 1_000_000_000.0);
    } else {
        println!("   Token Amount: None");
    }
    if let Some(max_sol_cost) = instruction.max_sol_cost {
        println!("   Max SOL Cost: {} lamports ({} SOL)", max_sol_cost, max_sol_cost as f64 / 1_000_000_000.0);
    } else {
        println!("   Max SOL Cost: None");
    }
    println!("   Data Length: {} bytes", instruction.data.len());
    println!();
    
    // Verify PDA derivations
    println!("🔑 Verifying PDA derivations:");
    println!("   Mint: {}", mint);
    println!("   User Wallet: {}", user_wallet);
    let pda_report = verify_pda_derivations(&instruction, &mint, &user_wallet);
    pda_report.print_report();
    
    // Test User Volume PDA seeds
    if let Some(actual_uv) = instruction.get_account_pubkey(13) {
        println!("🧪 Testing User Volume PDA seed combinations:");
        println!("   Actual User Volume: {}", actual_uv);
        println!();
        
        let pump_program = Pubkey::from_str(PUMP_PROGRAM_ID)?;
        let wallet_bytes = user_wallet.to_bytes();
        let wallet_ref = user_wallet.as_ref();
        let mint_bytes = mint.to_bytes();
        let mint_ref = mint.as_ref();
        
        let test_seeds: Vec<(&str, Vec<&[u8]>)> = vec![
            // ✅ POBJEDNIK (iz pumpfun-rs SDK)
            ("user_volume_accumulator + wallet as_ref", vec![b"user_volume_accumulator", wallet_ref]),
            
            // Ostale kombinacije za testiranje
            ("user_volume + wallet as_ref", vec![b"user_volume", wallet_ref]),
            ("user_volume + wallet to_bytes", vec![b"user_volume", &wallet_bytes]),
            ("user-trade-history + wallet as_ref", vec![b"user-trade-history", wallet_ref]),
            ("user-trade-history + wallet to_bytes", vec![b"user-trade-history", &wallet_bytes]),
            ("user-trade-history + mint as_ref", vec![b"user-trade-history", mint_ref]),
            ("user-trade-history + mint to_bytes", vec![b"user-trade-history", &mint_bytes]),
            ("user_volume + mint as_ref", vec![b"user_volume", mint_ref]),
            ("user_volume + mint to_bytes", vec![b"user_volume", &mint_bytes]),
            ("volume + wallet as_ref", vec![b"volume", wallet_ref]),
            ("volume + wallet to_bytes", vec![b"volume", &wallet_bytes]),
            ("user-trade-history reversed wallet", vec![wallet_ref, b"user-trade-history"]),
            ("user_volume reversed wallet", vec![wallet_ref, b"user_volume"]),
            ("user_volume_history + wallet as_ref", vec![b"user_volume_history", wallet_ref]),
            ("trade-history + wallet as_ref", vec![b"trade-history", wallet_ref]),
            
            // Kombinacije s mintom (za slučaj da se promijeni)
            ("user_volume + mint + wallet", vec![b"user_volume", mint_ref, wallet_ref]),
            ("user-trade-history + mint + wallet", vec![b"user-trade-history", mint_ref, wallet_ref]),
        ];
        
        let mut found_match = false;
        for (name, seeds_vec) in test_seeds {
            let seeds: Vec<&[u8]> = seeds_vec.iter().map(|s| *s).collect();
            let (pda, _) = Pubkey::find_program_address(&seeds, &pump_program);
            let matches = pda == actual_uv;
            if matches {
                if name == "user_volume + wallet as_ref" {
                    println!("      ✅✅✅ MATCH (POTVRĐENO): {} -> {}", name, pda);
                } else {
                    println!("      ✅ MATCH: {} -> {}", name, pda);
                }
                found_match = true;
            } else {
                println!("      ❌ {} -> {} (expected: {})", name, pda, actual_uv);
            }
        }
        
        if !found_match {
            println!();
            println!("   ⚠️  No seed combination matched the actual User Volume PDA!");
            println!("   This might indicate:");
            println!("   - User Volume uses a different seed format");
            println!("   - User Volume is not a PDA (hardcoded address)");
            println!("   - Additional seeds are required");
        }
    }
    
    // Show PDA accounts
    println!("\n🔍 PDA Accounts in Transaction:");
    for (index, pubkey, name) in instruction.get_pda_accounts() {
        println!("   [{}] {} ({})", index, pubkey, name);
    }
    
    Ok(())
}

/// Debug function to compare two transactions
pub async fn debug_compare_transactions(
    rpc: &RpcClient,
    successful_sig: &str,
    failed_sig: &str,
) -> Result<()> {
    println!("🔍 Extracting buy instruction from successful transaction...");
    let successful = extract_buy_instruction_from_tx(rpc, successful_sig).await?;
    
    println!("🔍 Extracting buy instruction from failed transaction...");
    let failed = extract_buy_instruction_from_tx(rpc, failed_sig).await?;
    
    // Extract mint from transactions
    let mint = successful.get_account_pubkey(2)
        .or_else(|| failed.get_account_pubkey(2))
        .ok_or_else(|| anyhow!("Mint not found in transactions"))?;
    
    println!("\n📊 Successful Transaction Accounts ({}):", successful.accounts.len());
    for acc in &successful.accounts {
        let label = acc.label.as_ref()
            .map(|l| format!(" ({})", l))
            .unwrap_or_default();
        let signer_str = if acc.is_signer { " [SIGNER]" } else { "" };
        let writable_str = if acc.is_writable { " [WRITABLE]" } else { " [READONLY]" };
        println!("   [{}] {}{}{}{}", acc.index, acc.pubkey, label, signer_str, writable_str);
    }
    
    println!("\n📊 Failed Transaction Accounts ({}):", failed.accounts.len());
    for acc in &failed.accounts {
        let label = acc.label.as_ref()
            .map(|l| format!(" ({})", l))
            .unwrap_or_default();
        let signer_str = if acc.is_signer { " [SIGNER]" } else { "" };
        let writable_str = if acc.is_writable { " [WRITABLE]" } else { " [READONLY]" };
        println!("   [{}] {}{}{}{}", acc.index, acc.pubkey, label, signer_str, writable_str);
    }
    
    println!("\n🔍 Comparing transactions...");
    let report = compare_instructions(&successful, &failed);
    report.print_report();
    
    // Get user wallets from each transaction separately
    let successful_user_wallet = successful.get_account_pubkey(6)
        .ok_or_else(|| anyhow!("User wallet not found in successful transaction"))?;
    let failed_user_wallet = failed.get_account_pubkey(6)
        .ok_or_else(|| anyhow!("User wallet not found in failed transaction"))?;
    
    // Verify PDA derivations for successful transaction
    println!("\n🔑 Verifying PDA derivations for successful transaction:");
    println!("   Mint: {}", mint);
    println!("   User Wallet: {}", successful_user_wallet);
    let successful_pda_report = verify_pda_derivations(&successful, &mint, &successful_user_wallet);
    successful_pda_report.print_report();
    
    // Verify PDA derivations for failed transaction
    println!("\n🔑 Verifying PDA derivations for failed transaction:");
    println!("   Mint: {}", mint);
    println!("   User Wallet: {}", failed_user_wallet);
    let failed_pda_report = verify_pda_derivations(&failed, &mint, &failed_user_wallet);
    failed_pda_report.print_report();
    
    // Test different seed combinations for User Volume PDA
    println!("\n🧪 Testing User Volume PDA seed combinations:");
    let successful_uv = successful.get_account_pubkey(13).unwrap();
    let failed_uv = failed.get_account_pubkey(13).unwrap();
    println!("   Successful transaction User Volume: {}", successful_uv);
    println!("   Failed transaction User Volume: {}", failed_uv);
    println!();
    
    let pump_program = Pubkey::from_str(PUMP_PROGRAM_ID)?;
    
    // Test seeds for successful transaction
    println!("   Testing seeds for successful transaction (user: {}):", successful_user_wallet);
    let successful_wallet_bytes = successful_user_wallet.to_bytes();
    let successful_wallet_ref = successful_user_wallet.as_ref();
    
    // Test with mint as well (maybe User Volume uses mint instead of wallet?)
    let mint_bytes = mint.to_bytes();
    let mint_ref = mint.as_ref();
    
    let test_seeds_successful: Vec<(&str, Vec<&[u8]>)> = vec![
        // ✅ POBJEDNIK (iz pumpfun-rs SDK)
        ("user_volume_accumulator + wallet as_ref", vec![b"user_volume_accumulator", successful_wallet_ref]),
        
        // Ostale kombinacije
        ("user_volume + wallet as_ref", vec![b"user_volume", successful_wallet_ref]),
        ("user_volume + wallet to_bytes", vec![b"user_volume", &successful_wallet_bytes]),
        ("user-trade-history + wallet as_ref", vec![b"user-trade-history", successful_wallet_ref]),
        ("user-trade-history + wallet to_bytes", vec![b"user-trade-history", &successful_wallet_bytes]),
        ("user-trade-history + mint as_ref", vec![b"user-trade-history", mint_ref]),
        ("user-trade-history + mint to_bytes", vec![b"user-trade-history", &mint_bytes]),
        ("user_volume + mint as_ref", vec![b"user_volume", mint_ref]),
        ("user_volume + mint to_bytes", vec![b"user_volume", &mint_bytes]),
        ("volume + wallet as_ref", vec![b"volume", successful_wallet_ref]),
        ("volume + wallet to_bytes", vec![b"volume", &successful_wallet_bytes]),
        ("user-trade-history reversed wallet", vec![successful_wallet_ref, b"user-trade-history"]),
        ("user_volume reversed wallet", vec![successful_wallet_ref, b"user_volume"]),
        ("user_volume_history + wallet as_ref", vec![b"user_volume_history", successful_wallet_ref]),
        ("trade-history + wallet as_ref", vec![b"trade-history", successful_wallet_ref]),
        ("user_volume + mint + wallet", vec![b"user_volume", mint_ref, successful_wallet_ref]),
        ("user-trade-history + mint + wallet", vec![b"user-trade-history", mint_ref, successful_wallet_ref]),
    ];
    
    for (name, seeds_vec) in test_seeds_successful {
        let seeds: Vec<&[u8]> = seeds_vec.iter().map(|s| *s).collect();
        let (pda, _) = Pubkey::find_program_address(&seeds, &pump_program);
        let matches = pda == successful_uv;
        if matches {
            if name == "user_volume + wallet as_ref" {
                println!("      ✅✅✅ MATCH (POTVRĐENO): {} -> {}", name, pda);
            } else {
                println!("      ✅ MATCH: {} -> {}", name, pda);
            }
        } else {
            println!("      ❌ {} -> {} (expected: {})", name, pda, successful_uv);
        }
    }
    
    // Test seeds for failed transaction
    println!("\n   Testing seeds for failed transaction (user: {}):", failed_user_wallet);
    let failed_wallet_bytes = failed_user_wallet.to_bytes();
    let failed_wallet_ref = failed_user_wallet.as_ref();
    
    let test_seeds_failed: Vec<(&str, Vec<&[u8]>)> = vec![
        // ✅ POBJEDNIK (iz pumpfun-rs SDK)
        ("user_volume_accumulator + wallet as_ref", vec![b"user_volume_accumulator", failed_wallet_ref]),
        
        // Ostale kombinacije
        ("user_volume + wallet as_ref", vec![b"user_volume", failed_wallet_ref]),
        ("user_volume + wallet to_bytes", vec![b"user_volume", &failed_wallet_bytes]),
        ("user-trade-history + wallet as_ref", vec![b"user-trade-history", failed_wallet_ref]),
        ("user-trade-history + wallet to_bytes", vec![b"user-trade-history", &failed_wallet_bytes]),
        ("volume + wallet as_ref", vec![b"volume", failed_wallet_ref]),
        ("volume + wallet to_bytes", vec![b"volume", &failed_wallet_bytes]),
    ];
    
    for (name, seeds_vec) in test_seeds_failed {
        let seeds: Vec<&[u8]> = seeds_vec.iter().map(|s| *s).collect();
        let (pda, _) = Pubkey::find_program_address(&seeds, &pump_program);
        let matches = pda == failed_uv;
        if matches {
            if name == "user_volume + wallet as_ref" {
                println!("      ✅✅✅ MATCH (POTVRĐENO): {} -> {}", name, pda);
            } else {
                println!("      ✅ MATCH: {} -> {}", name, pda);
            }
        } else {
            println!("      ❌ {} -> {} (expected: {})", name, pda, failed_uv);
        }
    }
    
    // Compare instruction data
    println!("\n📋 Instruction Data Comparison:");
    println!("   Successful discriminator: {:02x?}", successful.discriminator);
    println!("   Failed discriminator:     {:02x?}", failed.discriminator);
    println!("   Successful token_amount:  {:?}", successful.token_amount);
    println!("   Failed token_amount:      {:?}", failed.token_amount);
    println!("   Successful max_sol_cost:  {:?}", successful.max_sol_cost);
    println!("   Failed max_sol_cost:      {:?}", failed.max_sol_cost);
    
    Ok(())
}

/// Verify PDA derivations from a buy instruction
pub fn verify_pda_derivations(
    instruction: &BuyInstructionInfo,
    mint: &Pubkey,
    user_wallet: &Pubkey,
) -> PdaVerificationReport {
    let mut report = PdaVerificationReport::new();
    
    // Verify Global PDA (index 0)
    if let Some(actual_global) = instruction.get_account_pubkey(0) {
        let (expected_global, _) = pda_derivation::derive_global_pda();
        report.add_check(
            "Global",
            0,
            expected_global,
            actual_global,
            expected_global == actual_global,
        );
    }
    
    // Verify Bonding Curve PDA (index 3)
    if let Some(actual_bc) = instruction.get_account_pubkey(3) {
        let (expected_bc, _) = pda_derivation::derive_bonding_curve_pda(mint);
        report.add_check(
            "Bonding Curve",
            3,
            expected_bc,
            actual_bc,
            expected_bc == actual_bc,
        );
    }
    
    // Verify Event Authority PDA (index 10)
    if let Some(actual_ea) = instruction.get_account_pubkey(10) {
        let (expected_ea, _) = pda_derivation::derive_event_authority_pda();
        report.add_check(
            "Event Authority",
            10,
            expected_ea,
            actual_ea,
            expected_ea == actual_ea,
        );
    }
    
    // Verify User Volume PDA (index 13)
    if let Some(actual_uv) = instruction.get_account_pubkey(13) {
        let (expected_uv, _) = pda_derivation::derive_user_volume_pda(user_wallet);
        report.add_check(
            "User Volume",
            13,
            expected_uv,
            actual_uv,
            expected_uv == actual_uv,
        );
    }
    
    report
}

/// PDA verification report
#[derive(Debug, Clone)]
pub struct PdaVerificationReport {
    pub checks: Vec<(String, usize, Pubkey, Pubkey, bool)>,
}

impl PdaVerificationReport {
    pub fn new() -> Self {
        Self {
            checks: Vec::new(),
        }
    }
    
    pub fn add_check(&mut self, name: &str, index: usize, expected: Pubkey, actual: Pubkey, matches: bool) {
        self.checks.push((name.to_string(), index, expected, actual, matches));
    }
    
    pub fn all_match(&self) -> bool {
        self.checks.iter().all(|(_, _, _, _, matches)| *matches)
    }
    
    pub fn print_report(&self) {
        println!("\n╔═══════════════════════════════════════════════════════════════╗");
        println!("║              PDA DERIVATION VERIFICATION                      ║");
        println!("╚═══════════════════════════════════════════════════════════════╝\n");
        
        for (name, index, expected, actual, matches) in &self.checks {
            if *matches {
                println!("   [{}] {} ✅ VERIFIED", index, name);
                println!("        Expected: {}", expected);
                println!("        Actual:   {}", actual);
            } else {
                println!("   [{}] {} ❌ MISMATCH", index, name);
                println!("        Expected: {}", expected);
                println!("        Actual:   {}", actual);
            }
            println!();
        }
        
        if self.all_match() {
            println!("✅ All PDAs verified correctly!");
        } else {
            println!("❌ Some PDAs do not match expected derivations!");
        }
    }
}

/// Extract sell instruction from a transaction signature
pub async fn extract_sell_instruction_from_tx(
    rpc: &RpcClient,
    signature: &str,
) -> Result<BuyInstructionInfo> {
    let sig = Signature::from_str(signature)?;
    
    let tx = rpc.get_transaction_with_config(
        &sig,
        solana_client::rpc_config::RpcTransactionConfig {
            encoding: Some(UiTransactionEncoding::Base64),
            max_supported_transaction_version: Some(0),
            commitment: Some(CommitmentConfig::confirmed()),
        }
    ).await?;

    if let solana_transaction_status::EncodedTransaction::Binary(encoded, _) = &tx.transaction.transaction {
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

        eprintln!("   Looking for Pump program: {}", pump_program);
        eprintln!("   Total instructions in transaction: {}", instructions.len());
        
        // Log all programs in transaction
        eprintln!("   Programs in transaction:");
        for (ix_idx, ix) in instructions.iter().enumerate() {
            let program_id_idx = ix.program_id_index as usize;
            if let Some(&program_id) = account_keys.get(program_id_idx) {
                eprintln!("      [{}] Program: {} (data len: {})", ix_idx, program_id, ix.data.len());
            }
        }

        // Find sell instruction
        let mut found_pump_instructions = 0;
        for (ix_idx, ix) in instructions.iter().enumerate() {
            let program_id_idx = ix.program_id_index as usize;
            if let Some(&program_id) = account_keys.get(program_id_idx) {
                if program_id == pump_program {
                    found_pump_instructions += 1;
                    if ix.data.len() >= 8 {
                        let discriminator = &ix.data[0..8];
                        
                        eprintln!("   Found Pump instruction [{}]: discriminator={:02x?}", ix_idx, discriminator);
                        eprintln!("   Expected SELL discriminator: {:02x?}", SELL_DISCRIMINATOR);
                        
                        // Check for sell discriminator - may have multiple variants
                        let is_sell = discriminator == &SELL_DISCRIMINATOR
                            || discriminator == &[0x33, 0xe6, 0x85, 0xa4, 0x01, 0x7f, 0x83, 0xad]; // Alternative sell discriminator
                        eprintln!("   Is sell: {}", is_sell);
                    
                    if is_sell {
                        let num_required_signatures = match &versioned_tx.message {
                            VersionedMessage::Legacy(msg) => msg.header.num_required_signatures as usize,
                            VersionedMessage::V0(msg) => msg.header.num_required_signatures as usize,
                        };
                        
                        let mut accounts = Vec::new();
                        for (idx, &account_idx) in ix.accounts.iter().enumerate() {
                            if let Some(&pubkey) = account_keys.get(account_idx as usize) {
                                let is_signer = (account_idx as usize) < num_required_signatures;
                                let readonly_accounts = vec![
                                    system_program::id(),
                                    Pubkey::from_str("TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb").unwrap_or_default(),
                                    Pubkey::from_str("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA").unwrap_or_default(),
                                    pump_program,
                                ];
                                let is_writable = !readonly_accounts.contains(&pubkey);
                                
                                // Use sell-specific labels
                                let label = match idx {
                                    0 => Some("Global".to_string()),
                                    1 => Some("Fee Recipient".to_string()),
                                    2 => Some("Mint".to_string()),
                                    3 => Some("Bonding Curve".to_string()),
                                    4 => Some("Associated Bonding Curve".to_string()),
                                    5 => Some("Associated User (User Token Account)".to_string()),
                                    6 => Some("User (User Wallet)".to_string()),
                                    7 => Some("System Program".to_string()),
                                    8 => Some("Creator Vault".to_string()),
                                    9 => Some("Token Program 2022".to_string()),
                                    10 => Some("Event Authority".to_string()),
                                    11 => Some("Pump.fun Program".to_string()),
                                    12 => Some("Fee Config".to_string()),
                                    13 => Some("Fee Program".to_string()),
                                    _ => None,
                                };
                                
                                accounts.push(AccountInfo {
                                    index: idx,
                                    pubkey,
                                    is_signer,
                                    is_writable,
                                    label,
                                });
                            }
                        }

                        let data = ix.data.clone();
                        let mut discriminator = [0u8; 8];
                        discriminator.copy_from_slice(&data[0..8]);
                        
                        let token_amount = if data.len() >= 16 {
                            Some(u64::from_le_bytes(data[8..16].try_into().unwrap()))
                        } else {
                            None
                        };
                        
                        let min_sol_out = if data.len() >= 24 {
                            Some(u64::from_le_bytes(data[16..24].try_into().unwrap()))
                        } else {
                            None
                        };

                        println!("🔍 SELL INSTRUCTION FOUND:");
                        println!("   Transaction: {}", signature);
                        println!("   Accounts ({}):", accounts.len());
                        for acc in &accounts {
                            let label = acc.label.as_ref()
                                .map(|l| format!(" ({})", l))
                                .unwrap_or_default();
                            let signer_str = if acc.is_signer { " [SIGNER]" } else { "" };
                            let writable_str = if acc.is_writable { " [WRITABLE]" } else { " [READONLY]" };
                            println!("      [{}] {}{}{}{}", acc.index, acc.pubkey, label, signer_str, writable_str);
                        }
                        
                        if let Some(cv) = accounts.get(8) {
                            println!();
                            println!("🎯 CREATOR VAULT (Account 8): {}", cv.pubkey);
                        }

                        return Ok(BuyInstructionInfo {
                            program_id: pump_program,
                            accounts,
                            data,
                            discriminator,
                            token_amount,
                            max_sol_cost: min_sol_out,
                        });
                    } else {
                        eprintln!("   Not a sell instruction (discriminator mismatch)");
                    }
                    } else {
                        eprintln!("   Pump instruction [{}] has insufficient data (len: {})", ix_idx, ix.data.len());
                    }
                }
            }
        }
        
        eprintln!("   Total Pump instructions found: {}", found_pump_instructions);
        Err(anyhow!("Sell instruction not found in transaction (found {} Pump instructions)", found_pump_instructions))
    } else {
        Err(anyhow!("Transaction encoding not supported"))
    }
}
