// tabs/buys.rs - Recent buys table
#![allow(unused)]

use eframe::egui;
use std::sync::{Arc, RwLock};
use crate::accounts::TokenTracker;

pub fn render(ui: &mut egui::Ui, tracker: &Arc<RwLock<Option<TokenTracker>>>) {
    ui.vertical_centered(|ui| {
        ui.add_space(12.0);
        ui.label(egui::RichText::new("💰 Recent Buys")
            .size(26.0)
            .strong()
            .color(egui::Color32::from_rgb(255, 220, 0)));
        ui.label(egui::RichText::new("Track your successful token purchases")
            .size(13.0)
            .color(egui::Color32::from_rgb(160, 170, 185)));
    });
    ui.add_space(20.0);
    
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
        
        // Enhanced total buys count display
        ui.label(egui::RichText::new(format!("Total buys recorded: {}", total_buys))
            .size(13.0)
            .strong()
            .color(egui::Color32::from_rgb(180, 200, 255)));
        ui.add_space(8.0);
        
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
                egui::Grid::new("buys_grid")
                    .num_columns(6)
                    .spacing([20.0, 8.0])
                    .striped(true)
                    .min_row_height(38.0)
                    .show(ui, |ui| {
                        // Enhanced header with premium styling
                        ui.add_space(10.0);
                        ui.label(egui::RichText::new("Time")
                            .size(14.0)
                            .strong()
                            .color(egui::Color32::from_rgb(220, 230, 245)));
                        ui.label(egui::RichText::new("Token")
                            .size(14.0)
                            .strong()
                            .color(egui::Color32::from_rgb(220, 230, 245)));
                        ui.label(egui::RichText::new("MC ($)")
                            .size(14.0)
                            .strong()
                            .color(egui::Color32::from_rgb(220, 230, 245)));
                        ui.label(egui::RichText::new("Dev Buy (SOL)")
                            .size(14.0)
                            .strong()
                            .color(egui::Color32::from_rgb(220, 230, 245)));
                        ui.label(egui::RichText::new("Socials")
                            .size(14.0)
                            .strong()
                            .color(egui::Color32::from_rgb(220, 230, 245)));
                        ui.label(egui::RichText::new("Transaction")
                            .size(14.0)
                            .strong()
                            .color(egui::Color32::from_rgb(220, 230, 245)));
                        ui.end_row();
                        
                        // Enhanced rows with premium styling and hover effects
                        for buy in buys.iter().rev() {
                            // Enhanced time display
                            let time_str = buy.timestamp.format("%H:%M:%S").to_string();
                            ui.label(egui::RichText::new(time_str)
                                .size(13.0)
                                .color(egui::Color32::from_rgb(170, 190, 210))
                                .monospace());
                            
                            // Enhanced token display
                            let mint_display = format_address_safe(&buy.mint);
                            ui.label(egui::RichText::new(mint_display)
                                .size(13.0)
                                .monospace()
                                .color(egui::Color32::from_rgb(160, 210, 255)));
                            
                            // Enhanced MC with better color coding
                            if let Some(mc) = buy.mc_at_entry_usd {
                                if mc.is_finite() && mc >= 0.0 {
                                    let mc_color = if mc > 10000.0 {
                                        egui::Color32::from_rgb(100, 255, 160)
                                    } else if mc > 1000.0 {
                                        egui::Color32::from_rgb(255, 230, 110)
                                    } else {
                                        egui::Color32::from_rgb(255, 190, 190)
                                    };
                                    ui.label(egui::RichText::new(format!("${:.0}", mc))
                                        .size(13.0)
                                        .strong()
                                        .color(mc_color));
                                } else {
                                    ui.label(egui::RichText::new("-")
                                        .size(13.0)
                                        .color(egui::Color32::from_rgb(120, 130, 150)));
                                }
                            } else {
                                ui.label(egui::RichText::new("-")
                                    .size(13.0)
                                    .color(egui::Color32::from_rgb(120, 130, 150)));
                            }
                            
                            // Enhanced dev buy display
                            let dev_buy_str = if buy.dev_buy_sol.is_finite() {
                                format!("{:.3}", buy.dev_buy_sol)
                            } else {
                                "0.000".to_string()
                            };
                            ui.label(egui::RichText::new(dev_buy_str)
                                .size(13.0)
                                .strong()
                                .color(egui::Color32::from_rgb(255, 220, 0)));
                            
                            // Enhanced socials display
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
                                .size(13.0)
                                .strong()
                                .color(if buy.has_socials {
                                    egui::Color32::from_rgb(100, 255, 160)
                                } else {
                                    egui::Color32::from_rgb(255, 130, 130)
                                }));
                            
                            // Enhanced transaction link with better hover effect
                            let sig_short = format_address_safe(&buy.signature);
                            let sig_button = ui.add(egui::Button::new(egui::RichText::new(sig_short.clone())
                                    .size(12.0)
                                    .monospace()
                                    .color(egui::Color32::from_rgb(120, 200, 255)))
                                    .fill(egui::Color32::TRANSPARENT)
                                    .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(120, 200, 255).linear_multiply(0.3)))
                                    .rounding(egui::Rounding::same(4.0)));
                            
                            if sig_button.hovered() {
                                ui.painter().rect_filled(
                                    sig_button.rect,
                                    4.0,
                                    egui::Color32::from_rgb(120, 200, 255).linear_multiply(0.15),
                                );
                            }
                            
                            if sig_button.on_hover_cursor(egui::CursorIcon::PointingHand)
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
    if addr.len() > 12 {
        let start = addr.chars().take(6).collect::<String>();
        let end = addr.chars().rev().take(6).collect::<String>();
        format!("{}...{}", start, end.chars().rev().collect::<String>())
    } else {
        addr.to_string()
    }
}

