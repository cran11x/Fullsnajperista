// tabs/buys.rs - Recent buys table
#![allow(unused)]

use eframe::egui;
use std::sync::{Arc, RwLock};
use crate::accounts::TokenTracker;

pub fn render(ui: &mut egui::Ui, tracker: &Arc<RwLock<Option<TokenTracker>>>) {
    ui.vertical_centered(|ui| {
        ui.add_space(8.0);
        ui.label(egui::RichText::new("💰 Recent Buys")
            .size(22.0)
            .strong()
            .color(egui::Color32::from_rgb(255, 215, 100)));
    });
    ui.add_space(16.0);
    
    // Force refresh every frame by always reading fresh data
    let tracker_opt = match tracker.read() {
        Ok(t) => t,
        Err(e) => {
            ui.label(format!("Error reading tracker: {}", e));
            return;
        }
    };
    
    if let Some(tracker_ref) = tracker_opt.as_ref() {
        let total_buys = tracker_ref.total_buys();
        // Safely get buys with error handling
        let buys = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            tracker_ref.get_recent_buys(50)
        })).unwrap_or_else(|_| {
            eprintln!("⚠️  Error getting recent buys from tracker");
            Vec::new()
        });
        
        // Show total buys count for debugging
        ui.label(egui::RichText::new(format!("Total buys recorded: {}", total_buys))
            .size(11.0)
            .color(egui::Color32::from_gray(150)));
        ui.add_space(4.0);
        
        if buys.is_empty() {
            ui.label("No buys recorded yet.");
        } else {
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                egui::Grid::new("buys_grid")
                    .num_columns(6)
                    .spacing([16.0, 6.0])
                    .striped(true)
                    .min_row_height(32.0)
                    .show(ui, |ui| {
                        // Header with modern styling
                        ui.add_space(8.0);
                        ui.label(egui::RichText::new("Time")
                            .size(13.0)
                            .strong()
                            .color(egui::Color32::from_rgb(200, 200, 220)));
                        ui.label(egui::RichText::new("Token")
                            .size(13.0)
                            .strong()
                            .color(egui::Color32::from_rgb(200, 200, 220)));
                        ui.label(egui::RichText::new("MC ($)")
                            .size(13.0)
                            .strong()
                            .color(egui::Color32::from_rgb(200, 200, 220)));
                        ui.label(egui::RichText::new("Dev Buy (SOL)")
                            .size(13.0)
                            .strong()
                            .color(egui::Color32::from_rgb(200, 200, 220)));
                        ui.label(egui::RichText::new("Socials")
                            .size(13.0)
                            .strong()
                            .color(egui::Color32::from_rgb(200, 200, 220)));
                        ui.label(egui::RichText::new("Transaction")
                            .size(13.0)
                            .strong()
                            .color(egui::Color32::from_rgb(200, 200, 220)));
                        ui.end_row();
                        
                        // Rows with better styling
                        for buy in buys.iter().rev() {
                            // Time - safe formatting
                            let time_str = buy.timestamp.format("%H:%M:%S").to_string();
                            ui.label(egui::RichText::new(time_str)
                                .size(12.0)
                                .color(egui::Color32::from_rgb(160, 180, 200))
                                .monospace());
                            
                            // Token (shortened) - safe formatting
                            let mint_display = format_address_safe(&buy.mint);
                            ui.label(egui::RichText::new(mint_display)
                                .size(12.0)
                                .monospace()
                                .color(egui::Color32::from_rgb(150, 200, 255)));
                            
                            // MC with color coding - safe formatting
                            if let Some(mc) = buy.mc_at_entry_usd {
                                if mc.is_finite() && mc >= 0.0 {
                                    let mc_color = if mc > 10000.0 {
                                        egui::Color32::from_rgb(100, 255, 150)
                                    } else if mc > 1000.0 {
                                        egui::Color32::from_rgb(255, 220, 100)
                                    } else {
                                        egui::Color32::from_rgb(255, 180, 180)
                                    };
                                    ui.label(egui::RichText::new(format!("${:.0}", mc))
                                        .size(12.0)
                                        .strong()
                                        .color(mc_color));
                                } else {
                                    ui.label(egui::RichText::new("-")
                                        .size(12.0)
                                        .color(egui::Color32::from_gray(100)));
                                }
                            } else {
                                ui.label(egui::RichText::new("-")
                                    .size(12.0)
                                    .color(egui::Color32::from_gray(100)));
                            }
                            
                            // Dev Buy - safe formatting
                            let dev_buy_str = if buy.dev_buy_sol.is_finite() {
                                format!("{:.3}", buy.dev_buy_sol)
                            } else {
                                "0.000".to_string()
                            };
                            ui.label(egui::RichText::new(dev_buy_str)
                                .size(12.0)
                                .color(egui::Color32::from_rgb(255, 215, 100)));
                            
                            // Socials with icon - safe
                            let socials_count = if buy.has_socials {
                                [buy.twitter.as_ref(), buy.website.as_ref(), buy.telegram.as_ref()]
                                    .iter()
                                    .filter(|s| s.is_some())
                                    .count()
                            } else {
                                0
                            };
                            let socials_str = if buy.has_socials {
                                format!("✓ ({})", socials_count)
                            } else {
                                "✗".to_string()
                            };
                            ui.label(egui::RichText::new(socials_str)
                                .size(12.0)
                                .color(if buy.has_socials {
                                    egui::Color32::from_rgb(100, 255, 150)
                                } else {
                                    egui::Color32::from_rgb(255, 120, 120)
                                }));
                            
                            // Transaction link with hover effect - safe formatting
                            let sig_short = format_address_safe(&buy.signature);
                            if ui.add(egui::Button::new(egui::RichText::new(sig_short.clone())
                                    .size(11.0)
                                    .monospace()
                                    .color(egui::Color32::from_rgb(100, 180, 255)))
                                    .fill(egui::Color32::TRANSPARENT)
                                    .stroke(egui::Stroke::NONE)
                                    .small())
                                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                                    .clicked() {
                                if !buy.signature.is_empty() {
                                    let url = format!("https://solscan.io/tx/{}", buy.signature);
                                    if let Err(_e) = open::that(&url) {
                                        // Silently fail
                                    }
                                }
                            }
                            
                            ui.end_row();
                        }
                    });
            });
            
            ui.add_space(10.0);
            
            // Export button
            if ui.button("📥 Export CSV").clicked() {
                // CSV export is already handled by tracker
                ui.label("CSV file saved automatically in project directory.");
            }
        }
    } else {
        ui.label("Tracker not initialized. Enable tracking in settings.");
    }
}

fn format_address(addr: &str) -> String {
    format_address_safe(addr)
}

fn format_address_safe(addr: &str) -> String {
    if addr.is_empty() {
        return "N/A".to_string();
    }
    if addr.len() > 12 {
        let start = addr.chars().take(6).collect::<String>();
        let end = addr.chars().rev().take(6).collect::<String>();
        format!("{}...{}", start, end.chars().rev().collect::<String>())
    } else {
        addr.to_string()
    }
}

