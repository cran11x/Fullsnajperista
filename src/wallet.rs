// wallet.rs - Wallet loading utilities
#![allow(unused)]

use anyhow::{anyhow, Result};
use solana_sdk::{bs58, signature::{Keypair, Signer}};

pub fn load_wallet() -> Result<Keypair> {
    // Try multiple locations for .env file
    let current_dir = std::env::current_dir()
        .map_err(|e| anyhow!("Failed to get current directory: {}", e))?;
    
    // Try to manually parse .env file to handle BOM and parsing issues
    let env_path = current_dir.join(".env");
    if env_path.exists() {
        // Try manual parsing as fallback
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
                    std::env::set_var(key, value);
                }
            }
        }
        
        // Also try dotenv as it might handle some edge cases better
        let _ = dotenv::from_path(&env_path);
    }
    
    // Also try loading from current directory (standard behavior)
    // This searches up the directory tree
    let _ = dotenv::dotenv();
    
    // Also try parent directory (in case running from subdirectory)
    if let Some(parent) = current_dir.parent() {
        let parent_env = parent.join(".env");
        if parent_env.exists() {
            // Try manual parsing for parent too
            if let Ok(contents) = std::fs::read_to_string(&parent_env) {
                for line in contents.lines() {
                    let line = line.trim();
                    if line.is_empty() || line.starts_with('#') {
                        continue;
                    }
                    if let Some((key, value)) = line.split_once('=') {
                        let key = key.trim();
                        let value = value.trim().trim_matches('"').trim_matches('\'');
                        std::env::set_var(key, value);
                    }
                }
            }
            let _ = dotenv::from_path(&parent_env);
        }
    }

    // Try to get the key from environment
    match std::env::var("SOLANA_PRIVATE_KEY") {
        Ok(private_key_base58) => {
            let trimmed = private_key_base58.trim();
            if trimmed.is_empty() {
                return Err(anyhow!("SOLANA_PRIVATE_KEY is empty in .env file"));
            }
            
            let bytes = bs58::decode(trimmed)
                .into_vec()
                .map_err(|e| anyhow!("Invalid Base58 encoding: {}", e))?;

            let keypair = Keypair::from_bytes(&bytes)
                .map_err(|e| anyhow!("Invalid keypair bytes: {}", e))?;

            Ok(keypair)
        }
        Err(std::env::VarError::NotPresent) => {
            Err(anyhow!("SOLANA_PRIVATE_KEY not found in environment. Check .env file exists and is readable."))
        }
        Err(e) => {
            Err(anyhow!("Error reading SOLANA_PRIVATE_KEY: {}", e))
        }
    }
}

pub fn try_load_wallet_address() -> Result<String> {
    // Try multiple locations for .env file
    let current_dir = std::env::current_dir()
        .map_err(|e| anyhow!("Failed to get current directory: {}", e))?;
    
    // Try current directory first
    let env_path = current_dir.join(".env");
    if env_path.exists() {
        let _ = dotenv::from_path(&env_path);
    }
    
    // Also try loading from current directory (standard behavior)
    let _ = dotenv::dotenv();
    
    // Also try parent directory (in case running from subdirectory)
    if let Some(parent) = current_dir.parent() {
        let parent_env = parent.join(".env");
        if parent_env.exists() {
            let _ = dotenv::from_path(&parent_env);
        }
    }
    
    if let Ok(private_key_base58) = std::env::var("SOLANA_PRIVATE_KEY") {
        let trimmed = private_key_base58.trim();
        if trimmed.is_empty() {
            return Err(anyhow!("SOLANA_PRIVATE_KEY is empty"));
        }
        
        let bytes = bs58::decode(trimmed)
            .into_vec()
            .map_err(|e| anyhow!("Invalid Base58: {}", e))?;

        let keypair = Keypair::from_bytes(&bytes)
            .map_err(|e| anyhow!("Invalid keypair: {}", e))?;

        return Ok(keypair.pubkey().to_string());
    }

    Err(anyhow!("SOLANA_PRIVATE_KEY not set"))
}

