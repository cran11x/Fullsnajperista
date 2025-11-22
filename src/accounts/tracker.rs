// tracker.rs - TOKEN BUY TRACKING & STATISTICS (WITH MC TRACKING)
#![allow(dead_code)]

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::Path;
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenBuy {
    pub token_number: u32,
    pub mint: String,
    pub signature: String,
    pub creator: String,
    pub dev_buy_sol: f64,
    pub our_buy_sol: f64,
    pub timestamp: DateTime<Utc>,
    pub has_socials: bool,
    pub twitter: Option<String>,
    pub website: Option<String>,
    pub telegram: Option<String>,
    pub creator_token_count: u32,
    pub detection_method: String, // "instruction" or "balance_fallback"

    // 🆕 NEW: Market cap tracking - PRE and POST buy
    pub mc_at_detection_usd: Option<f64>,  // MC when first detected
    pub mc_at_entry_usd: Option<f64>,      // MC after TX confirmed (real entry)
    pub token_price_sol: Option<f64>,      // Token price in SOL at detection
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TrackerStats {
    pub total_buys: u32,
    pub total_sol_spent: f64,
    pub session_start: DateTime<Utc>,
    pub last_buy: DateTime<Utc>,
    pub buys: Vec<TokenBuy>,

    // 🆕 NEW: MC statistics
    pub avg_mc_usd: f64,
    pub min_mc_usd: f64,
    pub max_mc_usd: f64,
}

pub struct TokenTracker {
    stats: TrackerStats,
    csv_path: String,
    json_path: String,
}

impl TokenTracker {
    /// Create new tracker
    pub fn new() -> Result<Self> {
        let session_start = Utc::now();
        let timestamp = session_start.format("%Y%m%d_%H%M%S");

        let csv_path = format!("sniper_session_{}.csv", timestamp);
        let json_path = format!("sniper_session_{}.json", timestamp);

        // Create CSV header if file doesn't exist
        if !Path::new(&csv_path).exists() {
            let mut file = File::create(&csv_path)?;
            writeln!(file, "Token#,Mint,Signature,Creator,DevBuy(SOL),OurBuy(SOL),Timestamp,HasSocials,Twitter,Website,Telegram,CreatorTokens,DetectionMethod,MC_Detection_USD,MC_Entry_USD,TokenPrice_SOL")?;
        }

        Ok(Self {
            stats: TrackerStats {
                total_buys: 0,
                total_sol_spent: 0.0,
                session_start,
                last_buy: session_start,
                buys: Vec::new(),
                avg_mc_usd: 0.0,
                min_mc_usd: f64::MAX,
                max_mc_usd: 0.0,
            },
            csv_path,
            json_path,
        })
    }

    /// Record a token buy
    pub fn record_buy(&mut self, buy: TokenBuy) -> Result<()> {
        self.stats.total_buys += 1;
        self.stats.total_sol_spent += buy.our_buy_sol;
        self.stats.last_buy = buy.timestamp;

        // Update MC stats (using entry MC as that's the real execution price)
        if let Some(mc_usd) = buy.mc_at_entry_usd {
            self.stats.min_mc_usd = self.stats.min_mc_usd.min(mc_usd);
            self.stats.max_mc_usd = self.stats.max_mc_usd.max(mc_usd);

            // Recalculate average
            let total_mc: f64 = self.stats.buys.iter()
                .filter_map(|b| b.mc_at_entry_usd)
                .sum::<f64>() + mc_usd;
            let count_with_mc = self.stats.buys.iter()
                .filter(|b| b.mc_at_entry_usd.is_some())
                .count() + 1;
            self.stats.avg_mc_usd = total_mc / count_with_mc as f64;
        }

        // Append to CSV
        self.append_to_csv(&buy)?;

        // Add to memory
        self.stats.buys.push(buy);

        // Save JSON
        self.save_json()?;

        Ok(())
    }

    /// Append buy to CSV file with better error handling
    fn append_to_csv(&self, buy: &TokenBuy) -> Result<()> {
        // Use BufWriter for better performance
        let mut file = OpenOptions::new()
            .append(true)
            .create(true) // Create file if it doesn't exist
            .open(&self.csv_path)
            .map_err(|e| anyhow::anyhow!("Failed to open CSV file {}: {}", self.csv_path, e))?;

        let mut writer = BufWriter::new(&mut file);

        writeln!(
            writer,
            "{},{},{},{},{:.4},{:.4},{},{},{},{},{},{},{},{},{},{}",
            buy.token_number,
            buy.mint,
            buy.signature,
            buy.creator,
            buy.dev_buy_sol,
            buy.our_buy_sol,
            buy.timestamp.to_rfc3339(),
            buy.has_socials,
            buy.twitter.as_deref().unwrap_or(""),
            buy.website.as_deref().unwrap_or(""),
            buy.telegram.as_deref().unwrap_or(""),
            buy.creator_token_count,
            buy.detection_method,
            buy.mc_at_detection_usd.map(|v| format!("{:.0}", v)).unwrap_or_default(),
            buy.mc_at_entry_usd.map(|v| format!("{:.0}", v)).unwrap_or_default(),
            buy.token_price_sol.map(|v| format!("{:.8}", v)).unwrap_or_default(),
        )
        .map_err(|e| anyhow::anyhow!("Failed to write to CSV: {}", e))?;

        writer.flush()
            .map_err(|e| anyhow::anyhow!("Failed to flush CSV buffer: {}", e))?;

        Ok(())
    }

    /// Save full stats to JSON with better error handling
    fn save_json(&self) -> Result<()> {
        let file = File::create(&self.json_path)
            .map_err(|e| anyhow::anyhow!("Failed to create JSON file {}: {}", self.json_path, e))?;
        let writer = BufWriter::new(file);
        serde_json::to_writer_pretty(writer, &self.stats)
            .map_err(|e| anyhow::anyhow!("Failed to serialize JSON: {}", e))?;
        Ok(())
    }

    /// Print session summary
    pub fn print_summary(&self) {
        let duration = Utc::now() - self.stats.session_start;
        let hours = duration.num_hours();
        let minutes = duration.num_minutes() % 60;

        println!("\n╔══════════════════════════════════════════════════════╗");
        println!("║           SNIPER SESSION SUMMARY                    ║");
        println!("╠══════════════════════════════════════════════════════╣");
        println!("║ Total Buys:        {:>6}                          ║", self.stats.total_buys);
        println!("║ Total SOL Spent:   {:>9.4} SOL                   ║", self.stats.total_sol_spent);
        println!("║ Session Duration:  {}h {}m                         ║", hours, minutes);
        println!("║ Average per buy:   {:>9.4} SOL                   ║",
                 if self.stats.total_buys > 0 {
                     self.stats.total_sol_spent / self.stats.total_buys as f64
                 } else {
                     0.0
                 });

        // MC statistics
        if self.stats.max_mc_usd > 0.0 {
            println!("║                                                      ║");
            println!("║ 📊 MARKET CAP STATISTICS:                           ║");
            println!("║   Average MC:      ${:>10.0}                      ║", self.stats.avg_mc_usd);
            println!("║   Min MC:          ${:>10.0}                      ║", self.stats.min_mc_usd);
            println!("║   Max MC:          ${:>10.0}                      ║", self.stats.max_mc_usd);
        }

        println!("║                                                      ║");
        println!("║ CSV saved:  {:43} ║", &self.csv_path);
        println!("║ JSON saved: {:43} ║", &self.json_path);
        println!("╚══════════════════════════════════════════════════════╝\n");
    }

    /// Print periodic stats (every N buys)
    pub fn print_periodic_stats(&self) {
        if self.stats.total_buys % 10 == 0 {
            let mut msg = format!("\n📊 STATS: {} buys | {:.4} SOL spent | Avg: {:.4} SOL/buy",
                                  self.stats.total_buys,
                                  self.stats.total_sol_spent,
                                  self.stats.total_sol_spent / self.stats.total_buys as f64);

            if self.stats.avg_mc_usd > 0.0 {
                msg.push_str(&format!(" | Avg MC: ${:.0}", self.stats.avg_mc_usd));
            }

            println!("{}\n", msg);
        }
    }

    /// Get current stats
    pub fn get_stats(&self) -> &TrackerStats {
        &self.stats
    }
    
    /// Get recent buys for display (last N buys)
    pub fn get_recent_buys(&self, limit: usize) -> Vec<TokenBuy> {
        self.stats.buys.iter()
            .rev()
            .take(limit)
            .cloned()
            .collect()
    }

    /// Get total buys
    pub fn total_buys(&self) -> u32 {
        self.stats.total_buys
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tracker_creation() {
        let tracker = TokenTracker::new();
        assert!(tracker.is_ok());
    }

    #[test]
    fn test_buy_recording_with_mc() {
        let mut tracker = TokenTracker::new().unwrap();

        let buy = TokenBuy {
            token_number: 1,
            mint: "test_mint".to_string(),
            signature: "test_sig".to_string(),
            creator: "test_creator".to_string(),
            dev_buy_sol: 2.0,
            our_buy_sol: 0.1,
            timestamp: Utc::now(),
            has_socials: true,
            twitter: Some("@test".to_string()),
            website: None,
            telegram: None,
            creator_token_count: 1,
            detection_method: "instruction".to_string(),
            mc_at_detection_usd: Some(4800.0),
            mc_at_entry_usd: Some(5000.0),
            token_price_sol: Some(0.00005),
        };

        assert!(tracker.record_buy(buy).is_ok());
        assert_eq!(tracker.total_buys(), 1);
        assert_eq!(tracker.get_stats().avg_mc_usd, 5000.0);
    }

    #[test]
    fn test_csv_append() {
        use tempfile::NamedTempFile;
        
        let temp_file = NamedTempFile::new().unwrap();
        let csv_path = temp_file.path().to_str().unwrap().to_string();
        let json_path = format!("{}.json", csv_path);

        // Create tracker with temp file
        let tracker = TokenTracker {
            stats: TrackerStats {
                total_buys: 0,
                total_sol_spent: 0.0,
                session_start: Utc::now(),
                last_buy: Utc::now(),
                buys: Vec::new(),
                avg_mc_usd: 0.0,
                min_mc_usd: f64::MAX,
                max_mc_usd: 0.0,
            },
            csv_path: csv_path.clone(),
            json_path: json_path.clone(),
        };

        // Create CSV header
        std::fs::write(&csv_path, "Token#,Mint,Signature,Creator,DevBuy(SOL),OurBuy(SOL),Timestamp,HasSocials,Twitter,Website,Telegram,CreatorTokens,DetectionMethod,MC_Detection_USD,MC_Entry_USD,TokenPrice_SOL\n").unwrap();

        let buy = TokenBuy {
            token_number: 1,
            mint: "test_mint".to_string(),
            signature: "test_sig".to_string(),
            creator: "test_creator".to_string(),
            dev_buy_sol: 2.0,
            our_buy_sol: 0.1,
            timestamp: Utc::now(),
            has_socials: true,
            twitter: Some("@test".to_string()),
            website: None,
            telegram: None,
            creator_token_count: 1,
            detection_method: "instruction".to_string(),
            mc_at_detection_usd: Some(4800.0),
            mc_at_entry_usd: Some(5000.0),
            token_price_sol: Some(0.00005),
        };

        assert!(tracker.append_to_csv(&buy).is_ok());

        // Verify CSV content
        let content = std::fs::read_to_string(&csv_path).unwrap();
        assert!(content.contains("test_mint"));
        assert!(content.contains("test_sig"));
        assert!(content.contains("5000")); // MC entry
    }

    #[test]
    fn test_json_save() {
        use tempfile::NamedTempFile;
        
        let temp_file = NamedTempFile::new().unwrap();
        let json_path = temp_file.path().to_str().unwrap().to_string();
        let csv_path = format!("{}.csv", json_path);

        let tracker = TokenTracker {
            stats: TrackerStats {
                total_buys: 2,
                total_sol_spent: 0.2,
                session_start: Utc::now(),
                last_buy: Utc::now(),
                buys: Vec::new(),
                avg_mc_usd: 5000.0,
                min_mc_usd: 4800.0,
                max_mc_usd: 5200.0,
            },
            csv_path,
            json_path: json_path.clone(),
        };

        assert!(tracker.save_json().is_ok());

        // Verify JSON content
        let content = std::fs::read_to_string(&json_path).unwrap();
        assert!(content.contains("\"total_buys\":2") || content.contains("\"total_buys\": 2"));
        assert!(content.contains("\"total_sol_spent\":0.2") || content.contains("\"total_sol_spent\": 0.2"));
        assert!(content.contains("\"avg_mc_usd\":5000.0") || content.contains("\"avg_mc_usd\": 5000.0"));
    }

    #[test]
    fn test_mc_statistics() {
        let mut tracker = TokenTracker::new().unwrap();

        // Add buys with different MCs
        let buy1 = TokenBuy {
            token_number: 1,
            mint: "mint1".to_string(),
            signature: "sig1".to_string(),
            creator: "creator1".to_string(),
            dev_buy_sol: 2.0,
            our_buy_sol: 0.1,
            timestamp: Utc::now(),
            has_socials: false,
            twitter: None,
            website: None,
            telegram: None,
            creator_token_count: 1,
            detection_method: "instruction".to_string(),
            mc_at_detection_usd: Some(4000.0),
            mc_at_entry_usd: Some(4500.0),
            token_price_sol: Some(0.00004),
        };

        let buy2 = TokenBuy {
            token_number: 2,
            mint: "mint2".to_string(),
            signature: "sig2".to_string(),
            creator: "creator2".to_string(),
            dev_buy_sol: 2.0,
            our_buy_sol: 0.1,
            timestamp: Utc::now(),
            has_socials: false,
            twitter: None,
            website: None,
            telegram: None,
            creator_token_count: 1,
            detection_method: "instruction".to_string(),
            mc_at_detection_usd: Some(6000.0),
            mc_at_entry_usd: Some(6500.0),
            token_price_sol: Some(0.00006),
        };

        tracker.record_buy(buy1).unwrap();
        tracker.record_buy(buy2).unwrap();

        let stats = tracker.get_stats();
        assert_eq!(stats.avg_mc_usd, 5500.0); // (4500 + 6500) / 2
        assert_eq!(stats.min_mc_usd, 4500.0);
        assert_eq!(stats.max_mc_usd, 6500.0);
    }

    #[test]
    fn test_record_buy_without_mc() {
        let mut tracker = TokenTracker::new().unwrap();

        let buy = TokenBuy {
            token_number: 1,
            mint: "test_mint".to_string(),
            signature: "test_sig".to_string(),
            creator: "test_creator".to_string(),
            dev_buy_sol: 2.0,
            our_buy_sol: 0.1,
            timestamp: Utc::now(),
            has_socials: false,
            twitter: None,
            website: None,
            telegram: None,
            creator_token_count: 1,
            detection_method: "instruction".to_string(),
            mc_at_detection_usd: None,
            mc_at_entry_usd: None,
            token_price_sol: None,
        };

        assert!(tracker.record_buy(buy).is_ok());
        assert_eq!(tracker.total_buys(), 1);
        // MC stats should remain at defaults
        assert_eq!(tracker.get_stats().avg_mc_usd, 0.0);
    }

    #[test]
    #[ignore]
    fn test_tracker_file_operations() {
        // Integration test - creates real files
        let mut tracker = TokenTracker::new().unwrap();

        let buy = TokenBuy {
            token_number: 1,
            mint: "integration_test_mint".to_string(),
            signature: "integration_test_sig".to_string(),
            creator: "integration_test_creator".to_string(),
            dev_buy_sol: 2.0,
            our_buy_sol: 0.1,
            timestamp: Utc::now(),
            has_socials: true,
            twitter: Some("@integration_test".to_string()),
            website: Some("https://example.com".to_string()),
            telegram: None,
            creator_token_count: 1,
            detection_method: "instruction".to_string(),
            mc_at_detection_usd: Some(4800.0),
            mc_at_entry_usd: Some(5000.0),
            token_price_sol: Some(0.00005),
        };

        assert!(tracker.record_buy(buy).is_ok());

        // Verify files exist
        assert!(std::path::Path::new(&tracker.csv_path).exists());
        assert!(std::path::Path::new(&tracker.json_path).exists());

        // Cleanup
        let _ = std::fs::remove_file(&tracker.csv_path);
        let _ = std::fs::remove_file(&tracker.json_path);
    }
}