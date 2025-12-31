// history_tracker.rs - ULTRA MC/PRICE HISTORY TRACKING FOR CHARTS
//
// Records time-series snapshots of MC, price, and PnL for each active position.
// Enables historical analysis and chart generation.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Write};
use chrono::{DateTime, Utc};
use std::time::{Duration, Instant};

/// Single snapshot of token state at a point in time
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriceSnapshot {
    pub timestamp: DateTime<Utc>,
    pub mc_sol: f64,
    pub price_sol: f64,
    pub pnl_percent: Option<f64>,
    pub pnl_sol: Option<f64>,
    pub current_value_sol: Option<f64>,
    /// Virtual token reserves from bonding curve
    pub virtual_token_reserves: Option<u64>,
    /// Virtual SOL reserves from bonding curve  
    pub virtual_sol_reserves: Option<u64>,
    /// Real token reserves (tokens in bonding curve)
    pub real_token_reserves: Option<u64>,
    /// Real SOL reserves (SOL in bonding curve)
    pub real_sol_reserves: Option<u64>,
}

/// Complete history for a single token
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenHistory {
    pub mint: String,
    pub bonding_curve: String,
    pub entry_mc_sol: f64,
    pub entry_price_sol: f64,
    pub entry_timestamp: DateTime<Utc>,
    pub our_buy_sol: f64,
    pub token_amount: Option<u64>,
    /// All recorded snapshots (time-series data)
    pub snapshots: Vec<PriceSnapshot>,
    /// Peak MC reached (in SOL)
    pub peak_mc_sol: f64,
    /// Peak PnL percentage reached
    pub peak_pnl_percent: f64,
    /// Lowest MC reached (for drawdown analysis, in SOL)
    pub lowest_mc_sol: f64,
    /// Is position still active?
    pub is_active: bool,
    /// Sell timestamp if sold
    pub sell_timestamp: Option<DateTime<Utc>>,
    /// Final PnL if sold
    pub final_pnl_percent: Option<f64>,
}

/// Session-level history tracking data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryData {
    pub session_start: DateTime<Utc>,
    pub last_update: DateTime<Utc>,
    pub total_snapshots: u64,
    pub tokens: HashMap<String, TokenHistory>,
}

/// Main history tracker - manages recording and persistence
pub struct HistoryTracker {
    data: HistoryData,
    file_path: String,
    /// Minimum interval between snapshots per token (to avoid huge files)
    snapshot_interval: Duration,
    /// Last snapshot time per token
    last_snapshot: HashMap<String, Instant>,
    /// Auto-save interval
    save_interval: Duration,
    /// Last save time
    last_save: Instant,
    /// Dirty flag - needs save
    dirty: bool,
}

impl HistoryTracker {
    /// Create new history tracker
    pub fn new() -> Result<Self> {
        let session_start = Utc::now();
        let timestamp = session_start.format("%Y%m%d_%H%M%S");
        let file_path = format!("mc_history_{}.json", timestamp);
        
        Ok(Self {
            data: HistoryData {
                session_start,
                last_update: session_start,
                total_snapshots: 0,
                tokens: HashMap::new(),
            },
            file_path,
            snapshot_interval: Duration::from_millis(1000), // 1 snapshot per second per token
            last_snapshot: HashMap::new(),
            save_interval: Duration::from_secs(10), // Auto-save every 10 seconds
            last_save: Instant::now(),
            dirty: false,
        })
    }
    
    /// Create tracker with custom snapshot interval
    pub fn with_interval(snapshot_interval_ms: u64) -> Result<Self> {
        let mut tracker = Self::new()?;
        tracker.snapshot_interval = Duration::from_millis(snapshot_interval_ms);
        Ok(tracker)
    }
    
