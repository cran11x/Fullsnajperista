// tabs/buys.rs - Recent buys table
#![allow(unused)]

use eframe::egui;
use std::sync::{Arc, RwLock};
use crate::accounts::TokenTracker;

pub fn render(ui: &mut egui::Ui, tracker: &Arc<RwLock<Option<TokenTracker>>>) {
    ui.vertical_centered(|ui| {
        ui.add_space(16.0);
        ui.label(egui::RichText::new("💰 Recent Buys")
            .size(28.0)
            .strong()
            .color(egui::Color32::from_rgb(255, 220, 0)));
        ui.add_space(4.0);
        ui.label(egui::RichText::new("Track your successful token purchases")
            .size(14.0)
            .color(egui::Color32::from_rgb(160, 170, 185)));
    });
    ui.add_space(24.0);
    
    // Force refresh every frame by always reading fresh data
    // Also try to reload from JSON file to get latest data
    let tracker_opt = match tracker.read() {
        Ok(t) => t,
        Err(e) => {
            ui.label(format!("Error reading tracker: {}", e));
            eprintln!("❌ Error reading tracker in GUI: {}", e);
            return;
        }
    };
    
    if let Some(tracker_ref) = tracker_opt.as_ref() {
        // Try to reload tracker from JSON file to get latest data
        // This ensures we see new buys even if tracker wasn't updated in memory
        let reloaded_tracker = crate::accounts::TokenTracker::new().ok();
        let tracker_to_use = reloaded_tracker.as_ref().unwrap_or(tracker_ref);
        
        eprintln!("🔍 Tracker exists in GUI, getting buys... (reloaded: {})", reloaded_tracker.is_some());
        // Safely get total buys count
        let total_buys = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            tracker_to_use.total_buys()
        })).unwrap_or(0);
        
        // Safely get buys with error handling
        let buys = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            tracker_to_use.get_recent_buys(50)
        })).unwrap_or_else(|_| {
            eprintln!("⚠️  Error getting recent buys from tracker");
            Vec::new()
        });
        
        eprintln!("🔍 Found {} buys in tracker (total: {})", buys.len(), total_buys);
        
        // Enhanced total buys count display
        ui.label(egui::RichText::new(format!("Total buys recorded: {}", total_buys))
            .size(15.0)
            .strong()
            .color(egui::Color32::from_rgb(180, 200, 255)));
        ui.add_space(12.0);
        
        if buys.is_empty() {
            ui.vertical_centered(|ui| {
                ui.add_space(40.0);
                ui.label(egui::RichText::new("No buys recorded yet.")
                    .size(16.0)
                    .color(egui::Color32::from_rgb(160, 170, 185)));
            });
        } else {
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                // Improved grid with proper column widths and responsive spacing
                let available_width = ui.available_width();
                let spacing = if available_width > 1000.0 { 20.0 } else { 12.0 };
                
                egui::Grid::new("buys_grid")
                    .num_columns(6)
                    .spacing([spacing, 10.0])
                    .striped(true)
                    .min_row_height(42.0)
                    .show(ui, |ui| {
                        // Enhanced header with proper column widths
                        ui.set_width(90.0); // Time column
                        ui.label(egui::RichText::new("Time")
                            .size(15.0)
                            .strong()
                            .color(egui::Color32::from_rgb(220, 230, 245)));
                        ui.set_width(200.0); // Token column
                        ui.label(egui::RichText::new("Token")
                            .size(15.0)
                            .strong()
                            .color(egui::Color32::from_rgb(220, 230, 245)));
                        ui.set_width(100.0); // MC column
                        ui.label(egui::RichText::new("MC ($)")
                            .size(15.0)
                            .strong()
                            .color(egui::Color32::from_rgb(220, 230, 245)));
                        ui.set_width(120.0); // Dev Buy column
                        ui.label(egui::RichText::new("Dev Buy (SOL)")
                            .size(15.0)
                            .strong()
                            .color(egui::Color32::from_rgb(220, 230, 245)));
                        ui.set_width(100.0); // Socials column
                        ui.label(egui::RichText::new("Socials")
                            .size(15.0)
                            .strong()
                            .color(egui::Color32::from_rgb(220, 230, 245)));
                        ui.set_width(180.0); // Transaction column
                        ui.label(egui::RichText::new("Transaction")
                            .size(15.0)
                            .strong()
                            .color(egui::Color32::from_rgb(220, 230, 245)));
                        ui.end_row();
                        
                        // Enhanced rows with premium styling and hover effects
                        for buy in buys.iter().rev() {
                            // Time column
                            ui.set_width(90.0);
                            let time_str = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                                buy.timestamp.format("%H:%M:%S").to_string()
                            })).unwrap_or_else(|_| {
                                "N/A".to_string()
                            });
                            ui.label(egui::RichText::new(time_str)
                                .size(13.0)
                                .color(egui::Color32::from_rgb(170, 190, 210))
                                .monospace());
                            
                            // Token column
                            ui.set_width(200.0);
                            let mint_display = format_address_safe(&buy.mint);
                            ui.label(egui::RichText::new(mint_display)
                                .size(13.0)
                                .monospace()
                                .color(egui::Color32::from_rgb(160, 210, 255)));
                            
                            // MC column
                            ui.set_width(100.0);
                            let mc_display = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                                if let Some(mc) = buy.mc_at_entry_usd {
                                    if mc.is_finite() && mc >= 0.0 && !mc.is_nan() {
                                        let mc_color = if mc > 10000.0 {
                                            egui::Color32::from_rgb(100, 255, 160)
                                        } else if mc > 1000.0 {
                                            egui::Color32::from_rgb(255, 230, 110)
                                        } else {
                                            egui::Color32::from_rgb(255, 190, 190)
                                        };
                                        Some((format!("${:.0}", mc), mc_color))
                                    } else {
                                        None
                                    }
                                } else {
                                    None
                                }
                            })).unwrap_or(None);
                            
                            if let Some((mc_text, mc_color)) = mc_display {
                                ui.label(egui::RichText::new(mc_text)
                                    .size(13.0)
                                    .strong()
                                    .color(mc_color));
                            } else {
                                ui.label(egui::RichText::new("-")
                                    .size(13.0)
                                    .color(egui::Color32::from_rgb(120, 130, 150)));
                            }
                            
                            // Dev Buy column
                            ui.set_width(120.0);
                            let dev_buy_str = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                                if buy.dev_buy_sol.is_finite() && !buy.dev_buy_sol.is_nan() {
                                    format!("{:.3}", buy.dev_buy_sol)
                                } else {
                                    "0.000".to_string()
                                }
                            })).unwrap_or_else(|_| {
                                "0.000".to_string()
                            });
                            ui.label(egui::RichText::new(dev_buy_str)
                                .size(13.0)
                                .strong()
                                .color(egui::Color32::from_rgb(255, 220, 0)));
                            
                            // Socials column
                            ui.set_width(100.0);
                            let (socials_str, socials_color) = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                                let socials_count = if buy.has_socials {
                                    [buy.twitter.as_ref(), buy.website.as_ref(), buy.telegram.as_ref()]
                                        .iter()
                                        .filter(|s| s.is_some())
                                        .count()
                                } else {
                                    0
                                };
                                let str = if buy.has_socials {
                                    format!("✓ ({})", socials_count)
                                } else {
                                    "✗".to_string()
                                };
                                let color = if buy.has_socials {
                                    egui::Color32::from_rgb(100, 255, 160)
                                } else {
                                    egui::Color32::from_rgb(255, 130, 130)
                                };
                                (str, color)
                            })).unwrap_or_else(|_| {
                                ("✗".to_string(), egui::Color32::from_rgb(255, 130, 130))
                            });
                            ui.label(egui::RichText::new(socials_str)
                                .size(13.0)
                                .strong()
                                .color(socials_color));
                            
                            // Transaction column
                            ui.set_width(180.0);
                            let sig_short = format_address_safe(&buy.signature);
                            let sig_button = ui.add(egui::Button::new(egui::RichText::new(sig_short.clone())
                                    .size(12.0)
                                    .monospace()
                                    .color(egui::Color32::from_rgb(120, 200, 255)))
                                    .fill(egui::Color32::TRANSPARENT)
                                    .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(120, 200, 255).linear_multiply(0.3)))
                                    .rounding(egui::Rounding::same(4.0)));
                            
                            // Handle click outside of grid to avoid layout issues
                            if sig_button.clicked() {
                                if !buy.signature.is_empty() {
                                    let url = format!("https://solscan.io/tx/{}", buy.signature);
                                    // Safely open URL with error handling
                                    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                                        if let Err(_e) = open::that(&url) {
                                            eprintln!("⚠️  Failed to open URL: {}", url);
                                        }
                                    })).unwrap_or_else(|_| {
                                        eprintln!("⚠️  Panic while opening URL");
                                    });
                                }
                            }
                            
                            ui.end_row();
                        }
                    });
            });
            
            ui.add_space(16.0);
            
            // Enhanced export button
            let export_response = ui.add(egui::Button::new(egui::RichText::new("📥 Export CSV")
                    .size(14.0)
                    .strong())
                    .fill(egui::Color32::from_rgb(100, 200, 255).linear_multiply(0.2))
                    .stroke(egui::Stroke::new(1.5, egui::Color32::from_rgb(100, 200, 255)))
                    .min_size(egui::vec2(140.0, 36.0))
                    .rounding(egui::Rounding::same(6.0)));
            if export_response.clicked() {
                // CSV export is already handled by tracker
                ui.label(egui::RichText::new("CSV file saved automatically in project directory.")
                    .size(12.0)
                    .color(egui::Color32::from_rgb(100, 255, 160)));
            }
        }
    } else {
        eprintln!("⚠️  Tracker is None in GUI - not initialized or disabled");
        ui.vertical_centered(|ui| {
            ui.add_space(40.0);
            ui.label(egui::RichText::new("Tracker not initialized. Enable tracking in settings.")
                .size(16.0)
                .color(egui::Color32::from_rgb(160, 170, 185)));
        });
    }
}

fn format_address(addr: &str) -> String {
    format_address_safe(addr)
}

fn format_address_safe(addr: &str) -> String {
    if addr.is_empty() {
        return "N/A".to_string();
    }
    // Safe string slicing to avoid panic on invalid UTF-8 boundaries
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        if addr.len() > 12 {
            // Use byte slicing for safety, then convert to string
            let bytes = addr.as_bytes();
            if bytes.len() >= 12 {
                let start = String::from_utf8_lossy(&bytes[..6.min(bytes.len())]);
                let end = String::from_utf8_lossy(&bytes[bytes.len().saturating_sub(6)..]);
                format!("{}...{}", start, end)
            } else {
                addr.to_string()
            }
        } else {
            addr.to_string()
        }
    })).unwrap_or_else(|_| {
        // Fallback to simple truncation if panic occurs
        if addr.len() > 20 {
            // Safe truncation using chars to avoid UTF-8 boundary issues
            let truncated: String = addr.chars().take(17).collect();
            format!("{}...", truncated)
        } else {
            addr.to_string()
        }
    })
}

