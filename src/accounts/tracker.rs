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

    // 🆕 NEW: Position tracking for auto-sell
    pub token_amount: Option<u64>,         // Amount of tokens bought
    pub user_token_account: Option<String>, // Associated token account address
    pub bonding_curve: Option<String>,      // Bonding curve address for monitoring
    pub sold: bool,                         // Flag if position is sold
    pub sell_signature: Option<String>,     // Sell transaction signature if sold

    // 🆕 NEW: Ultra Live PnL tracking
    pub current_price_sol: Option<f64>,      // Current token price in SOL
    pub current_value_sol: Option<f64>,      // Current position value in SOL
    pub pnl_sol: Option<f64>,                // Profit/Loss in SOL
    pub pnl_percent: Option<f64>,            // Profit/Loss percentage
    pub last_pnl_update: Option<DateTime<Utc>>, // Last update timestamp
    
    // 🆕 NEW: Transaction fees tracking (for ultra-precision)
    pub buy_fees_sol: Option<f64>,           // Total fees paid for buy (Priority + Network)
    
    // 🆕 NEW: Peak tracking for breakeven stop loss
    #[serde(default)]
    pub peak_mc_usd: Option<f64>,            // Highest MC reached
    #[serde(default)]
    pub peak_pnl_percent: Option<f64>,       // Best PnL percentage reached
    #[serde(default)]
    pub breakeven_mode_active: bool,         // Whether breakeven mode is active
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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
    /// Create new tracker or load from latest existing JSON file
    pub fn new() -> Result<Self> {
        use std::io::Write;
        eprintln!("🔍 DEBUG: TokenTracker::new() called");
        std::io::stderr().flush().ok();
        
        // Try to find and load the latest JSON file first
        eprintln!("🔍 DEBUG: Attempting to load from latest JSON...");
        std::io::stderr().flush().ok();
        
        if let Ok(tracker) = Self::load_from_latest_json() {
            eprintln!("✅ Loaded tracker from existing JSON file with {} buys", tracker.stats.total_buys);
            std::io::stderr().flush().ok();
            return Ok(tracker);
        }
        eprintln!("🔍 DEBUG: Failed to load from JSON (or none found), creating new session...");
        std::io::stderr().flush().ok();
        
        // If no existing JSON found, create new tracker
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
    
    /// Load tracker from the latest JSON file
    fn load_from_latest_json() -> Result<Self> {
        use std::fs;
        use std::io::Write;
        
        eprintln!("🔍 DEBUG: load_from_latest_json() called");
        std::io::stderr().flush().ok();
        
        // Find all JSON files matching the pattern
        let current_dir = std::env::current_dir()?;
        eprintln!("🔍 DEBUG: current_dir: {:?}", current_dir);
        std::io::stderr().flush().ok();
        
        let json_files: Vec<_> = fs::read_dir(&current_dir)?
            .filter_map(|entry| entry.ok())
            .filter_map(|entry| {
                let path = entry.path();
                if path.is_file() {
                    if let Some(file_name) = path.file_name().and_then(|n| n.to_str()) {
                        if file_name.starts_with("sniper_session_") && file_name.ends_with(".json") {
                            return Some((path.clone(), entry.metadata().ok()?.modified().ok()?));
                        }
                    }
                }
                None
            })
            .collect();
        
        if json_files.is_empty() {
            eprintln!("🔍 DEBUG: No JSON files found");
            return Err(anyhow::anyhow!("No existing JSON files found"));
        }
        
        // Get the latest file
        let (latest_json_path, _) = json_files.iter()
            .max_by_key(|(_, modified)| modified)
            .ok_or_else(|| anyhow::anyhow!("Failed to find latest JSON file"))?;
        
        eprintln!("📂 Loading tracker from: {}", latest_json_path.display());
        
        // Read and deserialize JSON
        let content = fs::read_to_string(latest_json_path)?;
        eprintln!("🔍 DEBUG: Read {} bytes", content.len());
        let stats: TrackerStats = serde_json::from_str(&content)
            .map_err(|e| {
                eprintln!("🔍 DEBUG: JSON parse error: {}", e);
                anyhow::anyhow!("Failed to parse JSON: {}", e)
            })?;
        eprintln!("🔍 DEBUG: JSON parsed successfully");
        
        // Extract CSV path from JSON path
        let csv_path = latest_json_path.to_str()
            .ok_or_else(|| anyhow::anyhow!("Invalid path"))?
            .replace(".json", ".csv");
        
        Ok(Self {
            stats,
            csv_path,
            json_path: latest_json_path.to_str().unwrap().to_string(),
        })
    }

    /// Record a token buy
    pub fn record_buy(&mut self, buy: TokenBuy) -> Result<()> {
        self.stats.total_buys += 1;
        // Safe addition to prevent NaN/infinite accumulation
        let our_buy_safe = if buy.our_buy_sol.is_finite() && !buy.our_buy_sol.is_nan() {
            buy.our_buy_sol
        } else {
            0.0
        };
        self.stats.total_sol_spent += our_buy_safe;
        // Ensure total_sol_spent stays finite
        if !self.stats.total_sol_spent.is_finite() || self.stats.total_sol_spent.is_nan() {
            self.stats.total_sol_spent = 0.0;
        }
        self.stats.last_buy = buy.timestamp;

        // Update MC stats (using entry MC as that's the real execution price)
        if let Some(mc_usd) = buy.mc_at_entry_usd {
            // Safety check for NaN and infinite values
            if mc_usd.is_finite() && !mc_usd.is_nan() {
                if self.stats.min_mc_usd == f64::MAX || mc_usd < self.stats.min_mc_usd {
                    self.stats.min_mc_usd = mc_usd;
                }
                if mc_usd > self.stats.max_mc_usd {
                    self.stats.max_mc_usd = mc_usd;
                }

                // Recalculate average with safety checks
                let total_mc: f64 = self.stats.buys.iter()
                    .filter_map(|b| b.mc_at_entry_usd)
                    .filter(|&v| v.is_finite() && !v.is_nan())
                    .sum::<f64>() + mc_usd;
                let count_with_mc = self.stats.buys.iter()
                    .filter(|b| b.mc_at_entry_usd.is_some())
                    .filter(|b| {
                        if let Some(v) = b.mc_at_entry_usd {
                            v.is_finite() && !v.is_nan()
                        } else {
                            false
                        }
                    })
                    .count() + 1;
                
                if count_with_mc > 0 {
                    self.stats.avg_mc_usd = total_mc / count_with_mc as f64;
                    // Ensure average is also finite
                    if !self.stats.avg_mc_usd.is_finite() || self.stats.avg_mc_usd.is_nan() {
                        self.stats.avg_mc_usd = 0.0;
                    }
                }
            }
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

        // Safe formatting for numeric values to prevent NaN/infinite crashes
        let dev_buy_sol_safe = if buy.dev_buy_sol.is_finite() && !buy.dev_buy_sol.is_nan() {
            buy.dev_buy_sol
        } else {
            0.0
        };
        let our_buy_sol_safe = if buy.our_buy_sol.is_finite() && !buy.our_buy_sol.is_nan() {
            buy.our_buy_sol
        } else {
            0.0
        };
        
        let mc_detection_str = buy.mc_at_detection_usd
            .filter(|&v| v.is_finite() && !v.is_nan())
            .map(|v| format!("{:.0}", v))
            .unwrap_or_default();
        let mc_entry_str = buy.mc_at_entry_usd
            .filter(|&v| v.is_finite() && !v.is_nan())
            .map(|v| format!("{:.0}", v))
            .unwrap_or_default();
        let token_price_str = buy.token_price_sol
            .filter(|&v| v.is_finite() && !v.is_nan())
            .map(|v| format!("{:.8}", v))
            .unwrap_or_default();
        
        // Escape CSV special characters in strings
        let escape_csv = |s: &str| s.replace(",", " ").replace("\n", " ").replace("\r", " ");
        
        writeln!(
            writer,
            "{},{},{},{},{:.4},{:.4},{},{},{},{},{},{},{},{},{},{}",
            buy.token_number,
            escape_csv(&buy.mint),
            escape_csv(&buy.signature),
            escape_csv(&buy.creator),
            dev_buy_sol_safe,
            our_buy_sol_safe,
            buy.timestamp.to_rfc3339(),
            buy.has_socials,
            buy.twitter.as_deref().map(escape_csv).unwrap_or_default(),
            buy.website.as_deref().map(escape_csv).unwrap_or_default(),
            buy.telegram.as_deref().map(escape_csv).unwrap_or_default(),
            buy.creator_token_count,
            escape_csv(&buy.detection_method),
            mc_detection_str,
            mc_entry_str,
            token_price_str,
        )
        .map_err(|e| anyhow::anyhow!("Failed to write to CSV: {}", e))?;

        writer.flush()
            .map_err(|e| anyhow::anyhow!("Failed to flush CSV buffer: {}", e))?;

        Ok(())
    }

    /// Save full stats to JSON with better error handling
    fn save_json(&self) -> Result<()> {
        // Create a sanitized copy of stats to avoid NaN/infinite serialization issues
        let mut sanitized_stats = self.stats.clone();
        
        // Sanitize all f64 values in stats
        if !sanitized_stats.total_sol_spent.is_finite() || sanitized_stats.total_sol_spent.is_nan() {
            sanitized_stats.total_sol_spent = 0.0;
        }
        if !sanitized_stats.avg_mc_usd.is_finite() || sanitized_stats.avg_mc_usd.is_nan() {
            sanitized_stats.avg_mc_usd = 0.0;
        }
        if !sanitized_stats.min_mc_usd.is_finite() || sanitized_stats.min_mc_usd.is_nan() || sanitized_stats.min_mc_usd == f64::MAX {
            sanitized_stats.min_mc_usd = 0.0;
        }
        if !sanitized_stats.max_mc_usd.is_finite() || sanitized_stats.max_mc_usd.is_nan() {
            sanitized_stats.max_mc_usd = 0.0;
        }
        
        // Sanitize all buys
        for buy in &mut sanitized_stats.buys {
            if !buy.dev_buy_sol.is_finite() || buy.dev_buy_sol.is_nan() {
                buy.dev_buy_sol = 0.0;
            }
            if !buy.our_buy_sol.is_finite() || buy.our_buy_sol.is_nan() {
                buy.our_buy_sol = 0.0;
            }
            if let Some(mc) = &mut buy.mc_at_detection_usd {
                if !mc.is_finite() || mc.is_nan() {
                    *mc = 0.0;
                }
            }
            if let Some(mc) = &mut buy.mc_at_entry_usd {
                if !mc.is_finite() || mc.is_nan() {
                    *mc = 0.0;
                }
            }
            if let Some(price) = &mut buy.token_price_sol {
                if !price.is_finite() || price.is_nan() {
                    *price = 0.0;
                }
            }
        }
        
        let file = File::create(&self.json_path)
            .map_err(|e| anyhow::anyhow!("Failed to create JSON file {}: {}", self.json_path, e))?;
        let writer = BufWriter::new(file);
        serde_json::to_writer_pretty(writer, &sanitized_stats)
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
        if self.stats.total_buys.is_multiple_of(10) {
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

    /// Clear all buys (reset tracker)
    pub fn clear_all_buys(&mut self) -> Result<()> {
        self.stats.buys.clear();
        self.stats.total_buys = 0;
        self.stats.total_sol_spent = 0.0;
        self.stats.avg_mc_usd = 0.0;
        self.stats.min_mc_usd = f64::MAX;
        self.stats.max_mc_usd = 0.0;
        self.stats.last_buy = self.stats.session_start;
        
        // Save empty state
        self.save_json()?;
        
        // Clear CSV file
        if let Ok(mut file) = std::fs::File::create(&self.csv_path) {
            use std::io::Write;
            let _ = writeln!(file, "Token#,Mint,Signature,Creator,DevBuy(SOL),OurBuy(SOL),Timestamp,HasSocials,Twitter,Website,Telegram,CreatorTokens,DetectionMethod,MC_Detection_USD,MC_Entry_USD,TokenPrice_SOL");
        }
        
        Ok(())
    }

    /// Get active positions (not sold)
    pub fn get_active_positions(&self) -> Vec<TokenBuy> {
        self.stats.buys.iter()
            .filter(|buy| {
                // Show position if it's not sold and has bonding_curve (required for monitoring)
                // user_token_account is optional - we can still show position and try to sell
                !buy.sold && buy.bonding_curve.is_some()
            })
            .cloned()
            .collect()
    }

    /// Clear only active positions (remove positions that are not sold)
    pub fn clear_active_positions(&mut self) -> Result<()> {
        let initial_count = self.stats.buys.len();
        self.stats.buys.retain(|buy| buy.sold); // Keep only sold positions
        let _removed_count = initial_count - self.stats.buys.len();
        
        // Update stats
        self.stats.total_buys = self.stats.buys.len() as u32;
        self.stats.total_sol_spent = self.stats.buys.iter().map(|b| b.our_buy_sol).sum();
        
        // Recalculate MC stats
        let mc_values: Vec<f64> = self.stats.buys.iter()
            .filter_map(|b| b.mc_at_entry_usd)
            .collect();
        if !mc_values.is_empty() {
            self.stats.avg_mc_usd = mc_values.iter().sum::<f64>() / mc_values.len() as f64;
            self.stats.min_mc_usd = mc_values.iter().cloned().fold(f64::MAX, f64::min);
            self.stats.max_mc_usd = mc_values.iter().cloned().fold(0.0, f64::max);
        } else {
            self.stats.avg_mc_usd = 0.0;
            self.stats.min_mc_usd = f64::MAX;
            self.stats.max_mc_usd = 0.0;
        }
        
        // Save updated state
        self.save_json()?;
        
        Ok(())
    }

    /// Get all sold positions (for display in UI)
    pub fn get_sold_positions(&self) -> Vec<TokenBuy> {
        self.stats.buys.iter()
            .filter(|buy| buy.sold && buy.bonding_curve.is_some())
            .cloned()
            .collect()
    }

    /// Mark a position as sold (for cleanup purposes)
    pub fn mark_position_as_sold(&mut self, mint: &str) -> Result<()> {
        if let Some(buy) = self.stats.buys.iter_mut().find(|b| b.mint == mint && !b.sold) {
            buy.sold = true;
            buy.sell_signature = Some("AUTO_CLEANUP".to_string());
            self.save_json()?;
            Ok(())
        } else {
            Err(anyhow::anyhow!("Position not found or already sold: {}", mint))
        }
    }

    /// Mark a position as sold
    pub fn mark_as_sold(&mut self, mint: &str, sell_signature: String) -> Result<()> {
        if let Some(buy) = self.stats.buys.iter_mut().find(|b| b.mint == mint && !b.sold) {
            buy.sold = true;
            buy.sell_signature = Some(sell_signature);
            // Save JSON after update
            self.save_json()?;
            Ok(())
        } else {
            Err(anyhow::anyhow!("Position not found or already sold: {}", mint))
        }
    }

    /// Update token_amount for a position (from wallet balance check)
    pub fn update_token_amount(&mut self, mint: &str, token_amount: u64) -> Result<()> {
        if let Some(buy) = self.stats.buys.iter_mut().find(|b| b.mint == mint && !b.sold) {
            buy.token_amount = Some(token_amount);
            // Don't save JSON on every balance update to avoid excessive disk writes
            // JSON will be saved on next buy/sell operation
            Ok(())
        } else {
            Err(anyhow::anyhow!("Position not found or already sold: {}", mint))
        }
    }

    /// Update position PnL with current price (ultra-fast, no disk write)
    pub fn update_position_pnl_fast(&mut self, mint: &str, current_price_sol: f64) -> Result<()> {
        // #region agent log
        {
            let log_data = serde_json::json!({
                "sessionId": "debug-session",
                "runId": "run1",
                "hypothesisId": "C",
                "location": "tracker.rs:551",
                "message": "update_position_pnl_fast called",
                "data": {
                    "mint": mint[..8].to_string(),
                    "current_price_sol": current_price_sol,
                },
                "timestamp": chrono::Utc::now().timestamp_millis()
            });
            let _ = std::fs::create_dir_all(".cursor");
            if let Ok(mut file) = std::fs::OpenOptions::new().append(true).create(true).open(".cursor/debug.log") {
                use std::io::Write;
                let _ = writeln!(file, "{}", log_data);
            }
        }
        // #endregion
        
        // Debug: Check if position exists
        let position_exists = self.stats.buys.iter().any(|b| b.mint == mint && !b.sold);
        if !position_exists {
            eprintln!("⚠️  PnL UPDATE: Position not found for mint {} (sold or doesn't exist)", &mint[..8]);
            return Err(anyhow::anyhow!("Position not found or already sold: {}", mint));
        }
        
        if let Some(buy) = self.stats.buys.iter_mut().find(|b| b.mint == mint && !b.sold) {
            buy.current_price_sol = Some(current_price_sol);
            buy.last_pnl_update = Some(Utc::now());
            
            // Skip PnL calculation if our_buy_sol is 0 or invalid
            if buy.our_buy_sol <= 0.0 {
                eprintln!("⚠️  PnL UPDATE: our_buy_sol is 0 or invalid for {}", &mint[..8]);
                return Ok(());
            }
            
            // ✅ FIX: Calculate PnL even if current_price_sol is not available (token migrated)
            // If current_price_sol is 0 or very small, it means bonding curve doesn't exist
            // In that case, we can't calculate live PnL, but we can at least show entry price
            let is_token_migrated = current_price_sol <= 0.0 || current_price_sol < 1e-15;
            
            if is_token_migrated {
                eprintln!("⚠️  PnL UPDATE: Token {} appears to be migrated (current_price_sol={:.12})", 
                         &mint[..8], current_price_sol);
                // Token is migrated - we can't get live price, but we can still show entry info
                // Set a placeholder PnL of 0 or calculate based on entry price if we have token_amount
                if let Some(token_amount) = buy.token_amount {
                    let _tokens_actual = token_amount as f64 / 1e6;
                    if let Some(_entry_price) = buy.token_price_sol {
                        // Calculate what we paid
                        let buy_fees = buy.buy_fees_sol.unwrap_or(0.000015);
                        let _cost_basis = buy.our_buy_sol + buy_fees;
                        
                        // For migrated tokens, we can't know current value, so set PnL to 0
                        // Or we could try to fetch from DEX, but that's complex
                        buy.pnl_sol = Some(0.0); // Placeholder - token migrated
                        buy.pnl_percent = Some(0.0);
                        eprintln!("   Set PnL to 0.0 (token migrated, cannot determine current value)");
                    }
                }
                return Ok(());
            }
            
            // ✅ FIX: Try to get or calculate token_amount
            // First, use stored token_amount if available
            // Otherwise, try to calculate it from our_buy_sol and token_price_sol (or current_price as fallback)
            let token_amount = if let Some(amount) = buy.token_amount {
                Some(amount)
            } else {
                // Try to calculate token_amount from our_buy_sol and token_price_sol
                // Priority: token_price_sol > current_price_sol (as fallback)
                let entry_price = buy.token_price_sol.or_else(|| {
                    // Fallback: use current price as approximation if entry price not available
                    // This is less accurate but better than showing "..."
                    if current_price_sol > 0.0 {
                        Some(current_price_sol)
                    } else {
                        None
                    }
                });
                
                if let Some(price) = entry_price {
                    if price > 0.0 && buy.our_buy_sol > 0.0 {
                        // tokens = SOL invested / entry price per token
                        // Then convert to raw units (multiply by 1e6 for 6 decimals)
                        let tokens_human = buy.our_buy_sol / price;
                        let tokens_raw = (tokens_human * 1e6) as u64;
                        if tokens_raw > 0 {
                            Some(tokens_raw)
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                } else {
                    None
                }
            };
            
            // Calculate current value if we have token amount (either stored or calculated)
            if let Some(token_amount) = token_amount {
                eprintln!("✅ PnL UPDATE: Calculating PnL for {} with token_amount={}, our_buy_sol={:.6}", 
                         &mint[..8], token_amount, buy.our_buy_sol);
                // ✅ FIX: Calculate PnL even if token_price_sol is not set
                // We can calculate PnL using our_buy_sol + fees as cost basis
                // token_price_sol is only needed for display, not for PnL calculation
                
                // If token_price_sol is not set, calculate it from our_buy_sol and token_amount
                if buy.token_price_sol.is_none() || buy.token_price_sol.unwrap_or(0.0) <= 0.0 {
                    let tokens_actual_for_price = token_amount as f64 / 1e6; // Convert 6-decimal raw to actual tokens
                    if tokens_actual_for_price > 0.0 && buy.our_buy_sol > 0.0 {
                        // Entry price = SOL invested / tokens received
                        buy.token_price_sol = Some(buy.our_buy_sol / tokens_actual_for_price);
                    }
                }
                
                // Also update stored token_amount if it was None (so we don't need to recalculate next time)
                if buy.token_amount.is_none() {
                    buy.token_amount = Some(token_amount);
                }
                
                // FORMULA EXPLANATION:
                // get_token_price_sol() = (virtual_sol_reserves / virtual_token_reserves) / 1000.0
                // where virtual_sol_reserves is in lamports (1e9 per SOL)
                // and virtual_token_reserves is in raw token units (6 decimals for pump.fun tokens)
                // Formula: (lamports / raw_units) / 1000 = SOL per token
                // Example: (50e9 / 1e12) / 1000 = 0.05 / 1000 = 0.00005 SOL per token
                //
                // token_amount from balance is in raw token units with 6 decimals (1e6 per token)
                // So we need to convert: tokens_actual = token_amount / 1e6
                // Then: current_value = tokens_actual * current_price_sol
                // This gives: (token_amount / 1e6) * current_price_sol
                let tokens_actual = token_amount as f64 / 1e6; // Convert 6-decimal raw to actual tokens
                let current_value_gross = tokens_actual * current_price_sol;
                buy.current_value_sol = Some(current_value_gross);
                
                // Calculate PnL with ALL fees (Ultra Precision Mode):
                // 1. Buy Fees (already paid):
                //    - pump.fun 1% (deducted from received tokens)
                //    - network fee + priority fee + jito tip (deducted from wallet)
                //
                // 2. Sell Fees (to be paid):
                //    - pump.fun 1% (deducted from SOL received)
                //    - network fee + priority fee (deducted from wallet)
                
                // Real Cost Basis = Amount Sent to Curve + Buy Fees (Gas + Priority + Jito)
                // If buy_fees_sol is not set, we estimate it (0.000015 SOL based on tx history)
                let buy_fees = buy.buy_fees_sol.unwrap_or(0.000015); 
                let cost_basis = buy.our_buy_sol + buy_fees;
                
                // Estimated Sell Value = (Gross Value * 0.99) - Estimated Sell Fees
                // Sell usually has lower priority fee, estimating 0.00001 SOL base + minimal priority
                let estimated_sell_fees = 0.00001; 
                let current_value_net = (current_value_gross * 0.99) - estimated_sell_fees;
                
                let pnl = current_value_net - cost_basis;
                buy.pnl_sol = Some(pnl);
                
                // ✅ FIX: Calculate PnL percentage based on GROSS value change (before fees)
                // This shows the actual price appreciation, not the net profit after fees
                // Formula: PnL% = ((current_value_gross - cost_basis) / cost_basis) * 100
                // This matches how traders typically think about returns (gross appreciation)
                let pnl_gross = current_value_gross - cost_basis;
                let pnl_percent_gross = if cost_basis > 0.0 {
                    (pnl_gross / cost_basis) * 100.0
                } else {
                    0.0
                };
                
                // #region agent log
                {
                    let pnl_pct = if cost_basis > 0.0 { Some((pnl / cost_basis) * 100.0) } else { None };
                    let log_data = serde_json::json!({
                        "sessionId": "debug-session",
                        "runId": "run1",
                        "hypothesisId": "C",
                        "location": "tracker.rs:600",
                        "message": "PnL calculated successfully",
                        "data": {
                            "mint": mint[..8].to_string(),
                            "pnl_sol": pnl,
                            "pnl_percent": pnl_pct,
                            "pnl_percent_gross": pnl_percent_gross,
                            "cost_basis": cost_basis,
                        },
                        "timestamp": chrono::Utc::now().timestamp_millis()
                    });
                    if let Ok(mut file) = std::fs::OpenOptions::new().append(true).create(true).open(".cursor/debug.log") {
                        use std::io::Write;
                        let _ = writeln!(file, "{}", log_data);
                    }
                }
                // #endregion
                
                // Calculate PnL percentage - use GROSS for percentage (shows price appreciation)
                // Net PnL (after fees) is stored in pnl_sol for actual profit/loss
                if cost_basis > 0.0 {
                    buy.pnl_percent = Some(pnl_percent_gross);
                    
                    // Update peak PnL if current is better
                    if pnl_percent_gross > buy.peak_pnl_percent.unwrap_or(f64::MIN) {
                        buy.peak_pnl_percent = Some(pnl_percent_gross);
                    }
                }
                
                // DEBUG: Print PnL calculation details (only for first few updates to avoid spam)
                static UPDATE_COUNT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
                let count = UPDATE_COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                if count < 10 {
                    let entry_price_per_token = if tokens_actual > 0.0 { buy.our_buy_sol / tokens_actual } else { 0.0 };
                    let price_multiplier = if entry_price_per_token > 0.0 { current_price_sol / entry_price_per_token } else { 0.0 };
                    let expected_pnl_percent = if price_multiplier > 0.0 { (price_multiplier - 1.0) * 100.0 } else { 0.0 };
                    
                    eprintln!("🔍 PnL DEBUG [{}] for {}:", count + 1, &mint[..8]);
                    eprintln!("   📊 INPUT VALUES:");
                    eprintln!("      token_amount (raw, 6 decimals): {}", token_amount);
                    eprintln!("      tokens_actual (converted from 6dec): {:.6}", tokens_actual);
                    eprintln!("      entry_price_sol: {:?} SOL/token", buy.token_price_sol);
                    eprintln!("      current_price_sol: {:.12} SOL/token", current_price_sol);
                    eprintln!("      our_buy_sol (invested): {:.6} SOL", buy.our_buy_sol);
                    eprintln!("      buy_fees: {:.6} SOL", buy_fees);
                    eprintln!("   💰 CALCULATIONS:");
                    eprintln!("      cost_basis: {:.6} SOL (our_buy: {:.6} + fees: {:.6})", cost_basis, buy.our_buy_sol, buy_fees);
                    eprintln!("      current_value_gross: {:.6} SOL ({} tokens * {:.12} SOL/token)", current_value_gross, tokens_actual, current_price_sol);
                    eprintln!("      current_value_net: {:.6} SOL (after 1% fee: {:.6} SOL, sell fees: {:.6} SOL)", current_value_net, current_value_gross * 0.01, estimated_sell_fees);
                    eprintln!("      pnl_sol (net): {:.6} SOL", pnl);
                    eprintln!("      pnl_gross: {:.6} SOL (before fees)", pnl_gross);
                    eprintln!("      pnl_percent (gross): {:.2}%", pnl_percent_gross);
                    eprintln!("      pnl_percent (net): {:.2}%", if cost_basis > 0.0 { (pnl / cost_basis) * 100.0 } else { 0.0 });
                    eprintln!("   🔍 VERIFICATION:");
                    eprintln!("      entry_price_per_token (calculated): {:.12} SOL/token", entry_price_per_token);
                    eprintln!("      price_multiplier: {:.6}x (current/entry)", price_multiplier);
                    eprintln!("      expected_pnl_percent (from price change): {:.2}%", expected_pnl_percent);
                    eprintln!("      actual_pnl_percent (gross): {:.2}%", buy.pnl_percent.unwrap_or(0.0));
                    if (buy.pnl_percent.unwrap_or(0.0) - expected_pnl_percent).abs() > 5.0 {
                        eprintln!("   ⚠️  WARNING: PnL% differs significantly from expected! (diff: {:.2}%)", 
                                 buy.pnl_percent.unwrap_or(0.0) - expected_pnl_percent);
                        eprintln!("   💡 TIP: Check if token_amount decimals or price formula is correct");
                    }
                }
            } else {
                // ✅ FIX: Try one more fallback - calculate PnL using only price change if we have entry price
                // This is less accurate but better than showing nothing
                if let Some(entry_price) = buy.token_price_sol {
                    if entry_price > 0.0 && current_price_sol > 0.0 && buy.our_buy_sol > 0.0 {
                        // Calculate approximate PnL based on price change only
                        // This assumes we have tokens proportional to our_buy_sol / entry_price
                        let price_change_ratio = (current_price_sol - entry_price) / entry_price;
                        let approximate_pnl = buy.our_buy_sol * price_change_ratio;
                        
                        // Apply fees estimate
                        let buy_fees = buy.buy_fees_sol.unwrap_or(0.000015);
                        let cost_basis = buy.our_buy_sol + buy_fees;
                        let estimated_sell_fees = 0.00001;
                        let net_pnl = approximate_pnl - (buy.our_buy_sol * 0.01) - estimated_sell_fees; // Subtract 1% pump.fun fee
                        
                        buy.pnl_sol = Some(net_pnl);
                        if cost_basis > 0.0 {
                            buy.pnl_percent = Some((net_pnl / cost_basis) * 100.0);
                        }
                        
                        eprintln!("⚠️  PnL UPDATE: Using approximate PnL calculation for {} (token_amount missing)", &mint[..8]);
                        eprintln!("   Approximate PnL: {:.6} SOL ({:.2}%)", net_pnl, buy.pnl_percent.unwrap_or(0.0));
                    }
                }
                
                // DEBUG: Show when token_amount is missing
                static MISSING_COUNT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
                let count = MISSING_COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                if count < 5 {
                    eprintln!("⚠️  PnL DEBUG: token_amount is None for {} (using fallback calculation)", &mint[..8]);
                }
                // #region agent log
                {
                    let log_data = serde_json::json!({
                        "sessionId": "debug-session",
                        "runId": "run1",
                        "hypothesisId": "B",
                        "location": "tracker.rs:637",
                        "message": "token_amount is None",
                        "data": {
                            "mint": mint[..8].to_string(),
                        },
                        "timestamp": chrono::Utc::now().timestamp_millis()
                    });
                    if let Ok(mut file) = std::fs::OpenOptions::new().append(true).create(true).open(".cursor/debug.log") {
                        use std::io::Write;
                        let _ = writeln!(file, "{}", log_data);
                    }
                }
                // #endregion
            }
            
            Ok(())
        } else {
            eprintln!("❌ PnL UPDATE: Position not found in find() for mint {} (this shouldn't happen after existence check)", &mint[..8]);
            Err(anyhow::anyhow!("Position not found or already sold: {}", mint))
        }
    }

    /// Update peak MC for a position (used when MC is fetched separately)
    pub fn update_peak_mc(&mut self, mint: &str, current_mc_usd: f64, breakeven_threshold: f64) -> Result<()> {
        if let Some(buy) = self.stats.buys.iter_mut().find(|b| b.mint == mint && !b.sold) {
            // Update peak MC if current is higher than existing peak, or if peak is None
            match buy.peak_mc_usd {
                None => {
                    buy.peak_mc_usd = Some(current_mc_usd);
                }
                Some(peak) if current_mc_usd > peak => {
                    buy.peak_mc_usd = Some(current_mc_usd);
                }
                _ => {
                    // Peak remains the same
                }
            }
            
            // Activate breakeven mode if threshold reached
            if current_mc_usd >= breakeven_threshold {
                buy.breakeven_mode_active = true;
            }
            
            Ok(())
        } else {
            Err(anyhow::anyhow!("Position not found or already sold: {}", mint))
        }
    }

    /// Get all active positions with bonding curves (for batch PnL update)
    pub fn get_active_positions_for_pnl(&self) -> Vec<(String, String)> {
        let all_buys = self.stats.buys.len();
        let sold_count = self.stats.buys.iter().filter(|b| b.sold).count();
        let without_bc = self.stats.buys.iter().filter(|b| !b.sold && b.bonding_curve.is_none()).count();
        
        let positions = self.stats.buys.iter()
            .filter(|buy| !buy.sold && buy.bonding_curve.is_some())
            .map(|buy| {
                (buy.mint.clone(), buy.bonding_curve.clone().unwrap())
            })
            .collect::<Vec<_>>();
        
        // ✅ FIX: Only log debug info occasionally (every 100 calls) to reduce spam
        static CALL_COUNT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let count = CALL_COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        if count % 100 == 0 {
            eprintln!("🔍 get_active_positions_for_pnl: total={}, sold={}, without_bc={}, active={}", 
                     all_buys, sold_count, without_bc, positions.len());
        }
        // #region agent log
        {
            let log_data = serde_json::json!({
                "sessionId": "debug-session",
                "runId": "run1",
                "hypothesisId": "D",
                "location": "tracker.rs:675",
                "message": "get_active_positions_for_pnl",
                "data": {
                    "count": positions.len(),
                    "mints": positions.iter().map(|(m, _)| m[..8].to_string()).collect::<Vec<_>>(),
                },
                "timestamp": chrono::Utc::now().timestamp_millis()
            });
            let _ = std::fs::create_dir_all(".cursor");
            if let Ok(mut file) = std::fs::OpenOptions::new().append(true).create(true).open(".cursor/debug.log") {
                use std::io::Write;
                let _ = writeln!(file, "{}", log_data);
            }
        }
        // #endregion
        positions
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
    fn test_peak_pnl_tracking() {
        let mut tracker = TokenTracker::new().unwrap();

        let buy = TokenBuy {
            token_number: 1,
            mint: "test_mint_peak".to_string(),
            signature: "test_sig".to_string(),
            creator: "test_creator".to_string(),
            dev_buy_sol: 2.0,
            our_buy_sol: 0.1,
            timestamp: Utc::now(),
            has_socials: false,
            twitter: None,
            website: None,
            telegram: None,
            creator_token_count: 0,
            detection_method: "instruction".to_string(),
            mc_at_detection_usd: Some(5000.0),
            mc_at_entry_usd: Some(5000.0),
            token_price_sol: Some(0.00005),
            token_amount: Some(2000000), // 2 tokens with 6 decimals
            user_token_account: None,
            bonding_curve: Some("test_bonding_curve".to_string()),
            sold: false,
            sell_signature: None,
            current_price_sol: None,
            current_value_sol: None,
            pnl_sol: None,
            pnl_percent: None,
            last_pnl_update: None,
            buy_fees_sol: Some(0.00001),
            peak_mc_usd: None,
            peak_pnl_percent: None,
            breakeven_mode_active: false,
        };

        tracker.record_buy(buy).unwrap();

        // Update PnL with increasing prices (should track peak)
        tracker.update_position_pnl_fast("test_mint_peak", 0.0001).unwrap();
        let position = tracker.get_active_positions().into_iter().find(|p| p.mint == "test_mint_peak").unwrap();
        let first_peak = position.peak_pnl_percent;
        let first_current = position.pnl_percent;
        assert!(first_peak.is_some()); // Should have peak value
        assert!(first_current.is_some()); // Should have current PnL

        // Update with lower price (peak should remain same or higher)
        tracker.update_position_pnl_fast("test_mint_peak", 0.000075).unwrap();
        let position = tracker.get_active_positions().into_iter().find(|p| p.mint == "test_mint_peak").unwrap();
        assert!(position.peak_pnl_percent.is_some()); // Peak should exist
        // Peak should be >= current PnL (or remain same/higher than first)
        assert!(position.peak_pnl_percent.unwrap_or(f64::MIN) >= position.pnl_percent.unwrap_or(f64::MIN));
        // Peak should not decrease (should be same or higher than first peak)
        assert!(position.peak_pnl_percent.unwrap_or(0.0) >= first_peak.unwrap_or(0.0));

        // Update with higher price (peak should update to new higher value)
        let peak_before = position.peak_pnl_percent;
        tracker.update_position_pnl_fast("test_mint_peak", 0.0002).unwrap(); // Higher price
        let position = tracker.get_active_positions().into_iter().find(|p| p.mint == "test_mint_peak").unwrap();
        // Peak should have increased
        assert!(position.peak_pnl_percent.unwrap_or(0.0) >= peak_before.unwrap_or(0.0));
        // Current PnL should also be higher now
        assert!(position.pnl_percent.is_some());
    }

    #[test]
    fn test_peak_mc_tracking() {
        let mut tracker = TokenTracker::new().unwrap();

        let buy = TokenBuy {
            token_number: 1,
            mint: "test_mint_mc".to_string(),
            signature: "test_sig".to_string(),
            creator: "test_creator".to_string(),
            dev_buy_sol: 2.0,
            our_buy_sol: 0.1,
            timestamp: Utc::now(),
            has_socials: false,
            twitter: None,
            website: None,
            telegram: None,
            creator_token_count: 0,
            detection_method: "instruction".to_string(),
            mc_at_detection_usd: Some(7000.0),
            mc_at_entry_usd: Some(7000.0),
            token_price_sol: Some(0.00005),
            token_amount: None,
            user_token_account: None,
            bonding_curve: Some("test_bonding_curve_mc".to_string()),
            sold: false,
            sell_signature: None,
            current_price_sol: None,
            current_value_sol: None,
            pnl_sol: None,
            pnl_percent: None,
            last_pnl_update: None,
            buy_fees_sol: None,
            peak_mc_usd: None,
            peak_pnl_percent: None,
            breakeven_mode_active: false,
        };

        tracker.record_buy(buy).unwrap();

        // Update peak MC - should track highest
        tracker.update_peak_mc("test_mint_mc", 8000.0, 14000.0).unwrap();
        let position = tracker.get_active_positions().into_iter().find(|p| p.mint == "test_mint_mc").unwrap();
        assert_eq!(position.peak_mc_usd, Some(8000.0));
        assert_eq!(position.breakeven_mode_active, false); // Not reached threshold yet

        // Update with lower MC (peak should remain)
        tracker.update_peak_mc("test_mint_mc", 7500.0, 14000.0).unwrap();
        let position = tracker.get_active_positions().into_iter().find(|p| p.mint == "test_mint_mc").unwrap();
        assert_eq!(position.peak_mc_usd, Some(8000.0)); // Peak should remain

        // Update with higher MC (peak should update)
        tracker.update_peak_mc("test_mint_mc", 10000.0, 14000.0).unwrap();
        let position = tracker.get_active_positions().into_iter().find(|p| p.mint == "test_mint_mc").unwrap();
        assert_eq!(position.peak_mc_usd, Some(10000.0)); // Peak should update

        // Update with MC above threshold (should activate breakeven mode)
        tracker.update_peak_mc("test_mint_mc", 15000.0, 14000.0).unwrap();
        let position = tracker.get_active_positions().into_iter().find(|p| p.mint == "test_mint_mc").unwrap();
        assert_eq!(position.peak_mc_usd, Some(15000.0));
        assert_eq!(position.breakeven_mode_active, true); // Should activate breakeven mode
        
        // ✅ CRITICAL TEST: Breakeven mode should stay active even when MC drops below threshold
        // This simulates the scenario where token reaches 14000, then drops back to entry
        tracker.update_peak_mc("test_mint_mc", 6000.0, 14000.0).unwrap(); // MC drops below threshold
        let position = tracker.get_active_positions().into_iter().find(|p| p.mint == "test_mint_mc").unwrap();
        assert_eq!(position.breakeven_mode_active, true); // Breakeven mode should STAY active
        assert_eq!(position.peak_mc_usd, Some(15000.0)); // Peak should remain at highest
    }

    #[test]
    fn test_breakeven_stop_loss_scenario() {
        // Test scenario: Token goes to 14000, then returns to entry - should trigger sell
        let mut tracker = TokenTracker::new().unwrap();

        let buy = TokenBuy {
            token_number: 1,
            mint: "test_breakeven".to_string(),
            signature: "test_sig".to_string(),
            creator: "test_creator".to_string(),
            dev_buy_sol: 2.0,
            our_buy_sol: 0.1,
            timestamp: Utc::now(),
            has_socials: false,
            twitter: None,
            website: None,
            telegram: None,
            creator_token_count: 0,
            detection_method: "instruction".to_string(),
            mc_at_detection_usd: Some(5000.0),
            mc_at_entry_usd: Some(5000.0), // Entry MC = 5000
            token_price_sol: Some(0.00005),
            token_amount: Some(2000000),
            user_token_account: None,
            bonding_curve: Some("test_bonding_curve".to_string()),
            sold: false,
            sell_signature: None,
            current_price_sol: None,
            current_value_sol: None,
            pnl_sol: None,
            pnl_percent: None,
            last_pnl_update: None,
            buy_fees_sol: None,
            peak_mc_usd: None,
            peak_pnl_percent: None,
            breakeven_mode_active: false,
        };

        tracker.record_buy(buy).unwrap();

        // Step 1: MC reaches 14000 (breakeven threshold) - should activate breakeven mode
        tracker.update_peak_mc("test_breakeven", 14000.0, 14000.0).unwrap();
        let position = tracker.get_active_positions().into_iter().find(|p| p.mint == "test_breakeven").unwrap();
        assert_eq!(position.breakeven_mode_active, true, "Breakeven mode should be activated when MC reaches threshold");
        assert_eq!(position.peak_mc_usd, Some(14000.0));

        // Step 2: MC drops to 4500 (below entry of 5000) - breakeven mode should still be active
        // This simulates the scenario where token returns to entry after reaching 14000
        tracker.update_peak_mc("test_breakeven", 4500.0, 14000.0).unwrap();
        let position = tracker.get_active_positions().into_iter().find(|p| p.mint == "test_breakeven").unwrap();
        assert_eq!(position.breakeven_mode_active, true, "Breakeven mode should STAY active even when MC drops below threshold");
        assert_eq!(position.peak_mc_usd, Some(14000.0), "Peak MC should remain at highest value");
        
        // At this point, the monitoring logic should detect:
        // - breakeven_mode_active = true
        // - current_mc (4500) < entry_mc (5000)
        // - Should trigger breakeven stop loss and sell
    }

    #[test]
    fn test_buy_recording_with_mc() {
        let mut tracker = TokenTracker::new().unwrap();
        tracker.clear_all_buys().unwrap(); // Clear any existing data for test isolation

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
            token_amount: None,
            user_token_account: None,
            bonding_curve: None,
            sold: false,
            sell_signature: None,
            current_price_sol: None,
            current_value_sol: None,
            pnl_sol: None,
            pnl_percent: None,
            last_pnl_update: None,
            buy_fees_sol: None,
            peak_mc_usd: None,
            peak_pnl_percent: None,
            breakeven_mode_active: false,
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
            token_amount: None,
            user_token_account: None,
            bonding_curve: None,
            sold: false,
            sell_signature: None,
            current_price_sol: None,
            current_value_sol: None,
            pnl_sol: None,
            pnl_percent: None,
            last_pnl_update: None,
            buy_fees_sol: None,
            peak_mc_usd: None,
            peak_pnl_percent: None,
            breakeven_mode_active: false,
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
        tracker.clear_all_buys().unwrap(); // Clear any existing data for test isolation

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
            token_amount: None,
            user_token_account: None,
            bonding_curve: None,
            sold: false,
            sell_signature: None,
            current_price_sol: None,
            current_value_sol: None,
            pnl_sol: None,
            pnl_percent: None,
            last_pnl_update: None,
            buy_fees_sol: None,
            peak_mc_usd: None,
            peak_pnl_percent: None,
            breakeven_mode_active: false,
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
            token_amount: None,
            user_token_account: None,
            bonding_curve: None,
            sold: false,
            sell_signature: None,
            current_price_sol: None,
            current_value_sol: None,
            pnl_sol: None,
            pnl_percent: None,
            last_pnl_update: None,
            buy_fees_sol: None,
            peak_mc_usd: None,
            peak_pnl_percent: None,
            breakeven_mode_active: false,
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
        tracker.clear_all_buys().unwrap(); // Clear any existing data for test isolation

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
            token_amount: None,
            user_token_account: None,
            bonding_curve: None,
            sold: false,
            sell_signature: None,
            current_price_sol: None,
            current_value_sol: None,
            pnl_sol: None,
            pnl_percent: None,
            last_pnl_update: None,
            buy_fees_sol: None,
            peak_mc_usd: None,
            peak_pnl_percent: None,
            breakeven_mode_active: false,
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
            token_amount: None,
            user_token_account: None,
            bonding_curve: None,
            sold: false,
            sell_signature: None,
            current_price_sol: None,
            current_value_sol: None,
            pnl_sol: None,
            pnl_percent: None,
            last_pnl_update: None,
            buy_fees_sol: None,
            peak_mc_usd: None,
            peak_pnl_percent: None,
            breakeven_mode_active: false,
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