    /// Register a new token position for tracking
    pub fn register_token(
        &mut self,
        mint: &str,
        bonding_curve: &str,
        entry_mc_sol: f64,
        entry_price_sol: f64,
        our_buy_sol: f64,
        token_amount: Option<u64>,
    ) {
        if self.data.tokens.contains_key(mint) {
            // Already tracking this token
            return;
        }
        
        let now = Utc::now();
        let history = TokenHistory {
            mint: mint.to_string(),
            bonding_curve: bonding_curve.to_string(),
            entry_mc_sol,
            entry_price_sol,
            entry_timestamp: now,
            our_buy_sol,
            token_amount,
            snapshots: Vec::new(),
            peak_mc_sol: entry_mc_sol,
            peak_pnl_percent: 0.0,
            lowest_mc_sol: entry_mc_sol,
            is_active: true,
            sell_timestamp: None,
            final_pnl_percent: None,
        };
        
        self.data.tokens.insert(mint.to_string(), history);
        self.last_snapshot.insert(mint.to_string(), Instant::now() - self.snapshot_interval * 2); // Allow immediate first snapshot
        self.dirty = true;
        
        let mint_short = if mint.len() > 8 { &mint[..8] } else { mint };
        use crate::utils::sol_to_usd;
        eprintln!("📊 HISTORY: Registered token {} for tracking (entry MC: {:.2} SOL (${:.0}))", mint_short, entry_mc_sol, sol_to_usd(entry_mc_sol));
    }
    
    /// Record a price/MC snapshot for a token
    /// Returns true if snapshot was recorded, false if skipped (too soon)
    pub fn record_snapshot(
        &mut self,
        mint: &str,
        mc_sol: f64,
        price_sol: f64,
        pnl_percent: Option<f64>,
        pnl_sol: Option<f64>,
        current_value_sol: Option<f64>,
        virtual_token_reserves: Option<u64>,
        virtual_sol_reserves: Option<u64>,
        real_token_reserves: Option<u64>,
        real_sol_reserves: Option<u64>,
    ) -> bool {
        // Check if we should record (rate limiting)
        if let Some(last) = self.last_snapshot.get(mint) {
            if last.elapsed() < self.snapshot_interval {
                return false; // Too soon, skip this snapshot
            }
        }
        
        // Get or create token history
        let history = match self.data.tokens.get_mut(mint) {
            Some(h) => h,
            None => {
                // Token not registered - skip (should be registered via register_token first)
                return false;
            }
        };
        
        // Skip if position is no longer active
        if !history.is_active {
            return false;
        }
        
        // Create snapshot
        let snapshot = PriceSnapshot {
            timestamp: Utc::now(),
            mc_sol,
            price_sol,
            pnl_percent,
            pnl_sol,
            current_value_sol,
            virtual_token_reserves,
            virtual_sol_reserves,
            real_token_reserves,
            real_sol_reserves,
        };
        
        // Update peaks and lows
        if mc_sol > history.peak_mc_sol {
            history.peak_mc_sol = mc_sol;
        }
        if mc_sol < history.lowest_mc_sol {
            history.lowest_mc_sol = mc_sol;
        }
        if let Some(pnl) = pnl_percent {
            if pnl > history.peak_pnl_percent {
                history.peak_pnl_percent = pnl;
            }
        }
        
        // Add snapshot
        history.snapshots.push(snapshot);
        self.data.total_snapshots += 1;
        self.data.last_update = Utc::now();
        
        // Update last snapshot time
        self.last_snapshot.insert(mint.to_string(), Instant::now());
        self.dirty = true;
        
        // Auto-save if needed
        if self.last_save.elapsed() >= self.save_interval {
            let _ = self.save();
        }
        
        true
    }
    
    /// Record snapshot from bonding curve data directly
    pub fn record_from_bonding_curve(
        &mut self,
        mint: &str,
        curve: &crate::accounts::BondingCurveAccount,
        pnl_percent: Option<f64>,
        pnl_sol: Option<f64>,
        current_value_sol: Option<f64>,
    ) -> bool {
        let mc_sol = curve.calculate_mc_sol();
        let price_sol = curve.get_token_price_sol();
        
        self.record_snapshot(
            mint,
            mc_sol,
            price_sol,
            pnl_percent,
            pnl_sol,
            current_value_sol,
            Some(curve.virtual_token_reserves),
            Some(curve.virtual_sol_reserves),
            Some(curve.real_token_reserves),
            Some(curve.real_sol_reserves),
        )
    }
    
