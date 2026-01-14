// tracker.rs - TOKEN BUY TRACKING & STATISTICS (WITH MC TRACKING)
#![allow(dead_code)]

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Write};
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
    pub discord: Option<String>,
    pub twitter_type: Option<String>, // "account", "community", "status", or None
    pub creator_token_count: u32,
    pub detection_method: String, // "instruction" or "balance_fallback"
    pub socials_source: Option<String>, // "RPC", "IPFS", "RPC+IPFS", or "None" - where socials data was fetched from

    // 🆕 NEW: Market cap tracking - PRE and POST buy
    pub mc_at_detection_sol: Option<f64>,  // MC when first detected (in SOL)
    pub mc_at_entry_sol: Option<f64>,      // MC after TX confirmed (real entry, in SOL)
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
    
    // 🆕 NEW: Peak tracking
    #[serde(default)]
    pub peak_mc_sol: Option<f64>,            // Highest MC reached (in SOL)
    #[serde(default)]
    pub peak_pnl_percent: Option<f64>,       // Best PnL percentage reached
    
    // 🆕 NEW: Partial sell tracking for dynamic sell strategy
    #[serde(default)]
    pub executed_sell_rules: Vec<String>,    // IDs of executed sell rules
    #[serde(default)]
    pub partial_sell_count: u32,             // Number of partial sells executed
    #[serde(default)]
    pub total_sold_percent: f64,             // Cumulative sold percentage (0-100)
    
    // 🆕 NEW: USD equivalents (calculated at save time using current SOL price)
    #[serde(default)]
    pub dev_buy_usd: Option<f64>,             // Dev buy in USD
    #[serde(default)]
    pub our_buy_usd: Option<f64>,            // Our buy in USD
    #[serde(default)]
    pub pnl_usd: Option<f64>,                // PnL in USD
    #[serde(default)]
    pub mc_at_detection_usd: Option<f64>,    // MC at detection in USD
    #[serde(default)]
    pub mc_at_entry_usd: Option<f64>,        // MC at entry in USD
    #[serde(default)]
    pub current_value_usd: Option<f64>,       // Current position value in USD
    
    // 🆕 NEW: Sell failure tracking
    #[serde(default)]
    pub sell_failure_reason: Option<String>,   // Reason why sell failed (e.g., "slippage_too_high", "bonding_curve_not_found")
    #[serde(default)]
    pub sell_failure_timestamp: Option<DateTime<Utc>>, // When the sell failure occurred
    
    // 🆕 NEW: Sell reason tracking
    #[serde(default)]
    pub sell_reason: Option<String>,           // Reason why position was sold (e.g., "stop_loss", "take_profit", "manual_sell", "strategy_xxx")
    #[serde(default)]
    pub sell_timestamp: Option<DateTime<Utc>>, // When the position was sold
    
    // 🆕 NEW: Tracking error tracking
    #[serde(default)]
    pub tracking_error_count: u32,             // Number of tracking errors encountered
    #[serde(default)]
    pub last_tracking_error: Option<DateTime<Utc>>, // Timestamp of last tracking error
    #[serde(default)]
    pub last_successful_tracking: Option<DateTime<Utc>>, // Timestamp of last successful tracking update
    #[serde(default)]
    pub suspicious_price_detected: bool,       // Flag if suspicious price was detected (price ratio >10x or <0.1x)
    #[serde(default)]
    pub bonding_curve_mismatch_detected: bool, // Flag if bonding curve mismatch was detected
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackerStats {
    pub total_buys: u32,
    pub total_sol_spent: f64,
    pub session_start: DateTime<Utc>,
    pub last_buy: DateTime<Utc>,
    pub buys: Vec<TokenBuy>,

    // 🆕 NEW: MC statistics (in SOL)
    pub avg_mc_sol: f64,
    pub min_mc_sol: f64,
    pub max_mc_sol: f64,
    
    // 🆕 NEW: USD equivalents (calculated at save time using current SOL price)
    #[serde(default)]
    pub total_sol_spent_usd: Option<f64>,     // Total SOL spent in USD
}

pub struct TokenTracker {
    stats: TrackerStats,
    csv_path: String,
    json_path: String,
}

impl TokenTracker {
    /// Get the directory where tracker files should be stored
    /// On macOS, when running from GUI, current_dir() can be root or system directory
    /// So we use executable directory instead (where the binary is located)
    #[cfg(not(test))]
    fn get_tracker_dir() -> std::path::PathBuf {
        // Use executable directory (where the binary is located)
        // This works reliably on macOS even when launched from GUI
        if let Ok(exe_path) = std::env::current_exe() {
            if let Some(exe_dir) = exe_path.parent() {
                return exe_dir.to_path_buf();
            }
        }

        // Fallback: current directory (should not happen, but safe fallback)
        std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."))
    }

