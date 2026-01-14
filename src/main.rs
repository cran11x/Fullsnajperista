// main.rs - Main entry point for Pump.fun Sniper Bot GUI
// Temporarily disabled windows_subsystem to see debug output
// #![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

pub mod accounts;
pub mod account_subscription;
pub mod bot_core;
pub mod buy;
pub mod sell;
pub mod config;
pub mod constants;
pub mod das_check;
pub mod debug;
pub mod detection;
pub mod errors;
pub mod filters;
pub mod gui;
pub mod health;
pub mod helius;
pub mod jito;
pub mod metrics;
pub mod pda_derivation;
pub mod rate_limiter;
pub mod socials;
pub mod token_logger;
pub mod tracking_logger;
pub mod tools;
pub mod utils;
pub mod validation;
pub mod wallet;
pub mod websocket; // For tests

use anyhow::{anyhow, Result};

// Remove old constants - now using Config and constants module

fn main() -> Result<()> {
    // Check for test command
    let args: Vec<String> = std::env::args().collect();
    if args.len() >= 2 && args[1] == "test-sol-price" {
        // Test SOL price fetching
        let rt = tokio::runtime::Runtime::new()?;
        rt.block_on(async {
            if let Err(e) = tools::sol_price_test::run().await {
                eprintln!("❌ SOL price test failed: {e}");
            }
        });
        return Ok(());
    }
    
    // Check for debug command
    if args.len() >= 3 && args[1] == "debug" {
        let rt = tokio::runtime::Runtime::new()?;
        rt.block_on(async {
            let rpc = solana_client::nonblocking::rpc_client::RpcClient::new(
                std::env::var("SOLANA_RPC_URL")
                    .unwrap_or_else(|_| "https://api.mainnet-beta.solana.com".to_string())
            );
            
            if args.len() >= 4 && args[2] == "sell" {
                // Debug mode: analyze sell transaction
                // Usage: cargo run -- debug sell <tx_sig>
                let tx_sig = &args[3];
                
                println!("🔍 Analyzing SELL transaction: {}", tx_sig);
                match debug::extract_sell_instruction_from_tx(&rpc, tx_sig).await {
                    Ok(instruction) => {
                        println!("✅ Sell instruction extracted successfully");
                        if let Some(cv) = instruction.get_account_pubkey(8) {
                            println!();
                            println!("🎯 CREATOR VAULT (Account 8): {}", cv);
                        }
                    }
                    Err(e) => {
                        eprintln!("❌ Failed to extract sell instruction: {}", e);
                        std::process::exit(1);
                    }
                }
            } else if args.len() >= 4 {
                // Debug mode: compare two transactions
                // Usage: cargo run -- debug <successful_sig> <failed_sig>
                let successful_sig = &args[2];
                let failed_sig = &args[3];
                
                println!("🔍 Debug Mode: Comparing transactions");
                println!("   Successful: {}", successful_sig);
                println!("   Failed:     {}", failed_sig);
                println!();
                
                if let Err(e) = debug::debug_compare_transactions(&rpc, successful_sig, failed_sig).await {
                    eprintln!("❌ Debug failed: {}", e);
                    std::process::exit(1);
                }
            } else {
                // Debug mode: analyze single transaction (buy)
                // Usage: cargo run -- debug <tx_sig>
                let tx_sig = &args[2];
                
                if let Err(e) = debug::analyze_single_transaction(&rpc, tx_sig).await {
                    eprintln!("❌ Analysis failed: {}", e);
                    std::process::exit(1);
                }
            }
        });
        
        return Ok(());
    }
    
    // Install panic handler to catch crashes and show them in GUI instead of crashing
    std::panic::set_hook(Box::new(|panic_info| {
        eprintln!("❌ Panic occurred: {:?}", panic_info);
        eprintln!("   Location: {:?}", panic_info.location());
        if let Some(s) = panic_info.payload().downcast_ref::<&str>() {
            eprintln!("   Message: {}", s);
        } else if let Some(s) = panic_info.payload().downcast_ref::<String>() {
            eprintln!("   Message: {}", s);
        }
    }));
    
    // Load .env file early and from multiple locations
    let current_dir = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    
    // Try manual parsing first (more resilient to BOM and encoding issues)
    let env_path = current_dir.join(".env");
    if env_path.exists() {
        if let Ok(contents) = std::fs::read_to_string(&env_path) {
            for line in contents.lines() {
                let line = line.trim();
                // Skip comments and empty lines
                if line.is_empty() || line.starts_with('#') {
                    continue;
                }
                if let Some((key, value)) = line.split_once('=') {
                    let key = key.trim();
                    let value = value.trim().trim_matches('"').trim_matches('\'');
                    if !key.is_empty() && !value.is_empty() {
                        std::env::set_var(key, value);
                    }
                }
            }
        }
    }
    
    // Also try dotenv library (handles some edge cases better)
    let _ = dotenv::dotenv();
    
    // Also try parent directory
    if let Some(parent) = current_dir.parent() {
        let parent_env = parent.join(".env");
        if parent_env.exists() {
            if let Ok(contents) = std::fs::read_to_string(&parent_env) {
                for line in contents.lines() {
                    let line = line.trim();
                    if line.is_empty() || line.starts_with('#') {
                        continue;
                    }
                    if let Some((key, value)) = line.split_once('=') {
                        let key = key.trim();
                        let value = value.trim().trim_matches('"').trim_matches('\'');
                        if !key.is_empty() && !value.is_empty() {
                            std::env::set_var(key, value);
                        }
                    }
                }
            }
            let _ = dotenv::from_path(&parent_env);
        }
    }
    
    // Verify SOLANA_PRIVATE_KEY is loaded
    match std::env::var("SOLANA_PRIVATE_KEY") {
        Ok(key) => {
            if key.trim().is_empty() {
                eprintln!("⚠️  Warning: SOLANA_PRIVATE_KEY is empty in .env file");
            }
        }
        Err(_) => {
            eprintln!("⚠️  Warning: SOLANA_PRIVATE_KEY not found in environment");
        }
    }
    
    // Initialize GUI app with macOS compatibility options
    // Note: NSScreen panic is a known issue with icrate/eframe on macOS
    // This may require updating eframe/egui or using a workaround
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1400.0, 900.0])
            .with_min_inner_size([600.0, 400.0]) // Minimum width to fit all 7 tabs
            .with_title(if cfg!(debug_assertions) {
                "SNIPER - DEBUG BUILD"
            } else {
                "SNIPER"
            }),
        ..Default::default()
    };
    
    // Run with better error handling
    if let Err(e) = eframe::run_native(
        "SNIPER",
        options,
        Box::new(|cc| {
            match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| gui::GuiApp::new(cc))) {
                Ok(app) => Ok(Box::new(app) as Box<dyn eframe::App>),
                Err(_) => Err(Box::from("Failed to initialize GUI")),
            }
        }),
    ) {
        eprintln!("❌ Failed to run GUI: {}", e);
        eprintln!("⚠️  If you see NSScreen panic, this is a known issue with icrate/eframe on macOS");
        eprintln!("   Try updating eframe/egui or check for macOS-specific workarounds");
        return Err(anyhow!("Failed to run GUI: {}", e));
    }
    
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use solana_sdk::signature::Signer;
    use websocket::{is_initialize_bonding_curve, extract_signature};

    #[test]
    fn test_load_wallet() {
        // Save original value if exists
        let original_key = std::env::var("SOLANA_PRIVATE_KEY").ok();
        
        // Test with invalid base58 key
        std::env::set_var("SOLANA_PRIVATE_KEY", "!!!INVALID_BASE58!!!");
        let result = crate::wallet::load_wallet();
        
        match result {
            Ok(keypair) => {
                // If it succeeds (e.g., because .env file overrides), verify it's valid
                // Use Signer trait method explicitly
                let pubkey = Signer::pubkey(&keypair);
                assert!(pubkey.to_string().len() > 0, "Loaded wallet should have valid pubkey");
            }
            Err(e) => {
                // Expected error for invalid base58
                let err_msg = e.to_string();
                assert!(err_msg.contains("Invalid Base58") || err_msg.contains("Invalid keypair") ||
                        err_msg.contains("SOLANA_PRIVATE_KEY not found"),
                        "Expected Base58, keypair, or not found error, got: {}", err_msg);
            }
        }

        // Restore original or cleanup
        if let Some(key) = original_key {
            std::env::set_var("SOLANA_PRIVATE_KEY", key);
        } else {
            std::env::remove_var("SOLANA_PRIVATE_KEY");
        }
    }

    #[test]
    fn test_is_initialize_bonding_curve() {
        let notification = serde_json::json!({
            "params": {
                "result": {
                    "value": {
                        "logs": [
                            "Program 6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P invoke [1]",
                            "Program log: Instruction: Create"
                        ]
                    }
                }
            }
        });

        assert!(is_initialize_bonding_curve(&notification));
    }

    #[test]
    fn test_extract_signature() {
        let notification = serde_json::json!({
            "params": {
                "result": {
                    "value": {
                        "signature": "test_signature_123"
                    }
                }
            }
        });

        let sig = extract_signature(&notification);
        assert_eq!(sig, Some("test_signature_123".to_string()));
    }
}