    /// Mark a position as sold
    pub fn mark_sold(&mut self, mint: &str, final_pnl_percent: Option<f64>) {
        if let Some(history) = self.data.tokens.get_mut(mint) {
            history.is_active = false;
            history.sell_timestamp = Some(Utc::now());
            history.final_pnl_percent = final_pnl_percent;
            self.dirty = true;
            
            let mint_short = if mint.len() > 8 { &mint[..8] } else { mint };
            eprintln!("📊 HISTORY: Marked {} as sold (final PnL: {:?}%)", mint_short, final_pnl_percent);
        }
    }
    
    /// Save history to JSON file
    pub fn save(&mut self) -> Result<()> {
        if !self.dirty {
            return Ok(()); // Nothing to save
        }
        
        let file = File::create(&self.file_path)?;
        let writer = BufWriter::new(file);
        serde_json::to_writer_pretty(writer, &self.data)?;
        
        self.last_save = Instant::now();
        self.dirty = false;
        
        eprintln!("📊 HISTORY: Saved {} tokens, {} snapshots to {}", 
                 self.data.tokens.len(), 
                 self.data.total_snapshots,
                 self.file_path);
        
        Ok(())
    }
    
    /// Force save (ignores dirty flag)
    pub fn force_save(&mut self) -> Result<()> {
        self.dirty = true;
        self.save()
    }
    
    /// Get history for a specific token
    pub fn get_token_history(&self, mint: &str) -> Option<&TokenHistory> {
        self.data.tokens.get(mint)
    }
    
    /// Get all active tokens
    pub fn get_active_tokens(&self) -> Vec<&TokenHistory> {
        self.data.tokens.values()
            .filter(|h| h.is_active)
            .collect()
    }
    
    /// Get total snapshot count
    pub fn total_snapshots(&self) -> u64 {
        self.data.total_snapshots
    }
    
    /// Get file path
    pub fn file_path(&self) -> &str {
        &self.file_path
    }
    
    /// Export to JSONL (JSON Lines) format for easier streaming/processing
    pub fn export_jsonl(&self, path: &str) -> Result<()> {
        let mut file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(path)?;
        
        for (mint, history) in &self.data.tokens {
            // Write header line for this token
            let header = serde_json::json!({
                "type": "token_header",
                "mint": mint,
                "bonding_curve": history.bonding_curve,
                "entry_mc_sol": history.entry_mc_sol,
                "entry_price_sol": history.entry_price_sol,
                "entry_timestamp": history.entry_timestamp,
                "our_buy_sol": history.our_buy_sol,
                "token_amount": history.token_amount,
                "peak_mc_sol": history.peak_mc_sol,
                "peak_pnl_percent": history.peak_pnl_percent,
                "lowest_mc_sol": history.lowest_mc_sol,
                "is_active": history.is_active,
                "sell_timestamp": history.sell_timestamp,
                "final_pnl_percent": history.final_pnl_percent,
            });
            writeln!(file, "{}", serde_json::to_string(&header)?)?;
            
            // Write each snapshot as a line
            for snapshot in &history.snapshots {
                let line = serde_json::json!({
                    "type": "snapshot",
                    "mint": mint,
                    "timestamp": snapshot.timestamp,
                    "mc_sol": snapshot.mc_sol,
                    "price_sol": snapshot.price_sol,
                    "pnl_percent": snapshot.pnl_percent,
                    "pnl_sol": snapshot.pnl_sol,
                    "current_value_sol": snapshot.current_value_sol,
                    "virtual_token_reserves": snapshot.virtual_token_reserves,
                    "virtual_sol_reserves": snapshot.virtual_sol_reserves,
                    "real_token_reserves": snapshot.real_token_reserves,
                    "real_sol_reserves": snapshot.real_sol_reserves,
                });
                writeln!(file, "{}", serde_json::to_string(&line)?)?;
            }
        }
        
        eprintln!("📊 HISTORY: Exported to JSONL: {}", path);
        Ok(())
    }
    
