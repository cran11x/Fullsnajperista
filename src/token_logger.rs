// token_logger.rs - JSON logging for all detected tokens
use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::Path;
use std::sync::Mutex;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenLogEntry {
    pub timestamp: DateTime<Utc>,
    pub mint: String,
    pub init_signature: Option<String>,
    pub status: TokenStatus,
    pub reason: String,
    pub dev_buy_sol: Option<f64>,
    pub creator: Option<String>,
    pub creator_token_count: Option<u32>,
    pub socials: Option<SocialsInfo>,
    pub buy_signature: Option<String>,
    pub market_cap_usd: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TokenStatus {
    Detected,
    Filtered,
    Bought,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SocialsInfo {
    pub twitter: Option<String>,
    pub telegram: Option<String>,
    pub website: Option<String>,
    pub discord: Option<String>,
    pub twitter_type: Option<String>, // "account", "community", "status", or None
    pub count: usize,
}

pub struct TokenLogger {
    #[allow(dead_code)]
    file_path: String,
    writer: Mutex<BufWriter<File>>,
}

impl TokenLogger {
    pub fn new(file_path: impl AsRef<Path>) -> Result<Self> {
        let path = file_path.as_ref();
        
        // Create parent directory if it doesn't exist
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        
        // Open file in append mode, create if doesn't exist
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)?;
        
        let writer = BufWriter::new(file);
        
        Ok(Self {
            file_path: path.to_string_lossy().to_string(),
            writer: Mutex::new(writer),
        })
    }
    
    pub fn log_token(&self, entry: TokenLogEntry) -> Result<()> {
        let mut writer = self.writer.lock().unwrap();
        
        // Write JSON entry as a single line (JSONL format for easy appending)
        serde_json::to_writer(&mut *writer, &entry)?;
        writer.write_all(b"\n")?;
        writer.flush()?;
        
        Ok(())
    }
    
    pub fn log_detected(
        &self,
        mint: String,
        init_signature: Option<String>,
        dev_buy_sol: Option<f64>,
        creator: Option<String>,
    ) -> Result<()> {
        let entry = TokenLogEntry {
            timestamp: Utc::now(),
            mint,
            init_signature,
            status: TokenStatus::Detected,
            reason: "Token detected".to_string(),
            dev_buy_sol,
            creator,
            creator_token_count: None,
            socials: None,
            buy_signature: None,
            market_cap_usd: None,
        };
        
        self.log_token(entry)
    }
    
    pub fn log_filtered(
        &self,
        mint: String,
        reason: String,
        init_signature: Option<String>,
        dev_buy_sol: Option<f64>,
        creator: Option<String>,
        creator_token_count: Option<u32>,
        socials: Option<SocialsInfo>,
    ) -> Result<()> {
        let entry = TokenLogEntry {
            timestamp: Utc::now(),
            mint,
            init_signature,
            status: TokenStatus::Filtered,
            reason,
            dev_buy_sol,
            creator,
            creator_token_count,
            socials,
            buy_signature: None,
            market_cap_usd: None,
        };
        
        self.log_token(entry)
    }
    
    pub fn log_bought(
        &self,
        mint: String,
        buy_signature: String,
        init_signature: Option<String>,
        market_cap_usd: Option<f64>,
        dev_buy_sol: Option<f64>,
        creator: Option<String>,
        socials: Option<SocialsInfo>,
    ) -> Result<()> {
        let entry = TokenLogEntry {
            timestamp: Utc::now(),
            mint,
            init_signature,
            status: TokenStatus::Bought,
            reason: "Token bought successfully".to_string(),
            dev_buy_sol,
            creator,
            creator_token_count: None,
            socials,
            buy_signature: Some(buy_signature),
            market_cap_usd,
        };
        
        self.log_token(entry)
    }
}

// Helper function to create logger with timestamped filename
pub fn create_logger() -> Result<TokenLogger> {
    let timestamp = Utc::now().format("%Y%m%d_%H%M%S");
    let filename = format!("token_log_{}.jsonl", timestamp);
    eprintln!("📝 Creating token logger: {}", filename);
    TokenLogger::new(&filename)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    
    #[test]
    fn test_logger_creation() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.jsonl");
        let logger = TokenLogger::new(&file_path).unwrap();
        
        logger.log_detected(
            "TestMint123".to_string(),
            Some("TestSig123".to_string()),
            Some(0.5),
            Some("Creator123".to_string()),
        ).unwrap();
        
        // Verify file was created and has content
        assert!(file_path.exists());
    }
}

