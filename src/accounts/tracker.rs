// tracker.rs - TOKEN BUY TRACKING & STATISTICS

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
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TrackerStats {
    pub total_buys: u32,
    pub total_sol_spent: f64,
    pub session_start: DateTime<Utc>,
    pub last_buy: DateTime<Utc>,
    pub buys: Vec<TokenBuy>,
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
            writeln!(file, "Token#,Mint,Signature,Creator,DevBuy(SOL),OurBuy(SOL),Timestamp,HasSocials,Twitter,Website,Telegram,CreatorTokens,DetectionMethod")?;
        }

        Ok(Self {
            stats: TrackerStats {
                total_buys: 0,
                total_sol_spent: 0.0,
                session_start,
                last_buy: session_start,
                buys: Vec::new(),
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

        // Append to CSV
        self.append_to_csv(&buy)?;

        // Add to memory
        self.stats.buys.push(buy);

        // Save JSON
        self.save_json()?;

        Ok(())
    }

    /// Append buy to CSV file
    fn append_to_csv(&self, buy: &TokenBuy) -> Result<()> {
        let mut file = OpenOptions::new()
            .append(true)
            .open(&self.csv_path)?;

        writeln!(
            file,
            "{},{},{},{},{:.4},{:.4},{},{},{},{},{},{},{}",
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
        )?;

        Ok(())
    }

    /// Save full stats to JSON
    fn save_json(&self) -> Result<()> {
        let file = File::create(&self.json_path)?;
        let writer = BufWriter::new(file);
        serde_json::to_writer_pretty(writer, &self.stats)?;
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
        println!("║                                                      ║");
        println!("║ CSV saved:  {:43} ║", &self.csv_path);
        println!("║ JSON saved: {:43} ║", &self.json_path);
        println!("╚══════════════════════════════════════════════════════╝\n");
    }

    /// Print periodic stats (every N buys)
    pub fn print_periodic_stats(&self) {
        if self.stats.total_buys % 10 == 0 {
            println!("\n📊 STATS: {} buys | {:.4} SOL spent | Avg: {:.4} SOL/buy\n",
                     self.stats.total_buys,
                     self.stats.total_sol_spent,
                     self.stats.total_sol_spent / self.stats.total_buys as f64);
        }
    }

    /// Get current stats
    pub fn get_stats(&self) -> &TrackerStats {
        &self.stats
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
    fn test_buy_recording() {
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
        };

        assert!(tracker.record_buy(buy).is_ok());
        assert_eq!(tracker.total_buys(), 1);
        assert_eq!(tracker.get_stats().total_sol_spent, 0.1);
    }
}