    #[cfg(test)]
    fn get_tracker_dir() -> std::path::PathBuf {
        // In tests, never write into `target/` (shared + parallel tests).
        // Use a per-process temp directory to avoid file-name collisions and flaky tests.
        let mut dir = std::env::temp_dir();
        dir.push("sniper_tracker_tests");
        dir.push(format!("pid_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        dir
    }

    /// Create new tracker or load from latest existing JSON file
    pub fn new() -> Result<Self> {
        // Try to find and load the latest JSON file first (runtime only).
        // In tests we always create a fresh tracker to avoid cross-test state.
        #[cfg(not(test))]
        {
            if let Ok(tracker) = Self::load_from_latest_json() {
                return Ok(tracker);
            }
        }
        
        // If no existing JSON found, create new tracker
        let session_start = Utc::now();

        // Test-only: include seconds + subsec to avoid file-name collisions under parallel tests.
        #[cfg(test)]
        let timestamp = session_start.format("%Y-%m-%d_%H-%M-%S_%f");
        #[cfg(not(test))]
        let timestamp = session_start.format("%Y-%m-%d_%H-%M");

        let csv_filename = format!("tracker_{}.csv", timestamp);
        let json_filename = format!("tracker_{}.json", timestamp);

        // Use absolute paths in tracker directory (executable directory on macOS)
        let tracker_dir = Self::get_tracker_dir();
        let csv_path = tracker_dir.join(&csv_filename);
        let json_path = tracker_dir.join(&json_filename);

        // Convert to strings for storage
        let csv_path_str = csv_path.to_str()
            .ok_or_else(|| anyhow::anyhow!("Invalid CSV path"))?
            .to_string();
        let json_path_str = json_path.to_str()
            .ok_or_else(|| anyhow::anyhow!("Invalid JSON path"))?
            .to_string();

        eprintln!("📁 DEBUG: Tracker directory: {:?}", tracker_dir);
        eprintln!("📁 DEBUG: CSV file: {:?}", csv_path);
        eprintln!("📁 DEBUG: JSON file: {:?}", json_path);

        // Create CSV header if file doesn't exist
        if !csv_path.exists() {
            let mut file = File::create(&csv_path)?;
            writeln!(file, "Token#,Mint,Signature,Creator,DevBuy(SOL),OurBuy(SOL),Timestamp,HasSocials,Twitter,TwitterType,Website,Telegram,Discord,CreatorTokens,DetectionMethod,SocialsSource,MC_Detection_USD,MC_Entry_USD,TokenPrice_SOL")?;
        }

        Ok(Self {
            stats: TrackerStats {
                total_buys: 0,
                total_sol_spent: 0.0,
                session_start,
                last_buy: session_start,
                buys: Vec::new(),
                avg_mc_sol: 0.0,
                min_mc_sol: f64::MAX,
                max_mc_sol: 0.0,
                total_sol_spent_usd: None,
            },
            csv_path: csv_path_str,
            json_path: json_path_str,
        })
    }
    
    /// Load tracker from the latest JSON file
    fn load_from_latest_json() -> Result<Self> {
        use std::fs;
        
        // Find all JSON files matching the pattern
        let tracker_dir = Self::get_tracker_dir();
        eprintln!("📁 DEBUG load_from_latest_json: Tracker directory: {:?}", tracker_dir);
        
        let json_files: Vec<_> = fs::read_dir(&tracker_dir)?
            .filter_map(|entry| entry.ok())
            .filter_map(|entry| {
                let path = entry.path();
                if path.is_file() {
                    if let Some(file_name) = path.file_name().and_then(|n| n.to_str()) {
                        // Support both new format (tracker_) and old format (sniper_session_) for compatibility
                        if (file_name.starts_with("tracker_") || file_name.starts_with("sniper_session_")) 
                            && file_name.ends_with(".json") {
                            return Some((path.clone(), entry.metadata().ok()?.modified().ok()?));
                        }
                    }
                }
                None
            })
            .collect();
        
        if json_files.is_empty() {
            return Err(anyhow::anyhow!("No existing JSON files found"));
        }
        
        // Get the latest file
        let (latest_json_path, _) = json_files.iter()
            .max_by_key(|(_, modified)| modified)
            .ok_or_else(|| anyhow::anyhow!("Failed to find latest JSON file"))?;
        
        // Read and deserialize JSON
        let content = fs::read_to_string(latest_json_path)?;
        let stats: TrackerStats = serde_json::from_str(&content)
            .map_err(|e| anyhow::anyhow!("Failed to parse JSON: {}", e))?;
        
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
        if let Some(mc_sol) = buy.mc_at_entry_sol {
            // Safety check for NaN and infinite values
            if mc_sol.is_finite() && !mc_sol.is_nan() {
                if self.stats.min_mc_sol == f64::MAX || mc_sol < self.stats.min_mc_sol {
                    self.stats.min_mc_sol = mc_sol;
                }
                if mc_sol > self.stats.max_mc_sol {
                    self.stats.max_mc_sol = mc_sol;
                }

                // Recalculate average with safety checks
                let total_mc: f64 = self.stats.buys.iter()
                    .filter_map(|b| b.mc_at_entry_sol)
                    .filter(|&v| v.is_finite() && !v.is_nan())
                    .sum::<f64>() + mc_sol;
                let count_with_mc = self.stats.buys.iter()
                    .filter(|b| b.mc_at_entry_sol.is_some())
                    .filter(|b| {
                        if let Some(v) = b.mc_at_entry_sol {
                            v.is_finite() && !v.is_nan()
                        } else {
                            false
                        }
                    })
                    .count() + 1;
                
                if count_with_mc > 0 {
                    self.stats.avg_mc_sol = total_mc / count_with_mc as f64;
                    // Ensure average is also finite
                    if !self.stats.avg_mc_sol.is_finite() || self.stats.avg_mc_sol.is_nan() {
                        self.stats.avg_mc_sol = 0.0;
                    }
                }
            }
        }

        // Append to CSV
        self.append_to_csv(&buy)?;

        // Add to memory
        self.stats.buys.push(buy);

        // Save JSON
        if let Err(e) = self.save_json() {
            eprintln!("❌ Failed to save JSON tracker: {}", e);
            eprintln!("   JSON path: {}", self.json_path);
            // Don't fail the whole operation if JSON save fails, but log it clearly
        } else {
            eprintln!("✅ JSON tracker saved: {}", self.json_path);
        }

        // Print periodic stats (every 10 buys)
        self.print_periodic_stats();

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
        
        let mc_detection_str = buy.mc_at_detection_sol
            .filter(|&v| v.is_finite() && !v.is_nan())
            .map(|v| format!("{:.2}", v))
            .unwrap_or_default();
        let mc_entry_str = buy.mc_at_entry_sol
            .filter(|&v| v.is_finite() && !v.is_nan())
            .map(|v| format!("{:.2}", v))
            .unwrap_or_default();
        let token_price_str = buy.token_price_sol
            .filter(|&v| v.is_finite() && !v.is_nan())
            .map(|v| format!("{:.8}", v))
            .unwrap_or_default();
        
        // Escape CSV special characters in strings
        let escape_csv = |s: &str| s.replace(",", " ").replace("\n", " ").replace("\r", " ");
        
        writeln!(
            writer,
            "{},{},{},{},{:.4},{:.4},{},{},{},{},{},{},{},{},{},{},{},{},{}",
            buy.token_number,
            escape_csv(&buy.mint),
            escape_csv(&buy.signature),
            escape_csv(&buy.creator),
            dev_buy_sol_safe,
            our_buy_sol_safe,
            buy.timestamp.to_rfc3339(),
            buy.has_socials,
            buy.twitter.as_deref().map(escape_csv).unwrap_or_default(),
            buy.twitter_type.as_deref().map(escape_csv).unwrap_or_default(),
            buy.website.as_deref().map(escape_csv).unwrap_or_default(),
            buy.telegram.as_deref().map(escape_csv).unwrap_or_default(),
            buy.discord.as_deref().map(escape_csv).unwrap_or_default(),
            buy.creator_token_count,
            escape_csv(&buy.detection_method),
            buy.socials_source.as_deref().map(escape_csv).unwrap_or_default(),
            mc_detection_str,
            mc_entry_str,
            token_price_str
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
        
        // Get current SOL price for USD calculations
        use crate::utils::sol_to_usd;
        
        // Populate USD values for all buys
        for buy in &mut sanitized_stats.buys {
            // Calculate USD equivalents
            buy.dev_buy_usd = if buy.dev_buy_sol.is_finite() && !buy.dev_buy_sol.is_nan() {
                Some(sol_to_usd(buy.dev_buy_sol))
            } else {
                Some(0.0)
            };
            
            buy.our_buy_usd = if buy.our_buy_sol.is_finite() && !buy.our_buy_sol.is_nan() {
                Some(sol_to_usd(buy.our_buy_sol))
            } else {
                Some(0.0)
            };
            
            buy.pnl_usd = if let Some(pnl) = buy.pnl_sol {
                if pnl.is_finite() && !pnl.is_nan() {
                    Some(sol_to_usd(pnl))
                } else {
                    Some(0.0)
                }
            } else {
                None
            };
            
            buy.mc_at_detection_usd = buy.mc_at_detection_sol.map(|mc| {
                if mc.is_finite() && !mc.is_nan() {
                    sol_to_usd(mc)
                } else {
                    0.0
                }
            });
            
            buy.mc_at_entry_usd = buy.mc_at_entry_sol.map(|mc| {
                if mc.is_finite() && !mc.is_nan() {
                    sol_to_usd(mc)
                } else {
                    0.0
                }
            });
            
            buy.current_value_usd = buy.current_value_sol.map(|val| {
                if val.is_finite() && !val.is_nan() {
                    sol_to_usd(val)
                } else {
                    0.0
                }
            });
        }
        
        // Populate total_sol_spent_usd
        sanitized_stats.total_sol_spent_usd = if sanitized_stats.total_sol_spent.is_finite() && !sanitized_stats.total_sol_spent.is_nan() {
            Some(sol_to_usd(sanitized_stats.total_sol_spent))
        } else {
            Some(0.0)
        };
        
        // Sanitize all f64 values in stats
        if !sanitized_stats.total_sol_spent.is_finite() || sanitized_stats.total_sol_spent.is_nan() {
            sanitized_stats.total_sol_spent = 0.0;
        }
        if !sanitized_stats.avg_mc_sol.is_finite() || sanitized_stats.avg_mc_sol.is_nan() {
            sanitized_stats.avg_mc_sol = 0.0;
        }
        if !sanitized_stats.min_mc_sol.is_finite() || sanitized_stats.min_mc_sol.is_nan() || sanitized_stats.min_mc_sol == f64::MAX {
            sanitized_stats.min_mc_sol = 0.0;
        }
        if !sanitized_stats.max_mc_sol.is_finite() || sanitized_stats.max_mc_sol.is_nan() {
            sanitized_stats.max_mc_sol = 0.0;
        }
        
        // Sanitize all buys
        for buy in &mut sanitized_stats.buys {
            if !buy.dev_buy_sol.is_finite() || buy.dev_buy_sol.is_nan() {
                buy.dev_buy_sol = 0.0;
            }
            if !buy.our_buy_sol.is_finite() || buy.our_buy_sol.is_nan() {
                buy.our_buy_sol = 0.0;
            }
            if let Some(mc) = &mut buy.mc_at_detection_sol {
                if !mc.is_finite() || mc.is_nan() {
                    *mc = 0.0;
                }
            }
            if let Some(mc) = &mut buy.mc_at_entry_sol {
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
        
        // Get absolute path for display
        let absolute_path = std::fs::canonicalize(&self.json_path)
            .unwrap_or_else(|_| std::path::PathBuf::from(&self.json_path));
        
        eprintln!("💾 Attempting to save JSON tracker to: {}", self.json_path);
        
        // Show absolute path for debugging
        if let Ok(absolute_path) = std::fs::canonicalize(&self.json_path) {
            eprintln!("📁 DEBUG save_json: Absolute path: {:?}", absolute_path);
        } else {
            eprintln!("📁 DEBUG save_json: Path: {}", self.json_path);
        }
        
        let file = File::create(&self.json_path)
            .map_err(|e| {
                eprintln!("❌ ERROR creating JSON file {}: {}", self.json_path, e);
                anyhow::anyhow!("Failed to create JSON file {}: {}", self.json_path, e)
            })?;
        let mut writer = BufWriter::new(file);
        serde_json::to_writer_pretty(&mut writer, &sanitized_stats)
            .map_err(|e| {
                eprintln!("❌ ERROR serializing JSON: {}", e);
                anyhow::anyhow!("Failed to serialize JSON: {}", e)
            })?;
        writer.flush()
            .map_err(|e| {
                eprintln!("❌ ERROR flushing JSON file: {}", e);
                anyhow::anyhow!("Failed to flush JSON file: {}", e)
            })?;
        
        // Verify file was created and show size
        if let Ok(metadata) = std::fs::metadata(&self.json_path) {
            eprintln!("✅ DEBUG save_json: File created successfully, size: {} bytes", metadata.len());
        } else {
            eprintln!("⚠️  DEBUG save_json: Warning: Could not verify file creation");
        }
        
        // Print JSON tracker to console (only if there are buys to avoid empty output)
        // Use eprintln! instead of println! so it shows in console even in GUI mode
        if !sanitized_stats.buys.is_empty() {
            let json_string = serde_json::to_string_pretty(&sanitized_stats)
                .map_err(|e| anyhow::anyhow!("Failed to serialize JSON for printing: {}", e))?;
            eprintln!("\n📊 JSON TRACKER saved to: {}", absolute_path.display());
            eprintln!("📊 JSON TRACKER:\n{}", json_string);
        } else {
            // Still show path even if empty
            eprintln!("\n📊 JSON TRACKER saved to: {}", absolute_path.display());
        }
        
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
        if self.stats.max_mc_sol > 0.0 {
            use crate::utils::sol_to_usd;
            println!("║                                                      ║");
            println!("║ 📊 MARKET CAP STATISTICS:                           ║");
            println!("║   Average MC:      {:>10.2} SOL (${:.0})           ║", self.stats.avg_mc_sol, sol_to_usd(self.stats.avg_mc_sol));
            println!("║   Min MC:          {:>10.2} SOL (${:.0})           ║", self.stats.min_mc_sol, sol_to_usd(self.stats.min_mc_sol));
            println!("║   Max MC:          {:>10.2} SOL (${:.0})           ║", self.stats.max_mc_sol, sol_to_usd(self.stats.max_mc_sol));
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

            if self.stats.avg_mc_sol > 0.0 {
                use crate::utils::sol_to_usd;
                msg.push_str(&format!(" | Avg MC: {:.2} SOL (${:.0})", self.stats.avg_mc_sol, sol_to_usd(self.stats.avg_mc_sol)));
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
        self.stats.avg_mc_sol = 0.0;
        self.stats.min_mc_sol = f64::MAX;
        self.stats.max_mc_sol = 0.0;
        self.stats.last_buy = self.stats.session_start;
        
        // Save empty state
        self.save_json()?;
        
        // Clear CSV file
        if let Ok(mut file) = std::fs::File::create(&self.csv_path) {
            use std::io::Write;
            let _ = writeln!(file, "Token#,Mint,Signature,Creator,DevBuy(SOL),OurBuy(SOL),Timestamp,HasSocials,Twitter,TwitterType,Website,Telegram,Discord,CreatorTokens,DetectionMethod,SocialsSource,MC_Detection_USD,MC_Entry_USD,TokenPrice_SOL");
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
            .filter_map(|b| b.mc_at_entry_sol)
            .collect();
        if !mc_values.is_empty() {
            self.stats.avg_mc_sol = mc_values.iter().sum::<f64>() / mc_values.len() as f64;
            self.stats.min_mc_sol = mc_values.iter().cloned().fold(f64::MAX, f64::min);
            self.stats.max_mc_sol = mc_values.iter().cloned().fold(0.0, f64::max);
        } else {
            self.stats.avg_mc_sol = 0.0;
            self.stats.min_mc_sol = f64::MAX;
            self.stats.max_mc_sol = 0.0;
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
            buy.sell_reason = Some("auto_cleanup".to_string());
            buy.sell_timestamp = Some(Utc::now());
            self.save_json()?;
            Ok(())
        } else {
            Err(anyhow::anyhow!("Position not found or already sold: {}", mint))
        }
    }

    /// Mark a position as sold
    pub fn mark_as_sold(&mut self, mint: &str, sell_signature: String, sell_reason: Option<String>) -> Result<()> {
        if let Some(buy) = self.stats.buys.iter_mut().find(|b| b.mint == mint && !b.sold) {
            buy.sold = true;
            buy.sell_signature = Some(sell_signature);
            buy.sell_reason = sell_reason.clone();
            buy.sell_timestamp = Some(Utc::now());
            
            // If sell_reason starts with "strategy_", extract rule ID and add to executed_sell_rules
            if let Some(ref reason) = sell_reason {
                if reason.starts_with("strategy_") {
                    let rule_id = reason.strip_prefix("strategy_").unwrap_or(reason);
                    if !buy.executed_sell_rules.contains(&rule_id.to_string()) {
                        buy.executed_sell_rules.push(rule_id.to_string());
                    }
                }
            }
            
            // Clear sell failure reason when successfully sold
            buy.sell_failure_reason = None;
            buy.sell_failure_timestamp = None;
            // Save JSON after update
            if let Err(e) = self.save_json() {
                eprintln!("❌ Failed to save JSON tracker after marking as sold: {}", e);
                eprintln!("   JSON path: {}", self.json_path);
                return Err(e);
            } else {
                eprintln!("✅ JSON tracker saved after marking as sold: {}", self.json_path);
            }
            Ok(())
        } else {
            Err(anyhow::anyhow!("Position not found or already sold: {}", mint))
        }
    }

    /// Record sell failure reason for a position
    pub fn record_sell_failure(&mut self, mint: &str, reason: String) -> Result<()> {
        if let Some(buy) = self.stats.buys.iter_mut().find(|b| b.mint == mint && !b.sold) {
            buy.sell_failure_reason = Some(reason.clone());
            buy.sell_failure_timestamp = Some(Utc::now());
            // Save JSON after update
            if let Err(e) = self.save_json() {
                eprintln!("❌ Failed to save JSON tracker after recording sell failure: {}", e);
                eprintln!("   JSON path: {}", self.json_path);
                return Err(e);
            } else {
                eprintln!("⚠️  Sell failure recorded for {}: {}", mint, reason);
            }
            Ok(())
        } else {
            Err(anyhow::anyhow!("Position not found or already sold: {}", mint))
        }
    }

    /// Mark that a sell rule has been executed (for partial sells)
    pub fn mark_rule_executed(&mut self, mint: &str, rule_id: &str) -> Result<()> {
        if let Some(buy) = self.stats.buys.iter_mut().find(|b| b.mint == mint && !b.sold) {
            if !buy.executed_sell_rules.contains(&rule_id.to_string()) {
                buy.executed_sell_rules.push(rule_id.to_string());
                self.save_json()?;
            }
            Ok(())
        } else {
            Err(anyhow::anyhow!("Position not found or already sold: {}", mint))
        }
    }

    /// Check if a rule has been executed for a position
    pub fn is_rule_executed(&self, mint: &str, rule_id: &str) -> bool {
        if let Some(buy) = self.stats.buys.iter().find(|b| b.mint == mint && !b.sold) {
            buy.executed_sell_rules.contains(&rule_id.to_string())
        } else {
            false
        }
    }

    /// Get executed rules for a position
    pub fn get_executed_rules(&self, mint: &str) -> Vec<String> {
        if let Some(buy) = self.stats.buys.iter().find(|b| b.mint == mint && !b.sold) {
            buy.executed_sell_rules.clone()
        } else {
            Vec::new()
        }
    }

    /// Record a partial sell
    pub fn mark_partial_sell(&mut self, mint: &str, rule_id: &str, sell_percent: f64) -> Result<()> {
        if let Some(buy) = self.stats.buys.iter_mut().find(|b| b.mint == mint && !b.sold) {
            if !buy.executed_sell_rules.contains(&rule_id.to_string()) {
                buy.executed_sell_rules.push(rule_id.to_string());
            }
            buy.partial_sell_count += 1;
            buy.total_sold_percent = (buy.total_sold_percent + sell_percent).min(100.0);
            
            // If we've sold 100%, mark as fully sold
            if buy.total_sold_percent >= 100.0 {
                buy.sold = true;
            }
            
            self.save_json()?;
            Ok(())
        } else {
            Err(anyhow::anyhow!("Position not found or already sold: {}", mint))
        }
    }

    /// Get remaining position percentage
    pub fn get_remaining_percent(&self, mint: &str) -> f64 {
        if let Some(buy) = self.stats.buys.iter().find(|b| b.mint == mint && !b.sold) {
            (100.0 - buy.total_sold_percent).max(0.0)
        } else {
            0.0
        }
    }

    /// Update token_amount for a position (from wallet balance check)
    pub fn update_token_amount(&mut self, mint: &str, token_amount: u64) -> Result<()> {
        if let Some(buy) = self.stats.buys.iter_mut().find(|b| b.mint == mint && !b.sold) {
            buy.token_amount = Some(token_amount);
            // Save JSON after update (important for partial sells and manual operations)
            if let Err(e) = self.save_json() {
                eprintln!("❌ Failed to save JSON tracker after updating token amount: {}", e);
                eprintln!("   JSON path: {}", self.json_path);
                return Err(e);
            } else {
                eprintln!("✅ JSON tracker saved after updating token amount: {}", self.json_path);
            }
            Ok(())
        } else {
            Err(anyhow::anyhow!("Position not found or already sold: {}", mint))
        }
    }

    /// Update position PnL with current price (ultra-fast, no disk write)
    pub fn update_position_pnl_fast(&mut self, mint: &str, current_price_sol: f64) -> Result<()> {
        
        // Debug: Check if position exists
        let position_exists = self.stats.buys.iter().any(|b| b.mint == mint && !b.sold);
        if !position_exists {
            return Err(anyhow::anyhow!("Position not found or already sold: {}", mint));
        }
        
        if let Some(buy) = self.stats.buys.iter_mut().find(|b| b.mint == mint && !b.sold) {
            buy.current_price_sol = Some(current_price_sol);
            buy.last_pnl_update = Some(Utc::now());
            
            // Skip PnL calculation if our_buy_sol is 0 or invalid
            if buy.our_buy_sol <= 0.0 {
                return Ok(());
            }
            
            // ✅ FIX: Calculate PnL even if current_price_sol is not available (token migrated)
            // If current_price_sol is 0 or very small, it means bonding curve doesn't exist
            // In that case, we can't calculate live PnL, but we can at least show entry price
            let is_token_migrated = current_price_sol <= 0.0 || current_price_sol < 1e-15;
            
            if is_token_migrated {
                // Token is migrated - we can't get live price, but we can still show entry info
                // Set a placeholder PnL of 0 or calculate based on entry price if we have token_amount
                if let Some(_token_amount) = buy.token_amount {
                    if let Some(_entry_price) = buy.token_price_sol {
                        // For migrated tokens, we can't know current value, so set PnL to 0
                        buy.pnl_sol = Some(0.0); // Placeholder - token migrated
                        buy.pnl_percent = Some(0.0);
                    }
                }
                return Ok(());
            }
            
            // ✅ FIX: Try to get or calculate token_amount
            // First, use stored token_amount if available
            // Otherwise, try to calculate it from our_buy_sol and token_price_sol
            // ✅ FIX: Don't use current_price_sol as fallback - entry price should only come from actual buy data
            let token_amount = if let Some(amount) = buy.token_amount {
                Some(amount)
            } else {
                // Try to calculate token_amount from our_buy_sol and token_price_sol
                // Only use token_price_sol (entry price from actual buy), not current_price_sol
                if let Some(entry_price) = buy.token_price_sol {
                    if entry_price > 0.0 && buy.our_buy_sol > 0.0 {
                        // tokens = SOL invested / entry price per token
                        // Then convert to raw units (multiply by 1e6 for 6 decimals)
                        let tokens_human = buy.our_buy_sol / entry_price;
                        let tokens_raw = (tokens_human * 1e6) as u64;
                        if tokens_raw > 0 {
                            // ✅ FIX: Save immediately to prevent fallback on next call
                            buy.token_amount = Some(tokens_raw);
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
                // ✅ FIX: Don't log every 20ms - this function is called 50x/sec by monitor_pnl_ultra_fast
                // Only log on first calculation or significant changes
                // eprintln!("✅ PnL UPDATE: Calculating PnL for {} with token_amount={}, our_buy_sol={:.6}", 
                //          &mint[..8], token_amount, buy.our_buy_sol);
                // ✅ FIX: Calculate PnL even if token_price_sol is not set
                // We can calculate PnL using our_buy_sol + fees as cost basis
                // token_price_sol is only needed for display, not for PnL calculation
                
                // Only calculate entry price if it's not set (immutable after first calculation)
                // Entry price should never be modified once set
                if buy.token_price_sol.is_none() {
                    let tokens_actual_for_price = token_amount as f64 / 1e6; // Convert 6-decimal raw to actual tokens
                    if tokens_actual_for_price > 0.0 && buy.our_buy_sol > 0.0 {
                        // Entry price = SOL invested / tokens received
                        let calculated_price = buy.our_buy_sol / tokens_actual_for_price;
                        // Validate entry price is reasonable (not too small due to precision errors)
                        if calculated_price >= 1e-12 && calculated_price <= 1.0 {
                            buy.token_price_sol = Some(calculated_price);
                        }
                        // If calculated price is invalid, leave it as None (don't try to fix with current price)
                    }
                }
                
                // ✅ FIX: token_amount is already saved above when calculated, so no need to save again here
                
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
                
                // ✅ FIX: Calculate PnL percentage based on NET value (after all fees)
                // This shows the actual return including all fees, matching Axiom's calculation
                // Formula: PnL% = (pnl / cost_basis) * 100
                // where pnl = current_value_net - cost_basis (already calculated above)
                // This includes: 1% pump.fun buy fee, 1% pump.fun sell fee, and all network fees
                let pnl_percent = if cost_basis > 0.0 {
                    (pnl / cost_basis) * 100.0
                } else {
                    0.0
                };
                
                
                // Calculate PnL percentage - use NET for percentage (shows actual return after all fees)
                if cost_basis > 0.0 {
                    // ✅ FIX: Cap PnL% to reasonable range to prevent display of unrealistic values
                    // If entry_price is too small (near 0), PnL% can be astronomical
                    // Cap at ±10000% (100x) to prevent UI showing millions of percent
                    let capped_pnl_percent = if pnl_percent > 10000.0 {
                        // ✅ FIX: Rate limit warning messages - only show once every 5 minutes to reduce spam
                        static LAST_CAP_WARNING: std::sync::Mutex<Option<std::time::Instant>> = std::sync::Mutex::new(None);
                        static WARNING_COUNT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
                        
                        let mut last_warning = LAST_CAP_WARNING.lock().unwrap_or_else(|e| e.into_inner());
                        let now = std::time::Instant::now();
                        let _should_log = last_warning
                            .map(|last_time| now.duration_since(last_time).as_secs() >= 300) // 5 minutes
                            .unwrap_or(true);
                        
                        *last_warning = Some(now);
                        10000.0
                    } else if pnl_percent < -10000.0 {
                        // ✅ FIX: Rate limit warning messages for negative capping too
                        static LAST_CAP_WARNING_NEG: std::sync::Mutex<Option<std::time::Instant>> = std::sync::Mutex::new(None);
                        static WARNING_COUNT_NEG: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
                        
                        let mut last_warning = LAST_CAP_WARNING_NEG.lock().unwrap_or_else(|e| e.into_inner());
                        let now = std::time::Instant::now();
                        let _should_log = last_warning
                            .map(|last_time| now.duration_since(last_time).as_secs() >= 300) // 5 minutes
                            .unwrap_or(true);
                        
                        *last_warning = Some(now);
                        -10000.0
                    } else {
                        pnl_percent
                    };
                    
                    buy.pnl_percent = Some(capped_pnl_percent);
                    
                    // Update peak PnL if current is better (also capped)
                    let capped_peak = if pnl_percent > 10000.0 { 10000.0 } else { pnl_percent };
                    if capped_peak > buy.peak_pnl_percent.unwrap_or(f64::MIN) {
                        buy.peak_pnl_percent = Some(capped_peak);
                    }
                }
            } else {
                // ✅ FIX: Try one more fallback - calculate PnL using only price change if we have entry price
                // This is less accurate but better than showing nothing
                // ✅ FIX: Use same fee calculation formula as precise method for consistency
                if let Some(entry_price) = buy.token_price_sol {
                    if entry_price > 0.0 && current_price_sol > 0.0 && buy.our_buy_sol > 0.0 {
                        // Calculate approximate PnL based on price change only
                        // This assumes we have tokens proportional to our_buy_sol / entry_price
                        // ✅ FIX: Validate entry_price is reasonable before calculating ratio
                        if entry_price >= 1e-12 && entry_price <= 1.0 {
                            // Calculate approximate current value based on price change
                            // tokens_approx = our_buy_sol / entry_price (in human units)
                            let tokens_approx = buy.our_buy_sol / entry_price;
                            let current_value_gross_approx = tokens_approx * current_price_sol;
                            
                            // ✅ FIX: Use same fee calculation as precise method
                            let buy_fees = buy.buy_fees_sol.unwrap_or(0.000015);
                            let cost_basis = buy.our_buy_sol + buy_fees;
                            let estimated_sell_fees = 0.00001;
                            // Same formula: (Gross Value * 0.99) - Estimated Sell Fees
                            let current_value_net_approx = (current_value_gross_approx * 0.99) - estimated_sell_fees;
                            let net_pnl = current_value_net_approx - cost_basis;
                            
                            buy.pnl_sol = Some(net_pnl);
                            if cost_basis > 0.0 {
                                let pnl_pct = (net_pnl / cost_basis) * 100.0;
                                // Cap PnL% to reasonable range
                                let capped = if pnl_pct > 10000.0 { 10000.0 } else if pnl_pct < -10000.0 { -10000.0 } else { pnl_pct };
                                buy.pnl_percent = Some(capped);
                            }
                            
                        } else {
                        }
                    }
                }
                
            }
            
            Ok(())
        } else {
            eprintln!("❌ PnL UPDATE: Position not found in find() for mint {} (this shouldn't happen after existence check)", &mint[..8]);
            Err(anyhow::anyhow!("Position not found or already sold: {}", mint))
        }
    }

    /// Get all active positions with bonding curves (for batch PnL update)
    pub fn get_active_positions_for_pnl(&self) -> Vec<(String, String)> {
        let _all_buys = self.stats.buys.len();
        let _sold_count = self.stats.buys.iter().filter(|b| b.sold).count();
        let _without_bc = self.stats.buys.iter().filter(|b| !b.sold && b.bonding_curve.is_none()).count();
        
        let positions = self.stats.buys.iter()
            .filter(|buy| !buy.sold && buy.bonding_curve.is_some())
            .map(|buy| {
                (buy.mint.clone(), buy.bonding_curve.clone().unwrap())
            })
            .collect::<Vec<_>>();
        
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
            discord: None,
            twitter_type: None,
            creator_token_count: 0,
            detection_method: "instruction".to_string(),
            mc_at_detection_sol: Some(36.5),
            mc_at_entry_sol: Some(36.5),
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
            peak_mc_sol: None,
            peak_pnl_percent: None,
            executed_sell_rules: Vec::new(),
            partial_sell_count: 0,
            total_sold_percent: 0.0,
            dev_buy_usd: None,
            our_buy_usd: None,
            pnl_usd: None,
            mc_at_detection_usd: None,
            mc_at_entry_usd: None,
            current_value_usd: None,
            sell_failure_reason: None,
            sell_failure_timestamp: None,
            sell_reason: None,
            sell_timestamp: None,
            socials_source: None,
            tracking_error_count: 0,
            last_tracking_error: None,
            last_successful_tracking: None,
            suspicious_price_detected: false,
            bonding_curve_mismatch_detected: false,
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
            discord: None,
            twitter_type: Some("account".to_string()),
            creator_token_count: 1,
            detection_method: "instruction".to_string(),
            mc_at_detection_sol: Some(35.0),
            mc_at_entry_sol: Some(36.5),
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
            peak_mc_sol: None,
            peak_pnl_percent: None,
            executed_sell_rules: Vec::new(),
            partial_sell_count: 0,
            total_sold_percent: 0.0,
            dev_buy_usd: None,
            our_buy_usd: None,
            pnl_usd: None,
            mc_at_detection_usd: None,
            mc_at_entry_usd: None,
            current_value_usd: None,
            sell_failure_reason: None,
            sell_failure_timestamp: None,
            sell_reason: None,
            sell_timestamp: None,
            socials_source: None,
            tracking_error_count: 0,
            last_tracking_error: None,
            last_successful_tracking: None,
            suspicious_price_detected: false,
            bonding_curve_mismatch_detected: false,
        };

        assert!(tracker.record_buy(buy).is_ok());
        assert_eq!(tracker.total_buys(), 1);
        assert_eq!(tracker.get_stats().avg_mc_sol, 36.5);
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
                avg_mc_sol: 0.0,
                min_mc_sol: f64::MAX,
                max_mc_sol: 0.0,
                total_sol_spent_usd: None,
            },
            csv_path: csv_path.clone(),
            json_path: json_path.clone(),
        };

        // Create CSV header
        std::fs::write(&csv_path, "Token#,Mint,Signature,Creator,DevBuy(SOL),OurBuy(SOL),Timestamp,HasSocials,Twitter,TwitterType,Website,Telegram,Discord,CreatorTokens,DetectionMethod,SocialsSource,MC_Detection_USD,MC_Entry_USD,TokenPrice_SOL\n").unwrap();

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
            discord: None,
            twitter_type: Some("account".to_string()),
            creator_token_count: 1,
            detection_method: "instruction".to_string(),
            mc_at_detection_sol: Some(35.0),
            mc_at_entry_sol: Some(36.5),
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
            peak_mc_sol: None,
            peak_pnl_percent: None,
            executed_sell_rules: Vec::new(),
            partial_sell_count: 0,
            total_sold_percent: 0.0,
            dev_buy_usd: None,
            our_buy_usd: None,
            pnl_usd: None,
            mc_at_detection_usd: None,
            mc_at_entry_usd: None,
            current_value_usd: None,
            sell_failure_reason: None,
            sell_failure_timestamp: None,
            sell_reason: None,
            sell_timestamp: None,
            socials_source: None,
            tracking_error_count: 0,
            last_tracking_error: None,
            last_successful_tracking: None,
            suspicious_price_detected: false,
            bonding_curve_mismatch_detected: false,
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
                avg_mc_sol: 36.5,
                min_mc_sol: 35.0,
                max_mc_sol: 38.0,
                total_sol_spent_usd: None,
            },
            csv_path,
            json_path: json_path.clone(),
        };

        assert!(tracker.save_json().is_ok());

        // Verify JSON content
        let content = std::fs::read_to_string(&json_path).unwrap();
        assert!(content.contains("\"total_buys\":2") || content.contains("\"total_buys\": 2"));
        assert!(content.contains("\"total_sol_spent\":0.2") || content.contains("\"total_sol_spent\": 0.2"));
        assert!(content.contains("\"avg_mc_sol\":36.5") || content.contains("\"avg_mc_sol\": 36.5"));
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
            discord: None,
            twitter_type: None,
            creator_token_count: 1,
            detection_method: "instruction".to_string(),
            mc_at_detection_sol: Some(29.0),
            mc_at_entry_sol: Some(33.0),
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
            peak_mc_sol: None,
            peak_pnl_percent: None,
            executed_sell_rules: Vec::new(),
            partial_sell_count: 0,
            total_sold_percent: 0.0,
            dev_buy_usd: None,
            our_buy_usd: None,
            pnl_usd: None,
            mc_at_detection_usd: None,
            mc_at_entry_usd: None,
            current_value_usd: None,
            sell_failure_reason: None,
            sell_failure_timestamp: None,
            sell_reason: None,
            sell_timestamp: None,
            socials_source: None,
            tracking_error_count: 0,
            last_tracking_error: None,
            last_successful_tracking: None,
            suspicious_price_detected: false,
            bonding_curve_mismatch_detected: false,
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
            discord: None,
            twitter_type: None,
            creator_token_count: 1,
            detection_method: "instruction".to_string(),
            mc_at_detection_sol: Some(44.0),
            mc_at_entry_sol: Some(47.0),
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
            peak_mc_sol: None,
            peak_pnl_percent: None,
            executed_sell_rules: Vec::new(),
            partial_sell_count: 0,
            total_sold_percent: 0.0,
            dev_buy_usd: None,
            our_buy_usd: None,
            pnl_usd: None,
            mc_at_detection_usd: None,
            mc_at_entry_usd: None,
            current_value_usd: None,
            sell_failure_reason: None,
            sell_failure_timestamp: None,
            sell_reason: None,
            sell_timestamp: None,
            socials_source: None,
            tracking_error_count: 0,
            last_tracking_error: None,
            last_successful_tracking: None,
            suspicious_price_detected: false,
            bonding_curve_mismatch_detected: false,
        };
        
        tracker.record_buy(buy1).unwrap();
        tracker.record_buy(buy2).unwrap();

        let stats = tracker.get_stats();
        assert_eq!(stats.avg_mc_sol, 40.0); // (33 + 47) / 2
        assert_eq!(stats.min_mc_sol, 33.0);
        assert_eq!(stats.max_mc_sol, 47.0);
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
            discord: None,
            twitter_type: None,
            creator_token_count: 1,
            detection_method: "instruction".to_string(),
            mc_at_detection_sol: None,
            mc_at_entry_sol: None,
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
            peak_mc_sol: None,
            peak_pnl_percent: None,
            executed_sell_rules: Vec::new(),
            partial_sell_count: 0,
            total_sold_percent: 0.0,
            dev_buy_usd: None,
            our_buy_usd: None,
            pnl_usd: None,
            mc_at_detection_usd: None,
            mc_at_entry_usd: None,
            current_value_usd: None,
            sell_failure_reason: None,
            sell_failure_timestamp: None,
            sell_reason: None,
            sell_timestamp: None,
            socials_source: None,
            tracking_error_count: 0,
            last_tracking_error: None,
            last_successful_tracking: None,
            suspicious_price_detected: false,
            bonding_curve_mismatch_detected: false,
        };

        assert!(tracker.record_buy(buy).is_ok());
        assert_eq!(tracker.total_buys(), 1);
        // MC stats should remain at defaults
        assert_eq!(tracker.get_stats().avg_mc_sol, 0.0);
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
            discord: None,
            twitter_type: Some("account".to_string()),
            creator_token_count: 1,
            detection_method: "instruction".to_string(),
            mc_at_detection_sol: Some(35.0),
            mc_at_entry_sol: Some(36.5),
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
            peak_mc_sol: None,
            peak_pnl_percent: None,
            executed_sell_rules: Vec::new(),
            partial_sell_count: 0,
            total_sold_percent: 0.0,
            dev_buy_usd: None,
            our_buy_usd: None,
            pnl_usd: None,
            mc_at_detection_usd: None,
            mc_at_entry_usd: None,
            current_value_usd: None,
            sell_failure_reason: None,
            sell_failure_timestamp: None,
            sell_reason: None,
            sell_timestamp: None,
            socials_source: None,
            tracking_error_count: 0,
            last_tracking_error: None,
            last_successful_tracking: None,
            suspicious_price_detected: false,
            bonding_curve_mismatch_detected: false,
        };

        assert!(tracker.record_buy(buy).is_ok());

        // Verify files exist
        assert!(std::path::Path::new(&tracker.csv_path).exists());
        assert!(std::path::Path::new(&tracker.json_path).exists());

        // Cleanup
        let _ = std::fs::remove_file(&tracker.csv_path);
        let _ = std::fs::remove_file(&tracker.json_path);
    }

    #[test]
    fn test_json_tracker_file_naming_and_save() {
        // Test that new tracker creates files with correct naming format
        // First, clear any existing tracker files to ensure we get a new one
        let current_dir = std::env::current_dir().unwrap();
        let json_files: Vec<_> = std::fs::read_dir(&current_dir).unwrap()
            .filter_map(|entry| entry.ok())
            .filter_map(|entry| {
                let path = entry.path();
                if path.is_file() {
                    if let Some(file_name) = path.file_name().and_then(|n| n.to_str()) {
                        if (file_name.starts_with("tracker_") || file_name.starts_with("sniper_session_")) 
                            && file_name.ends_with(".json") {
                            return Some(path);
                        }
                    }
                }
                None
            })
            .collect();
        
        // Clean up old files for clean test
        for json_file in &json_files {
            let _ = std::fs::remove_file(json_file);
            // Also remove corresponding CSV
            if let Some(csv_path) = json_file.to_str().map(|s| s.replace(".json", ".csv")) {
                let _ = std::fs::remove_file(&csv_path);
            }
        }
        
        let mut tracker = TokenTracker::new().unwrap();
        tracker.clear_all_buys().unwrap(); // Clear any existing data for test isolation
        
        // Verify file name format: tracker_YYYY-MM-DD_HH-MM.json (new format)
        // or sniper_session_... (old format for compatibility)
        let json_filename = std::path::Path::new(&tracker.json_path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("");
        assert!(
            json_filename.starts_with("tracker_") || json_filename.starts_with("sniper_session_"),
            "JSON filename should start with 'tracker_' or 'sniper_session_', got: {}", json_filename
        );
        assert!(tracker.json_path.ends_with(".json"));
        
        // Check format only if it's the new format (tracker_)
        if json_filename.starts_with("tracker_") {
            // Check format: tracker_YYYY-MM-DD_HH-MM.json (runtime)
            // Test builds include extra precision to avoid collisions: tracker_YYYY-MM-DD_HH-MM-SS_SUBSEC.json
            let name_without_prefix = json_filename.strip_prefix("tracker_").unwrap();
            let name_without_suffix = name_without_prefix.strip_suffix(".json").unwrap();
            
            // Should match pattern: YYYY-MM-DD_HH-MM (runtime) or YYYY-MM-DD_HH-MM-SS_SUBSEC (tests)
            let parts: Vec<&str> = name_without_suffix.split('_').collect();
            assert!(
                parts.len() == 2 || parts.len() == 3,
                "Filename should have date and time separated by underscore (optional subsec part). Got parts={:?} for {}",
                parts,
                name_without_suffix
            );
            
            // Date part should be YYYY-MM-DD
            let date_parts: Vec<&str> = parts[0].split('-').collect();
            assert_eq!(date_parts.len(), 3, "Date should be in YYYY-MM-DD format");
            assert_eq!(date_parts[0].len(), 4, "Year should be 4 digits");
            
            // Time part should be HH-MM or HH-MM-SS
            let time_parts: Vec<&str> = parts[1].split('-').collect();
            assert!(
                time_parts.len() == 2 || time_parts.len() == 3,
                "Time should be in HH-MM or HH-MM-SS format"
            );

            // Optional: subsecond precision in tests (digits only)
            if parts.len() == 3 {
                assert!(
                    parts[2].chars().all(|c| c.is_ascii_digit()),
                    "Subsecond part should be digits only"
                );
            }
        }
        
        // Add a buy to trigger JSON save
        let buy = TokenBuy {
            token_number: 1,
            mint: "test_json_tracker_mint".to_string(),
            signature: "test_json_tracker_sig".to_string(),
            creator: "test_creator".to_string(),
            dev_buy_sol: 2.0,
            our_buy_sol: 0.1,
            timestamp: Utc::now(),
            has_socials: false,
            twitter: None,
            website: None,
            telegram: None,
            discord: None,
            twitter_type: None,
            creator_token_count: 1,
            detection_method: "instruction".to_string(),
            mc_at_detection_sol: Some(35.0),
            mc_at_entry_sol: Some(36.5),
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
            peak_mc_sol: None,
            peak_pnl_percent: None,
            executed_sell_rules: Vec::new(),
            partial_sell_count: 0,
            total_sold_percent: 0.0,
            dev_buy_usd: None,
            our_buy_usd: None,
            pnl_usd: None,
            mc_at_detection_usd: None,
            mc_at_entry_usd: None,
            current_value_usd: None,
            sell_failure_reason: None,
            sell_failure_timestamp: None,
            sell_reason: None,
            sell_timestamp: None,
            socials_source: None,
            tracking_error_count: 0,
            last_tracking_error: None,
            last_successful_tracking: None,
            suspicious_price_detected: false,
            bonding_curve_mismatch_detected: false,
        };
        
        // Record buy should save JSON
        assert!(tracker.record_buy(buy).is_ok());
        
        // Verify JSON file exists
        assert!(std::path::Path::new(&tracker.json_path).exists(), 
                "JSON file should exist at: {}", tracker.json_path);
        
        // Verify JSON content is valid
        let json_content = std::fs::read_to_string(&tracker.json_path).unwrap();
        let parsed: TrackerStats = serde_json::from_str(&json_content)
            .expect("JSON should be valid and parseable");
        
        assert_eq!(parsed.total_buys, 1);
        assert_eq!(parsed.buys.len(), 1);
        assert_eq!(parsed.buys[0].mint, "test_json_tracker_mint");
        
        // Cleanup
        let _ = std::fs::remove_file(&tracker.csv_path);
        let _ = std::fs::remove_file(&tracker.json_path);
    }

    #[test]
    fn test_token_amount_persistence() {
        // Test that token_amount is saved immediately when calculated
        // This prevents fallback method from being used on subsequent calls
        let mut tracker = TokenTracker::new().unwrap();

        let buy = TokenBuy {
            token_number: 1,
            mint: "test_token_amount_persistence".to_string(),
            signature: "test_sig".to_string(),
            creator: "test_creator".to_string(),
            dev_buy_sol: 2.0,
            our_buy_sol: 0.1,
            timestamp: Utc::now(),
            has_socials: false,
            twitter: None,
            website: None,
            telegram: None,
            discord: None,
            twitter_type: None,
            creator_token_count: 0,
            detection_method: "instruction".to_string(),
            mc_at_detection_sol: Some(36.5),
            mc_at_entry_sol: Some(36.5),
            token_price_sol: Some(0.00005), // Entry price: 0.00005 SOL per token
            token_amount: None, // Not set initially - should be calculated
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
            peak_mc_sol: None,
            peak_pnl_percent: None,
            executed_sell_rules: Vec::new(),
            partial_sell_count: 0,
            total_sold_percent: 0.0,
            dev_buy_usd: None,
            our_buy_usd: None,
            pnl_usd: None,
            mc_at_detection_usd: None,
            mc_at_entry_usd: None,
            current_value_usd: None,
            sell_failure_reason: None,
            sell_failure_timestamp: None,
            sell_reason: None,
            sell_timestamp: None,
            socials_source: None,
            tracking_error_count: 0,
            last_tracking_error: None,
            last_successful_tracking: None,
            suspicious_price_detected: false,
            bonding_curve_mismatch_detected: false,
        };

        tracker.record_buy(buy).unwrap();

        // First call: token_amount should be calculated and saved
        tracker.update_position_pnl_fast("test_token_amount_persistence", 0.0001).unwrap();
        let position1 = tracker.get_active_positions().into_iter()
            .find(|p| p.mint == "test_token_amount_persistence").unwrap();
        
        // token_amount should now be set (calculated from our_buy_sol / token_price_sol)
        assert!(position1.token_amount.is_some(), "token_amount should be calculated and saved");
        let expected_tokens = (0.1 / 0.00005 * 1e6) as u64; // 0.1 SOL / 0.00005 SOL/token * 1e6
        assert_eq!(position1.token_amount, Some(expected_tokens));

        // Second call: token_amount should still be there (not recalculated)
        tracker.update_position_pnl_fast("test_token_amount_persistence", 0.00015).unwrap();
        let position2 = tracker.get_active_positions().into_iter()
            .find(|p| p.mint == "test_token_amount_persistence").unwrap();
        
        // token_amount should remain the same (not lost)
        assert_eq!(position2.token_amount, Some(expected_tokens), 
                   "token_amount should persist between calls");
        
        // Both calls should use the same calculation method (precise, not fallback)
        assert!(position1.pnl_percent.is_some());
        assert!(position2.pnl_percent.is_some());
    }

    #[test]
    fn test_no_current_price_fallback_for_entry_price() {
        // Test that current_price_sol is NOT used as fallback for entry price calculation
        // Entry price should only be calculated from actual buy data
        let mut tracker = TokenTracker::new().unwrap();

        let buy = TokenBuy {
            token_number: 1,
            mint: "test_no_fallback".to_string(),
            signature: "test_sig".to_string(),
            creator: "test_creator".to_string(),
            dev_buy_sol: 2.0,
            our_buy_sol: 0.1,
            timestamp: Utc::now(),
            has_socials: false,
            twitter: None,
            website: None,
            telegram: None,
            discord: None,
            twitter_type: None,
            creator_token_count: 0,
            detection_method: "instruction".to_string(),
            mc_at_detection_sol: Some(36.5),
            mc_at_entry_sol: Some(36.5),
            token_price_sol: None, // Entry price not set - should NOT use current_price_sol
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
            peak_mc_sol: None,
            peak_pnl_percent: None,
            executed_sell_rules: Vec::new(),
            partial_sell_count: 0,
            total_sold_percent: 0.0,
            dev_buy_usd: None,
            our_buy_usd: None,
            pnl_usd: None,
            mc_at_detection_usd: None,
            mc_at_entry_usd: None,
            current_value_usd: None,
            sell_failure_reason: None,
            sell_failure_timestamp: None,
            sell_reason: None,
            sell_timestamp: None,
            socials_source: None,
            tracking_error_count: 0,
            last_tracking_error: None,
            last_successful_tracking: None,
            suspicious_price_detected: false,
            bonding_curve_mismatch_detected: false,
        };

        tracker.record_buy(buy).unwrap();

        // Update with a high current price
        // If current_price_sol was used as fallback, entry_price would be set to this high value
        tracker.update_position_pnl_fast("test_no_fallback", 0.001).unwrap();
        let position = tracker.get_active_positions().into_iter()
            .find(|p| p.mint == "test_no_fallback").unwrap();
        
        // Entry price should be calculated from our_buy_sol / token_amount, not from current_price_sol
        // Expected: 0.1 SOL / 2 tokens = 0.05 SOL per token
        assert!(position.token_price_sol.is_some());
        let expected_entry_price = 0.1 / 2.0; // our_buy_sol / tokens
        assert!((position.token_price_sol.unwrap() - expected_entry_price).abs() < 1e-6,
                "Entry price should be calculated from buy data, not current price");
        
        // Entry price should NOT be the current price (0.001)
        assert_ne!(position.token_price_sol.unwrap(), 0.001,
                   "Entry price should not use current_price_sol as fallback");
    }

    #[test]
    fn test_fallback_method_uses_same_formula() {
        // Test that fallback method uses the same fee calculation formula as precise method
        // This ensures consistent results even when token_amount is not available
        let mut tracker = TokenTracker::new().unwrap();

        let buy = TokenBuy {
            token_number: 1,
            mint: "test_fallback_formula".to_string(),
            signature: "test_sig".to_string(),
            creator: "test_creator".to_string(),
            dev_buy_sol: 2.0,
            our_buy_sol: 0.1,
            timestamp: Utc::now(),
            has_socials: false,
            twitter: None,
            website: None,
            telegram: None,
            discord: None,
            twitter_type: None,
            creator_token_count: 0,
            detection_method: "instruction".to_string(),
            mc_at_detection_sol: Some(36.5),
            mc_at_entry_sol: Some(36.5),
            token_price_sol: Some(0.00005), // Entry price set
            token_amount: None, // Not set - will use fallback method
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
            peak_mc_sol: None,
            peak_pnl_percent: None,
            executed_sell_rules: Vec::new(),
            partial_sell_count: 0,
            total_sold_percent: 0.0,
            dev_buy_usd: None,
            our_buy_usd: None,
            pnl_usd: None,
            mc_at_detection_usd: None,
            mc_at_entry_usd: None,
            current_value_usd: None,
            sell_failure_reason: None,
            sell_failure_timestamp: None,
            sell_reason: None,
            sell_timestamp: None,
            socials_source: None,
            tracking_error_count: 0,
            last_tracking_error: None,
            last_successful_tracking: None,
            suspicious_price_detected: false,
            bonding_curve_mismatch_detected: false,
        };

        tracker.record_buy(buy).unwrap();

        // Update PnL - should use fallback method since token_amount is None
        let current_price = 0.0001; // 2x entry price
        tracker.update_position_pnl_fast("test_fallback_formula", current_price).unwrap();
        let position = tracker.get_active_positions().into_iter()
            .find(|p| p.mint == "test_fallback_formula").unwrap();
        
        // Should have calculated PnL using fallback method
        assert!(position.pnl_sol.is_some());
        assert!(position.pnl_percent.is_some());
        
        // Verify fallback method uses same formula structure:
        // - cost_basis = our_buy_sol + buy_fees
        // - current_value_net = (current_value_gross * 0.99) - estimated_sell_fees
        // - pnl = current_value_net - cost_basis
        let cost_basis = 0.1 + 0.00001; // our_buy_sol + buy_fees
        let tokens_approx = 0.1 / 0.00005; // our_buy_sol / entry_price
        let current_value_gross_approx = tokens_approx * current_price;
        let estimated_sell_fees = 0.00001;
        let current_value_net_approx = (current_value_gross_approx * 0.99) - estimated_sell_fees;
        let expected_pnl = current_value_net_approx - cost_basis;
        let expected_pnl_percent = (expected_pnl / cost_basis) * 100.0;
        
        // Allow small floating point differences
        assert!((position.pnl_sol.unwrap() - expected_pnl).abs() < 0.001,
                "Fallback method should use same formula as precise method");
        assert!((position.pnl_percent.unwrap() - expected_pnl_percent).abs() < 1.0,
                "Fallback method should calculate percentage using same formula");
    }

    #[test]
    fn test_consistent_calculation_method() {
        // Test that once token_amount is set, the same calculation method is used consistently
        // This prevents blinking between two different values
        let mut tracker = TokenTracker::new().unwrap();

        let buy = TokenBuy {
            token_number: 1,
            mint: "test_consistent_method".to_string(),
            signature: "test_sig".to_string(),
            creator: "test_creator".to_string(),
            dev_buy_sol: 2.0,
            our_buy_sol: 0.1,
            timestamp: Utc::now(),
            has_socials: false,
            twitter: None,
            website: None,
            telegram: None,
            discord: None,
            twitter_type: None,
            creator_token_count: 0,
            detection_method: "instruction".to_string(),
            mc_at_detection_sol: Some(36.5),
            mc_at_entry_sol: Some(36.5),
            token_price_sol: Some(0.00005),
            token_amount: None, // Will be calculated on first call
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
            peak_mc_sol: None,
            peak_pnl_percent: None,
            executed_sell_rules: Vec::new(),
            partial_sell_count: 0,
            total_sold_percent: 0.0,
            dev_buy_usd: None,
            our_buy_usd: None,
            pnl_usd: None,
            mc_at_detection_usd: None,
            mc_at_entry_usd: None,
            current_value_usd: None,
            sell_failure_reason: None,
            sell_failure_timestamp: None,
            sell_reason: None,
            sell_timestamp: None,
            socials_source: None,
            tracking_error_count: 0,
            last_tracking_error: None,
            last_successful_tracking: None,
            suspicious_price_detected: false,
            bonding_curve_mismatch_detected: false,
        };

        tracker.record_buy(buy).unwrap();

        // First call: should calculate token_amount and use precise method
        tracker.update_position_pnl_fast("test_consistent_method", 0.0001).unwrap();
        let position1 = tracker.get_active_positions().into_iter()
            .find(|p| p.mint == "test_consistent_method").unwrap();
        let pnl_percent1 = position1.pnl_percent;
        assert!(position1.token_amount.is_some(), "token_amount should be set after first call");

        // Second call: should use same method (precise, not fallback)
        tracker.update_position_pnl_fast("test_consistent_method", 0.00012).unwrap();
        let position2 = tracker.get_active_positions().into_iter()
            .find(|p| p.mint == "test_consistent_method").unwrap();
        let pnl_percent2 = position2.pnl_percent;
        
        // token_amount should still be set
        assert!(position2.token_amount.is_some(), 
                "token_amount should persist and not be lost");
        assert_eq!(position1.token_amount, position2.token_amount,
                   "token_amount should remain the same between calls");

        // Third call: should still use same method
        tracker.update_position_pnl_fast("test_consistent_method", 0.00015).unwrap();
        let position3 = tracker.get_active_positions().into_iter()
            .find(|p| p.mint == "test_consistent_method").unwrap();
        let pnl_percent3 = position3.pnl_percent;
        
        // All three calls should have calculated PnL (not None)
        assert!(pnl_percent1.is_some());
        assert!(pnl_percent2.is_some());
        assert!(pnl_percent3.is_some());
        
        // PnL should increase as price increases (0.0001 -> 0.00012 -> 0.00015)
        assert!(pnl_percent2.unwrap() > pnl_percent1.unwrap(),
                "PnL should increase when price increases");
        assert!(pnl_percent3.unwrap() > pnl_percent2.unwrap(),
                "PnL should continue increasing with price");
    }
}