    /// Get statistics summary
    pub fn get_stats(&self) -> HistoryStats {
        let active_count = self.data.tokens.values().filter(|t| t.is_active).count();
        let sold_count = self.data.tokens.values().filter(|t| !t.is_active).count();
        
        let avg_snapshots = if !self.data.tokens.is_empty() {
            self.data.total_snapshots as f64 / self.data.tokens.len() as f64
        } else {
            0.0
        };
        
        HistoryStats {
            total_tokens: self.data.tokens.len(),
            active_tokens: active_count,
            sold_tokens: sold_count,
            total_snapshots: self.data.total_snapshots,
            avg_snapshots_per_token: avg_snapshots,
            session_duration_secs: (Utc::now() - self.data.session_start).num_seconds() as u64,
        }
    }
}

/// Statistics summary
#[derive(Debug, Clone)]
pub struct HistoryStats {
    pub total_tokens: usize,
    pub active_tokens: usize,
    pub sold_tokens: usize,
    pub total_snapshots: u64,
    pub avg_snapshots_per_token: f64,
    pub session_duration_secs: u64,
}

impl Drop for HistoryTracker {
    fn drop(&mut self) {
        // Auto-save on drop
        if self.dirty {
            let _ = self.save();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_history_tracker_creation() {
        let tracker = HistoryTracker::new();
        assert!(tracker.is_ok());
    }
    
    #[test]
    fn test_register_and_record() {
        let mut tracker = HistoryTracker::with_interval(10).unwrap(); // 10ms for testing
        
        tracker.register_token(
            "test_mint_123456789",
            "test_bonding_curve",
            36.5, // ~5000 USD at 137 SOL/USD
            0.00005,
            0.1,
            Some(2000000),
        );
        
        // First snapshot should be recorded
        let recorded = tracker.record_snapshot(
            "test_mint_123456789",
            37.2, // ~5100 USD
            0.000051,
            Some(2.0),
            Some(0.002),
            Some(0.102),
            None, None, None, None,
        );
        assert!(recorded);
        
        // Second immediate snapshot should be skipped (rate limited)
        let recorded2 = tracker.record_snapshot(
            "test_mint_123456789",
            38.0, // ~5200 USD
            0.000052,
            Some(4.0),
            None, None, None, None, None, None,
        );
        assert!(!recorded2);
        
        // Check history
        let history = tracker.get_token_history("test_mint_123456789");
        assert!(history.is_some());
        let h = history.unwrap();
        assert_eq!(h.snapshots.len(), 1);
        assert_eq!(h.peak_mc_sol, 37.2);
    }
    
    #[test]
    fn test_mark_sold() {
        let mut tracker = HistoryTracker::new().unwrap();
        
        tracker.register_token(
            "test_mint_sold",
            "test_bc",
            36.5, // ~5000 USD at 137 SOL/USD
            0.00005,
            0.1,
            None,
        );
        
        tracker.mark_sold("test_mint_sold", Some(50.0));
        
        let history = tracker.get_token_history("test_mint_sold").unwrap();
        assert!(!history.is_active);
        assert_eq!(history.final_pnl_percent, Some(50.0));
    }
    
    #[test]
    fn test_stats() {
        let mut tracker = HistoryTracker::new().unwrap();
        
        tracker.register_token("mint1", "bc1", 36.5, 0.00005, 0.1, None);
        tracker.register_token("mint2", "bc2", 43.8, 0.00006, 0.1, None);
        tracker.mark_sold("mint1", Some(10.0));
        
        let stats = tracker.get_stats();
        assert_eq!(stats.total_tokens, 2);
        assert_eq!(stats.active_tokens, 1);
        assert_eq!(stats.sold_tokens, 1);
    }
}

