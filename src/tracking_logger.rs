// tracking_logger.rs - Detailed logging for position tracking operations
use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::Path;
use std::sync::Mutex;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackingLogEntry {
    pub timestamp: DateTime<Utc>,
    pub mint: String,
    pub mint_short: String,  // First 8 chars for readability
    
    // Position info at time of tracking
    pub position_info: PositionTrackingInfo,
    
    // Tracking operation details
    pub operation: TrackingOperation,
    
    // Result
    pub success: bool,
    pub error: Option<TrackingError>,
    
    // Performance metrics
    pub duration_ms: Option<u64>,
    
    // Context
    pub context: TrackingContext,
    
    // Validation flags
    pub validation: Option<ValidationInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PositionTrackingInfo {
    pub bonding_curve: Option<String>,
    pub token_amount: Option<u64>,
    pub our_buy_sol: f64,
    pub buy_fees_sol: Option<f64>,
    pub entry_price_sol: Option<f64>,
    pub current_price_sol: Option<f64>,
    pub current_mc_sol: Option<f64>,
    pub pnl_sol: Option<f64>,
    pub pnl_percent: Option<f64>,
    pub last_pnl_update: Option<DateTime<Utc>>,
    pub sold: bool,
    pub sell_signature: Option<String>,
    pub tracking_error_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum TrackingOperation {
    PnLUpdate {
        current_price: f64,
        calculated_pnl: Option<f64>,
        calculated_pnl_percent: Option<f64>,
        method: String,  // "precise" or "fallback"
        price_change_percent: Option<f64>,  // % change from entry price
    },
    BondingCurveFetch {
        bonding_curve: String,
        success: bool,
        account_exists: Option<bool>,
        account_data_size: Option<usize>,
        deserialization_success: Option<bool>,
    },
    PeakMCUpdate {
        current_mc: f64,
        peak_mc: Option<f64>,
    },
    PriceValidation {
        entry_price: f64,
        current_price: f64,
        price_ratio: f64,  // current / entry
        is_suspicious: bool,  // true if price changed >10x unexpectedly
        validation_reason: Option<String>,
    },
    TrackingHealthCheck {
        error_count: u32,
        last_error_ago_sec: Option<i64>,
        last_success_ago_sec: Option<i64>,
        is_healthy: bool,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackingError {
    pub error_type: String,  // "bonding_curve_fetch_failed", "pnl_update_failed", "price_suspicious", etc.
    pub error_message: String,
    pub error_code: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackingContext {
    pub monitor_cycle: Option<u64>,  // Which monitoring cycle this is
    pub batch_index: Option<usize>,  // Position in batch
    pub batch_size: Option<usize>,
    pub rpc_endpoint: Option<String>,
    pub network_latency_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationInfo {
    pub price_ratio_valid: bool,  // current_price / entry_price is reasonable
    pub price_ratio: f64,
    pub bonding_curve_valid: bool,  // Bonding curve account is valid
    pub token_amount_valid: bool,  // Token amount is reasonable
    pub warnings: Vec<String>,  // Any warnings about data quality
}

pub struct TrackingLogger {
    file_path: String,
    writer: Mutex<BufWriter<File>>,
    monitor_cycle: Mutex<u64>,
}

impl TrackingLogger {
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
            monitor_cycle: Mutex::new(0),
        })
    }
    
    pub fn increment_cycle(&self) {
        let mut cycle = self.monitor_cycle.lock().unwrap();
        *cycle += 1;
    }
    
    pub fn get_cycle(&self) -> u64 {
        *self.monitor_cycle.lock().unwrap()
    }
    
    fn log_entry(&self, entry: TrackingLogEntry) -> Result<()> {
        let mut writer = self.writer.lock().unwrap();
        
        // Write JSON entry as a single line (JSONL format)
        serde_json::to_writer(&mut *writer, &entry)?;
        writer.write_all(b"\n")?;
        writer.flush()?;
        
        Ok(())
    }
    
    /// Log PnL update attempt with validation
    pub fn log_pnl_update(
        &self,
        mint: &str,
        position_info: PositionTrackingInfo,
        current_price: f64,
        result: Result<(Option<f64>, Option<f64>), anyhow::Error>,
        method: &str,
        duration_ms: Option<u64>,
        context: TrackingContext,
    ) -> Result<()> {
        // Calculate price change
        let price_change_percent = if let Some(entry_price) = position_info.entry_price_sol {
            if entry_price > 0.0 {
                Some(((current_price - entry_price) / entry_price) * 100.0)
            } else {
                None
            }
        } else {
            None
        };
        
        // Clone position_info before using it in validation
        let position_info_clone = position_info.clone();
        
        // Validate price - check if suspicious
        let validation = if let Some(entry_price) = position_info.entry_price_sol {
            let price_ratio = if entry_price > 0.0 {
                current_price / entry_price
            } else {
                1.0
            };
            
            // Flag as suspicious if price changed >10x in either direction unexpectedly
            // (this could indicate wrong bonding curve account or data corruption)
            let is_suspicious = price_ratio > 10.0 || price_ratio < 0.1;
            
            let mut warnings = Vec::new();
            if is_suspicious {
                warnings.push(format!(
                    "SUSPICIOUS: Price ratio {:.2}x (entry: {:.8e}, current: {:.8e})",
                    price_ratio, entry_price, current_price
                ));
            }
            
            Some(ValidationInfo {
                price_ratio_valid: !is_suspicious,
                price_ratio,
                bonding_curve_valid: position_info.bonding_curve.is_some(),
                token_amount_valid: position_info.token_amount.is_some(),
                warnings,
            })
        } else {
            None
        };
        
        let (success, error, calculated_pnl, calculated_pnl_percent) = match result {
            Ok((pnl_sol, pnl_percent)) => (true, None, pnl_sol, pnl_percent),
            Err(e) => (
                false,
                Some(TrackingError {
                    error_type: "pnl_update_failed".to_string(),
                    error_message: format!("{}", e),
                    error_code: None,
                }),
                None,
                None,
            ),
        };
        
        let entry = TrackingLogEntry {
            timestamp: Utc::now(),
            mint: mint.to_string(),
            mint_short: if mint.len() > 8 {
                mint[..8].to_string()
            } else {
                mint.to_string()
            },
            position_info: position_info_clone,
            operation: TrackingOperation::PnLUpdate {
                current_price,
                calculated_pnl,
                calculated_pnl_percent,
                method: method.to_string(),
                price_change_percent,
            },
            success,
            error,
            duration_ms,
            context,
            validation: validation.clone(),
        };
        
        // Log critical warnings to console
        if let Some(ref val) = validation {
            if !val.warnings.is_empty() {
                eprintln!("🚨 TRACKING WARNING for {}: {}", &mint[..8], val.warnings.join(", "));
            }
        }
        
        self.log_entry(entry)
    }
    
    /// Log bonding curve fetch attempt
    pub fn log_bonding_curve_fetch(
        &self,
        mint: &str,
        position_info: PositionTrackingInfo,
        bonding_curve: &str,
        result: Result<(bool, Option<usize>, bool), anyhow::Error>,  // (exists, data_size, deserialized)
        duration_ms: Option<u64>,
        context: TrackingContext,
    ) -> Result<()> {
        let (success, error, account_exists, account_data_size, deserialization_success) = match result {
            Ok((exists, size, deserialized)) => (true, None, Some(exists), size, Some(deserialized)),
            Err(e) => (
                false,
                Some(TrackingError {
                    error_type: "bonding_curve_fetch_failed".to_string(),
                    error_message: format!("{}", e),
                    error_code: None,
                }),
                None,
                None,
                None,
            ),
        };
        
        let entry = TrackingLogEntry {
            timestamp: Utc::now(),
            mint: mint.to_string(),
            mint_short: if mint.len() > 8 {
                mint[..8].to_string()
            } else {
                mint.to_string()
            },
            position_info,
            operation: TrackingOperation::BondingCurveFetch {
                bonding_curve: bonding_curve.to_string(),
                success,
                account_exists,
                account_data_size,
                deserialization_success,
            },
            success,
            error,
            duration_ms,
            context,
            validation: None,
        };
        
        self.log_entry(entry)
    }
    
    /// Log price validation check
    pub fn log_price_validation(
        &self,
        mint: &str,
        position_info: PositionTrackingInfo,
        entry_price: f64,
        current_price: f64,
        is_suspicious: bool,
        reason: Option<String>,
        context: TrackingContext,
    ) -> Result<()> {
        let price_ratio = if entry_price > 0.0 {
            current_price / entry_price
        } else {
            1.0
        };
        
        // Clone position_info before using it in validation
        let position_info_clone = position_info.clone();
        let bonding_curve_valid = position_info.bonding_curve.is_some();
        let token_amount_valid = position_info.token_amount.is_some();
        
        let reason_clone = reason.clone();
        let entry = TrackingLogEntry {
            timestamp: Utc::now(),
            mint: mint.to_string(),
            mint_short: if mint.len() > 8 {
                mint[..8].to_string()
            } else {
                mint.to_string()
            },
            position_info: position_info_clone,
            operation: TrackingOperation::PriceValidation {
                entry_price,
                current_price,
                price_ratio,
                is_suspicious,
                validation_reason: reason_clone,
            },
            success: !is_suspicious,
            error: if is_suspicious {
                Some(TrackingError {
                    error_type: "price_suspicious".to_string(),
                    error_message: format!("Price ratio {:.2}x is suspicious", price_ratio),
                    error_code: None,
                })
            } else {
                None
            },
            duration_ms: None,
            context,
            validation: Some(ValidationInfo {
                price_ratio_valid: !is_suspicious,
                price_ratio,
                bonding_curve_valid,
                token_amount_valid,
                warnings: if is_suspicious {
                    vec![format!("Suspicious price ratio: {:.2}x", price_ratio)]
                } else {
                    Vec::new()
                },
            }),
        };
        
        if is_suspicious {
            eprintln!("🚨 SUSPICIOUS PRICE for {}: ratio {:.2}x (entry: {:.8e}, current: {:.8e}) - {}", 
                     &mint[..8], price_ratio, entry_price, current_price,
                     reason.as_deref().unwrap_or("unknown reason"));
        }
        
        self.log_entry(entry)
    }
    
    /// Log critical tracking failure
    pub fn log_critical_tracking_failure(
        &self,
        mint: &str,
        position_info: PositionTrackingInfo,
        error: TrackingError,
        consecutive_failures: u32,
        context: TrackingContext,
    ) -> Result<()> {
        let entry = TrackingLogEntry {
            timestamp: Utc::now(),
            mint: mint.to_string(),
            mint_short: if mint.len() > 8 {
                mint[..8].to_string()
            } else {
                mint.to_string()
            },
            position_info,
            operation: TrackingOperation::TrackingHealthCheck {
                error_count: consecutive_failures,
                last_error_ago_sec: Some(0),
                last_success_ago_sec: None,
                is_healthy: false,
            },
            success: false,
            error: Some(error.clone()),
            duration_ms: None,
            context,
            validation: None,
        };
        
        // Also print to console for immediate visibility
        eprintln!("🚨 CRITICAL TRACKING FAILURE for {}: {} consecutive failures - {} ({})", 
                 &mint[..8], consecutive_failures, error.error_message, error.error_type);
        
        self.log_entry(entry)
    }
}

// Helper function to create logger with timestamped filename
pub fn create_tracking_logger() -> Result<TrackingLogger> {
    let timestamp = Utc::now().format("%Y%m%d_%H%M%S");
    let filename = format!("tracking_log_{}.jsonl", timestamp);
    eprintln!("📊 Creating tracking logger: {}", filename);
    TrackingLogger::new(&filename)
}

// Helper to create PositionTrackingInfo from TokenBuy
impl From<&crate::accounts::TokenBuy> for PositionTrackingInfo {
    fn from(buy: &crate::accounts::TokenBuy) -> Self {
        PositionTrackingInfo {
            bonding_curve: buy.bonding_curve.clone(),
            token_amount: buy.token_amount,
            our_buy_sol: buy.our_buy_sol,
            buy_fees_sol: buy.buy_fees_sol,
            entry_price_sol: buy.token_price_sol,
            current_price_sol: buy.current_price_sol,
            current_mc_sol: buy.mc_at_entry_sol,  // Use entry MC as approximation
            pnl_sol: buy.pnl_sol,
            pnl_percent: buy.pnl_percent,
            last_pnl_update: buy.last_pnl_update,
            sold: buy.sold,
            sell_signature: buy.sell_signature.clone(),
            tracking_error_count: 0,  // Will be set from actual error count if we add it
        }
    }
}

