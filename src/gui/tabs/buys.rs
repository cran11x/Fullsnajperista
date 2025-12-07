// tabs/buys.rs - Recent buys table
#![allow(unused)]

use eframe::egui;
use std::sync::{Arc, RwLock};
use crate::accounts::TokenTracker;

pub fn render(ui: &mut egui::Ui, tracker: &Arc<RwLock<Option<TokenTracker>>>) {
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
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
    
            let tracker_opt = match tracker.read() {
                Ok(t) => t,
                Err(e) => {
                    ui.label(format!("Error reading tracker: {}", e));
                    return;
                }
            };
    
            if let Some(tracker_ref) = tracker_opt.as_ref() {
                let reloaded_tracker = crate::accounts::TokenTracker::new().ok();
                let tracker_to_use = reloaded_tracker.as_ref().unwrap_or(tracker_ref);
                
                let total_buys = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    tracker_to_use.total_buys()
                })).unwrap_or(0);
                
                let buys = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    tracker_to_use.get_recent_buys(50)
                })).unwrap_or_else(|_| Vec::new());
                
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(format!("Total buys recorded: {}", total_buys))
                        .size(17.0)
                        .strong()
                        .color(egui::Color32::from_rgb(180, 200, 255)));
                    
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.add(
                            egui::Button::new(
                                egui::RichText::new("🗑️ Clear All")
                                    .size(13.0)
                                    .strong()
                                    .color(egui::Color32::WHITE)
                            )
                            .fill(egui::Color32::from_rgb(200, 80, 80))
                            .rounding(egui::Rounding::same(4.0))
                        ).clicked() {
                            if let Ok(mut tracker_guard) = tracker.write() {
                                if let Some(tracker_ref) = tracker_guard.as_mut() {
                                    if let Err(e) = tracker_ref.clear_all_buys() {
                                        eprintln!("❌ Failed to clear buys: {}", e);
                                    } else {
                                        eprintln!("✅ All buys cleared");
                                    }
                                }
                            }
                        }
                    });
                });
                ui.add_space(20.0);
                
                if buys.is_empty() {
                    ui.vertical_centered(|ui| {
                        ui.add_space(40.0);
                        ui.label(egui::RichText::new("No buys recorded yet.")
                            .size(16.0)
                            .color(egui::Color32::from_rgb(160, 170, 185)));
                    });
                } else {
                    let available_width = ui.available_width();
                    
                    // Responsive: use percentage-based widths with NO minimum constraints
                    let spacing = 12.0;
                    let total_spacing = spacing * 5.0;
                    let usable = (available_width - total_spacing).max(400.0);
                    
                    // Percentage-based columns that scale with window
                    let time_width = usable * 0.10;
                    let token_width = usable * 0.25;
                    let mc_width = usable * 0.12;
                    let dev_buy_width = usable * 0.15;
                    let socials_width = usable * 0.10;
                    let tx_width = usable * 0.18;
                    
                    // Responsive font size
                    let font_size = if available_width > 1000.0 { 16.0 } 
                                   else if available_width > 700.0 { 14.0 } 
                                   else { 12.0 };
                    let header_font = font_size + 2.0;
                    let row_height = font_size * 2.5;
                    
                    let header_color = egui::Color32::from_rgb(220, 230, 245);
                    
                    // Header row
                    ui.horizontal(|ui| {
                        ui.allocate_ui_with_layout(egui::vec2(time_width, row_height), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                            ui.label(egui::RichText::new("Time").size(header_font).strong().color(header_color));
                        });
                        ui.add_space(spacing);
                        ui.allocate_ui_with_layout(egui::vec2(token_width, row_height), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                            ui.label(egui::RichText::new("Token").size(header_font).strong().color(header_color));
                        });
                        ui.add_space(spacing);
                        ui.allocate_ui_with_layout(egui::vec2(mc_width, row_height), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                            ui.label(egui::RichText::new("MC").size(header_font).strong().color(header_color));
                        });
                        ui.add_space(spacing);
                        ui.allocate_ui_with_layout(egui::vec2(dev_buy_width, row_height), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                            ui.label(egui::RichText::new("Dev Buy").size(header_font).strong().color(header_color));
                        });
                        ui.add_space(spacing);
                        ui.allocate_ui_with_layout(egui::vec2(socials_width, row_height), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                            ui.label(egui::RichText::new("Socials").size(header_font).strong().color(header_color));
                        });
                        ui.add_space(spacing);
                        ui.allocate_ui_with_layout(egui::vec2(tx_width, row_height), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                            ui.label(egui::RichText::new("TX").size(header_font).strong().color(header_color));
                        });
                    });
                    
                    ui.add_space(8.0);
                    ui.separator();
                    ui.add_space(8.0);
                    
                    // Data rows
                    for (i, buy) in buys.iter().rev().enumerate() {
                        let row_bg = if i % 2 == 0 {
                            egui::Color32::TRANSPARENT
                        } else {
                            egui::Color32::from_rgba_unmultiplied(255, 255, 255, 8)
                        };
                        
                        egui::Frame::none()
                            .fill(row_bg)
                            .inner_margin(egui::Margin::symmetric(2.0, 4.0))
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    // Time
                                    ui.allocate_ui_with_layout(egui::vec2(time_width, row_height), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                                        let time_str = buy.timestamp.format("%H:%M:%S").to_string();
                                        ui.label(egui::RichText::new(time_str).size(font_size).monospace().color(egui::Color32::from_rgb(170, 190, 210)));
                                    });
                                    ui.add_space(spacing);
                                    
                                    // Token
                                    ui.allocate_ui_with_layout(egui::vec2(token_width, row_height), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                                        let mint_display = format_address_safe(&buy.mint);
                                        if ui.link(egui::RichText::new(mint_display).size(font_size).monospace().color(egui::Color32::from_rgb(160, 210, 255))).clicked() {
                                            let _ = open::that(format!("https://solscan.io/token/{}", buy.mint));
                                        }
                                    });
                                    ui.add_space(spacing);
                                    
                                    // MC
                                    ui.allocate_ui_with_layout(egui::vec2(mc_width, row_height), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                                        if let Some(mc) = buy.mc_at_entry_usd {
                                            if mc.is_finite() && mc >= 0.0 {
                                                let mc_color = if mc > 10000.0 { egui::Color32::from_rgb(100, 255, 160) }
                                                              else if mc > 1000.0 { egui::Color32::from_rgb(255, 230, 110) }
                                                              else { egui::Color32::from_rgb(255, 190, 190) };
                                                ui.label(egui::RichText::new(format!("${:.0}", mc)).size(font_size).strong().color(mc_color));
                                            } else {
                                                ui.label(egui::RichText::new("-").size(font_size).color(egui::Color32::GRAY));
                                            }
                                        } else {
                                            ui.label(egui::RichText::new("-").size(font_size).color(egui::Color32::GRAY));
                                        }
                                    });
                                    ui.add_space(spacing);
                                    
                                    // Dev Buy
                                    ui.allocate_ui_with_layout(egui::vec2(dev_buy_width, row_height), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                                        let dev_str = if buy.dev_buy_sol.is_finite() { format!("{:.3}", buy.dev_buy_sol) } else { "0.000".to_string() };
                                        ui.label(egui::RichText::new(dev_str).size(font_size).strong().color(egui::Color32::from_rgb(255, 220, 0)));
                                    });
                                    ui.add_space(spacing);
                                    
                                    // Socials
                                    ui.allocate_ui_with_layout(egui::vec2(socials_width, row_height), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                                        let (txt, col) = if buy.has_socials {
                                            let cnt = [buy.twitter.as_ref(), buy.website.as_ref(), buy.telegram.as_ref()].iter().filter(|s| s.is_some()).count();
                                            (format!("✓{}", cnt), egui::Color32::from_rgb(100, 255, 160))
                                        } else {
                                            ("✗".to_string(), egui::Color32::from_rgb(255, 130, 130))
                                        };
                                        ui.label(egui::RichText::new(txt).size(font_size).strong().color(col));
                                    });
                                    ui.add_space(spacing);
                                    
                                    // TX
                                    ui.allocate_ui_with_layout(egui::vec2(tx_width, row_height), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                                        let sig_short = format_address_safe(&buy.signature);
                                        if ui.add(egui::Button::new(egui::RichText::new(sig_short).size(font_size - 1.0).monospace().color(egui::Color32::from_rgb(120, 200, 255)))
                                            .fill(egui::Color32::TRANSPARENT)
                                            .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(120, 200, 255).linear_multiply(0.5)))
                                            .rounding(egui::Rounding::same(4.0))
                                        ).clicked() {
                                            if !buy.signature.is_empty() {
                                                let _ = open::that(format!("https://solscan.io/tx/{}", buy.signature));
                                            }
                                        }
                                    });
                                });
                            });
                    }
                    
                    ui.add_space(20.0);
                    
                    if ui.add(egui::Button::new(egui::RichText::new("📥 Export CSV").size(14.0).strong())
                        .fill(egui::Color32::from_rgb(100, 200, 255).linear_multiply(0.2))
                        .stroke(egui::Stroke::new(2.0, egui::Color32::from_rgb(100, 200, 255)))
                        .min_size(egui::vec2(140.0, 36.0))
                        .rounding(egui::Rounding::same(6.0))
                    ).clicked() {
                        // Export handled by tracker
                    }
                }
            } else {
                ui.vertical_centered(|ui| {
                    ui.add_space(40.0);
                    ui.label(egui::RichText::new("Tracker not initialized.")
                        .size(16.0)
                        .color(egui::Color32::from_rgb(160, 170, 185)));
                });
            }
        });
}

fn format_address_safe(addr: &str) -> String {
    if addr.is_empty() { return "N/A".to_string(); }
    if addr.len() > 12 {
        format!("{}...{}", &addr[..6], &addr[addr.len()-4..])
    } else {
        addr.to_string()
    }
}
