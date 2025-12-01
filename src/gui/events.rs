// events.rs - Event system for bot-GUI communication
#![allow(unused)]

use chrono::{DateTime, Utc};

#[derive(Debug, Clone)]
pub enum TokenEvent {
    Detected {
        mint: String,
        timestamp: DateTime<Utc>,
    },
    Filtered {
        mint: String,
        reason: String,
        timestamp: DateTime<Utc>,
    },
    Bought {
        mint: String,
        signature: String,
        mc: Option<f64>,
        timestamp: DateTime<Utc>,
    },
    Sold {
        mint: String,
        signature: String,
        reason: String,
        pnl: Option<f64>,
        timestamp: DateTime<Utc>,
    },
    Error {
        message: String,
        timestamp: DateTime<Utc>,
    },
    Info {
        message: String,
        timestamp: DateTime<Utc>,
    },
}

#[derive(Debug, Clone)]
pub enum BotControl {
    Start,
    Stop,
    UpdateConfig(crate::config::Config),
    Restart,
    ManualSell(String), // Mint address to sell
    ManualBuy { mint: String, sol_amount: Option<u64> }, // Manual buy with mint address and optional SOL amount
}

impl TokenEvent {
    pub fn format_for_display(&self) -> String {
        match self {
            TokenEvent::Detected { mint, .. } => {
                format!("🔍 Detected: {}", format_address(&mint))
            }
            TokenEvent::Filtered { mint, reason, .. } => {
                format!("⏭️  Filtered: {} - {}", format_address(&mint), reason)
            }
            TokenEvent::Bought { mint, signature, mc, .. } => {
                let mc_str = mc.map(|m| format!(" (MC: ${:.0})", m)).unwrap_or_default();
                format!("✅ Bought: {} {} - {}", format_address(&mint), mc_str, format_address(&signature))
            }
            TokenEvent::Sold { mint, signature, reason, pnl, .. } => {
                let pnl_str = pnl.map(|p| {
                    if p >= 0.0 {
                        format!(" (+{:.4} SOL)", p)
                    } else {
                        format!(" ({:.4} SOL)", p)
                    }
                }).unwrap_or_default();
                format!("💰 Sold: {} ({}){} - {}", format_address(&mint), reason, pnl_str, format_address(&signature))
            }
            TokenEvent::Error { message, .. } => {
                format!("❌ Error: {}", message)
            }
            TokenEvent::Info { message, .. } => {
                format!("ℹ️  {}", message)
            }
        }
    }

    pub fn timestamp(&self) -> DateTime<Utc> {
        match self {
            TokenEvent::Detected { timestamp, .. } => *timestamp,
            TokenEvent::Filtered { timestamp, .. } => *timestamp,
            TokenEvent::Bought { timestamp, .. } => *timestamp,
            TokenEvent::Sold { timestamp, .. } => *timestamp,
            TokenEvent::Error { timestamp, .. } => *timestamp,
            TokenEvent::Info { timestamp, .. } => *timestamp,
        }
    }
}

fn format_address(addr: &str) -> String {
    if addr.len() > 8 {
        format!("{}...{}", &addr[..4], &addr[addr.len()-4..])
    } else {
        addr.to_string()
    }
}

