// tabs/positions.rs - Active positions table
#![allow(unused)]

use eframe::egui;
use std::sync::{Arc, RwLock, mpsc};
use crate::accounts::TokenTracker;
use crate::gui::events::BotControl;

pub fn render(ui: &mut egui::Ui, tracker: &Arc<RwLock<Option<TokenTracker>>>, control_tx: &mpsc::Sender<BotControl>) {
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(16.0);
                ui.label(egui::RichText::new("🎯 Active Positions")
                    .size(28.0)
                    .strong()
                    .color(egui::Color32::from_rgb(100, 255, 160)));
                ui.add_space(4.0);
                ui.label(egui::RichText::new("Monitor your currently open trades")
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
                // Use the live tracker directly (don't reload from disk - it would have stale data)
                let active_positions = tracker_ref.get_active_positions();
                let sold_positions = tracker_ref.get_sold_positions();
                
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(format!("Active positions: {} | Sold: {}", active_positions.len(), sold_positions.len()))
                        .size(15.0)
                        .strong()
                        .color(egui::Color32::from_rgb(180, 200, 255)));
                    
                    if !active_positions.is_empty() {
                        ui.add_space(20.0);
                        if ui.add(egui::Button::new(egui::RichText::new("🗑️ Clear Active Positions")
                                .size(13.0)
                                .strong())
                                .fill(egui::Color32::from_rgb(255, 100, 100).linear_multiply(0.2))
                                .stroke(egui::Stroke::new(1.5, egui::Color32::from_rgb(255, 100, 100)))
                                .min_size(egui::vec2(180.0, 32.0))
                                .rounding(egui::Rounding::same(6.0)))
                            .clicked() {
                            // Clear active positions
                            drop(tracker_opt); // Release read lock
                            if let Ok(mut tracker_guard) = tracker.write() {
                                if let Some(tracker_ref) = tracker_guard.as_mut() {
                                    if let Err(e) = tracker_ref.clear_active_positions() {
                                        eprintln!("❌ Failed to clear active positions: {}", e);
                                    } else {
                                        eprintln!("✅ Cleared all active positions");
                                    }
                                }
                            }
                        }
                    }
                });
                ui.add_space(12.0);
                
                // Show active positions
                if active_positions.is_empty() && sold_positions.is_empty() {
                    ui.vertical_centered(|ui| {
                        ui.add_space(40.0);
                        ui.label(egui::RichText::new("No positions.")
                            .size(16.0)
                            .color(egui::Color32::from_rgb(160, 170, 185)));
                    });
                } else {
                    // Active positions section
                    if !active_positions.is_empty() {
                        // Column widths
                        let col_time = 100.0;
                        let col_token = 160.0;
                        let col_mc = 100.0;
                        let col_invested = 110.0;
                        let col_pnl = 140.0;
                        let col_tokens = 100.0;
                        let col_action = 180.0;
                        let row_height = 40.0;
                        let header_color = egui::Color32::from_rgb(220, 230, 245);
                        
                        // Header row
                        ui.horizontal(|ui| {
                        ui.allocate_ui_with_layout(
                            egui::vec2(col_time, row_height),
                            egui::Layout::left_to_right(egui::Align::Center),
                            |ui| { ui.label(egui::RichText::new("Time").size(15.0).strong().color(header_color)); }
                        );
                        ui.allocate_ui_with_layout(
                            egui::vec2(col_token, row_height),
                            egui::Layout::left_to_right(egui::Align::Center),
                            |ui| { ui.label(egui::RichText::new("Token").size(15.0).strong().color(header_color)); }
                        );
                        ui.allocate_ui_with_layout(
                            egui::vec2(col_mc, row_height),
                            egui::Layout::left_to_right(egui::Align::Center),
                            |ui| { ui.label(egui::RichText::new("Entry MC").size(15.0).strong().color(header_color)); }
                        );
                        ui.allocate_ui_with_layout(
                            egui::vec2(col_invested, row_height),
                            egui::Layout::left_to_right(egui::Align::Center),
                            |ui| { ui.label(egui::RichText::new("Invested").size(15.0).strong().color(header_color)); }
                        );
                        ui.allocate_ui_with_layout(
                            egui::vec2(col_pnl, row_height),
                            egui::Layout::left_to_right(egui::Align::Center),
                            |ui| { ui.label(egui::RichText::new("PnL").size(15.0).strong().color(header_color)); }
                        );
                        ui.allocate_ui_with_layout(
                            egui::vec2(col_tokens, row_height),
                            egui::Layout::left_to_right(egui::Align::Center),
                            |ui| { ui.label(egui::RichText::new("Tokens").size(15.0).strong().color(header_color)); }
                        );
                        ui.allocate_ui_with_layout(
                            egui::vec2(col_action, row_height),
                            egui::Layout::left_to_right(egui::Align::Center),
                            |ui| { ui.label(egui::RichText::new("Action").size(15.0).strong().color(header_color)); }
                        );
                        });
                        
                        ui.add_space(4.0);
                        ui.separator();
                        ui.add_space(4.0);
                        
                        // Data rows
                        for (i, pos) in active_positions.iter().rev().enumerate() {
                            let row_bg = if i % 2 == 0 {
                                egui::Color32::TRANSPARENT
                            } else {
                                egui::Color32::from_rgba_unmultiplied(255, 255, 255, 10)
                            };
                            
                            egui::Frame::none()
                                .fill(row_bg)
                                .inner_margin(egui::Margin::symmetric(4.0, 2.0))
                                .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    // Time
                                    ui.allocate_ui_with_layout(
                                        egui::vec2(col_time, row_height),
                                        egui::Layout::left_to_right(egui::Align::Center),
                                        |ui| {
                                            let time_str = pos.timestamp.format("%H:%M:%S").to_string();
                                            ui.label(egui::RichText::new(time_str)
                                                .size(13.0)
                                                .monospace()
                                                .color(egui::Color32::from_rgb(170, 190, 210)));
                                        }
                                    );
                                    
                                    // Token
                                    ui.allocate_ui_with_layout(
                                        egui::vec2(col_token, row_height),
                                        egui::Layout::left_to_right(egui::Align::Center),
                                        |ui| {
                                            let mint_short = format_address_safe(&pos.mint);
                                            if ui.link(egui::RichText::new(mint_short)
                                                .size(13.0)
                                                .monospace()
                                                .color(egui::Color32::from_rgb(160, 210, 255))
                                            ).clicked() {
                                                let _ = open::that(format!("https://solscan.io/token/{}", pos.mint));
                                            }
                                        }
                                    );
                                    
                                    // Entry MC
                                    ui.allocate_ui_with_layout(
                                        egui::vec2(col_mc, row_height),
                                        egui::Layout::left_to_right(egui::Align::Center),
                                        |ui| {
                                            let mc_str = if let Some(mc) = pos.mc_at_entry_usd {
                                                format!("${:.0}", mc)
                                            } else {
                                                "-".to_string()
                                            };
                                            ui.label(egui::RichText::new(mc_str)
                                                .size(13.0)
                                                .strong()
                                                .color(egui::Color32::from_rgb(255, 230, 110)));
                                        }
                                    );
                                    
                                    // Invested
                                    ui.allocate_ui_with_layout(
                                        egui::vec2(col_invested, row_height),
                                        egui::Layout::left_to_right(egui::Align::Center),
                                        |ui| {
                                            ui.label(egui::RichText::new(format!("{:.3} SOL", pos.our_buy_sol))
                                                .size(13.0)
                                                .strong()
                                                .color(egui::Color32::WHITE));
                                        }
                                    );
                                    
                                    // PnL - ULTRA LIVE
                                    ui.allocate_ui_with_layout(
                                        egui::vec2(col_pnl, row_height),
                                        egui::Layout::left_to_right(egui::Align::Center),
                                        |ui| {
                                            if let Some(pnl) = pos.pnl_sol {
                                                let color = if pnl >= 0.0 {
                                                    egui::Color32::from_rgb(100, 255, 100)  // Green for profit
                                                } else {
                                                    egui::Color32::from_rgb(255, 100, 100)  // Red for loss
                                                };
                                                let sign = if pnl >= 0.0 { "+" } else { "" };
                                                
                                                // Show SOL value, and percentage if available
                                                ui.vertical(|ui| {
                                                    ui.label(egui::RichText::new(format!("{}{:.4} SOL", sign, pnl))
                                                        .size(12.0)
                                                        .strong()
                                                        .color(color));
                                                    if let Some(pnl_pct) = pos.pnl_percent {
                                                        ui.label(egui::RichText::new(format!("{}{:.1}%", sign, pnl_pct))
                                                            .size(11.0)
                                                            .color(color.linear_multiply(0.9)));
                                                    } else {
                                                        ui.label(egui::RichText::new("...")
                                                            .size(11.0)
                                                            .color(egui::Color32::from_rgb(160, 170, 185)));
                                                    }
                                                });
                                            } else {
                                                ui.label(egui::RichText::new("...")
                                                    .size(13.0)
                                                    .color(egui::Color32::from_rgb(160, 170, 185)));
                                            }
                                        }
                                    );
                                    
                                    // Tokens
                                    ui.allocate_ui_with_layout(
                                        egui::vec2(col_tokens, row_height),
                                        egui::Layout::left_to_right(egui::Align::Center),
                                        |ui| {
                                            let tokens_str = if let Some(amt) = pos.token_amount {
                                                format!("{:.0}", amt as f64 / 1e6)
                                            } else {
                                                "?".to_string()
                                            };
                                            ui.label(egui::RichText::new(tokens_str)
                                                .size(13.0)
                                                .color(egui::Color32::from_rgb(200, 200, 200)));
                                        }
                                    );
                                    
                                    // Action buttons
                                    ui.allocate_ui_with_layout(
                                        egui::vec2(col_action, row_height),
                                        egui::Layout::left_to_right(egui::Align::Center),
                                        |ui| {
                                            ui.spacing_mut().item_spacing.x = 8.0;
                                            
                                            if ui.button("📈 Chart").clicked() {
                                                let url = format!(
                                                    "https://photon-sol.tinyastro.io/en/lp/{}",
                                                    pos.bonding_curve.as_ref().unwrap_or(&pos.mint)
                                                );
                                                let _ = open::that(url);
                                            }
                                            
                                            if ui.add(
                                                egui::Button::new(
                                                    egui::RichText::new("🚨 SELL")
                                                        .size(13.0)
                                                        .strong()
                                                        .color(egui::Color32::WHITE)
                                                )
                                                .fill(egui::Color32::from_rgb(255, 80, 80))
                                                .rounding(egui::Rounding::same(4.0))
                                            ).clicked() {
                                                eprintln!("🖱️ Sell button clicked for {}", pos.mint);
                                                if let Err(e) = control_tx.send(BotControl::ManualSell(pos.mint.clone())) {
                                                    eprintln!("❌ Failed to send ManualSell command: {}", e);
                                                } else {
                                                    eprintln!("✅ ManualSell command sent for {}", pos.mint);
                                                }
                                            }
                                        }
                                    );
                                });
                            });
                        }
                    }
                    
                    // Sold positions section
                    if !sold_positions.is_empty() {
                        ui.add_space(20.0);
                        ui.separator();
                        ui.add_space(12.0);
                        
                        ui.label(egui::RichText::new(format!("✅ Sold Positions ({})", sold_positions.len()))
                            .size(18.0)
                            .strong()
                            .color(egui::Color32::from_rgb(150, 200, 150)));
                        ui.add_space(8.0);
                        
                        // Column widths (same as active)
                        let col_time = 100.0;
                        let col_token = 160.0;
                        let col_mc = 100.0;
                        let col_invested = 110.0;
                        let col_pnl = 140.0;
                        let col_tokens = 100.0;
                        let col_status = 180.0;
                        let row_height = 40.0;
                        let header_color = egui::Color32::from_rgb(200, 200, 200);
                        
                        // Header row for sold positions
                        ui.horizontal(|ui| {
                            ui.allocate_ui_with_layout(
                                egui::vec2(col_time, row_height),
                                egui::Layout::left_to_right(egui::Align::Center),
                                |ui| { ui.label(egui::RichText::new("Time").size(14.0).strong().color(header_color)); }
                            );
                            ui.allocate_ui_with_layout(
                                egui::vec2(col_token, row_height),
                                egui::Layout::left_to_right(egui::Align::Center),
                                |ui| { ui.label(egui::RichText::new("Token").size(14.0).strong().color(header_color)); }
                            );
                            ui.allocate_ui_with_layout(
                                egui::vec2(col_mc, row_height),
                                egui::Layout::left_to_right(egui::Align::Center),
                                |ui| { ui.label(egui::RichText::new("Entry MC").size(14.0).strong().color(header_color)); }
                            );
                            ui.allocate_ui_with_layout(
                                egui::vec2(col_invested, row_height),
                                egui::Layout::left_to_right(egui::Align::Center),
                                |ui| { ui.label(egui::RichText::new("Invested").size(14.0).strong().color(header_color)); }
                            );
                            ui.allocate_ui_with_layout(
                                egui::vec2(col_pnl, row_height),
                                egui::Layout::left_to_right(egui::Align::Center),
                                |ui| { ui.label(egui::RichText::new("PnL").size(14.0).strong().color(header_color)); }
                            );
                            ui.allocate_ui_with_layout(
                                egui::vec2(col_tokens, row_height),
                                egui::Layout::left_to_right(egui::Align::Center),
                                |ui| { ui.label(egui::RichText::new("Tokens").size(14.0).strong().color(header_color)); }
                            );
                            ui.allocate_ui_with_layout(
                                egui::vec2(col_status, row_height),
                                egui::Layout::left_to_right(egui::Align::Center),
                                |ui| { ui.label(egui::RichText::new("Status").size(14.0).strong().color(header_color)); }
                            );
                        });
                        
                        ui.add_space(4.0);
                        ui.separator();
                        ui.add_space(4.0);
                        
                        // Sold positions rows
                        for (i, pos) in sold_positions.iter().rev().enumerate() {
                            let row_bg = if i % 2 == 0 {
                                egui::Color32::from_rgba_unmultiplied(50, 50, 50, 30)
                            } else {
                                egui::Color32::from_rgba_unmultiplied(40, 40, 40, 20)
                            };
                            
                            egui::Frame::none()
                                .fill(row_bg)
                                .inner_margin(egui::Margin::symmetric(4.0, 2.0))
                                .show(ui, |ui| {
                                    ui.horizontal(|ui| {
                                        // Time
                                        ui.allocate_ui_with_layout(
                                            egui::vec2(col_time, row_height),
                                            egui::Layout::left_to_right(egui::Align::Center),
                                            |ui| {
                                                let time_str = pos.timestamp.format("%H:%M:%S").to_string();
                                                ui.label(egui::RichText::new(time_str)
                                                    .size(12.0)
                                                    .monospace()
                                                    .color(egui::Color32::from_rgb(140, 140, 140)));
                                            }
                                        );
                                        
                                        // Token
                                        ui.allocate_ui_with_layout(
                                            egui::vec2(col_token, row_height),
                                            egui::Layout::left_to_right(egui::Align::Center),
                                            |ui| {
                                                let mint_short = format_address_safe(&pos.mint);
                                                if ui.link(egui::RichText::new(mint_short)
                                                    .size(12.0)
                                                    .monospace()
                                                    .color(egui::Color32::from_rgb(140, 180, 200))
                                                ).clicked() {
                                                    let _ = open::that(format!("https://solscan.io/token/{}", pos.mint));
                                                }
                                            }
                                        );
                                        
                                        // Entry MC
                                        ui.allocate_ui_with_layout(
                                            egui::vec2(col_mc, row_height),
                                            egui::Layout::left_to_right(egui::Align::Center),
                                            |ui| {
                                                let mc_str = if let Some(mc) = pos.mc_at_entry_usd {
                                                    format!("${:.0}", mc)
                                                } else {
                                                    "-".to_string()
                                                };
                                                ui.label(egui::RichText::new(mc_str)
                                                    .size(12.0)
                                                    .color(egui::Color32::from_rgb(180, 180, 180)));
                                            }
                                        );
                                        
                                        // Invested
                                        ui.allocate_ui_with_layout(
                                            egui::vec2(col_invested, row_height),
                                            egui::Layout::left_to_right(egui::Align::Center),
                                            |ui| {
                                                ui.label(egui::RichText::new(format!("{:.3} SOL", pos.our_buy_sol))
                                                    .size(12.0)
                                                    .color(egui::Color32::from_rgb(180, 180, 180)));
                                            }
                                        );
                                        
                                        // PnL
                                        ui.allocate_ui_with_layout(
                                            egui::vec2(col_pnl, row_height),
                                            egui::Layout::left_to_right(egui::Align::Center),
                                            |ui| {
                                                if let Some(pnl) = pos.pnl_sol {
                                                    let color = if pnl >= 0.0 {
                                                        egui::Color32::from_rgb(100, 200, 100)
                                                    } else {
                                                        egui::Color32::from_rgb(200, 100, 100)
                                                    };
                                                    let sign = if pnl >= 0.0 { "+" } else { "" };
                                                    if let Some(pnl_pct) = pos.pnl_percent {
                                                        ui.label(egui::RichText::new(format!("{}{:.2}%", sign, pnl_pct))
                                                            .size(12.0)
                                                            .color(color));
                                                    } else {
                                                        ui.label(egui::RichText::new(format!("{}{:.4} SOL", sign, pnl))
                                                            .size(12.0)
                                                            .color(color));
                                                    }
                                                } else {
                                                    ui.label(egui::RichText::new("-")
                                                        .size(12.0)
                                                        .color(egui::Color32::from_rgb(140, 140, 140)));
                                                }
                                            }
                                        );
                                        
                                        // Tokens
                                        ui.allocate_ui_with_layout(
                                            egui::vec2(col_tokens, row_height),
                                            egui::Layout::left_to_right(egui::Align::Center),
                                            |ui| {
                                                let tokens_str = if let Some(amt) = pos.token_amount {
                                                    format!("{:.0}", amt as f64 / 1e6)
                                                } else {
                                                    "?".to_string()
                                                };
                                                ui.label(egui::RichText::new(tokens_str)
                                                    .size(12.0)
                                                    .color(egui::Color32::from_rgb(140, 140, 140)));
                                            }
                                        );
                                        
                                        // Status - SOLD tag
                                        ui.allocate_ui_with_layout(
                                            egui::vec2(col_status, row_height),
                                            egui::Layout::left_to_right(egui::Align::Center),
                                            |ui| {
                                                ui.add(
                                                    egui::Button::new(
                                                        egui::RichText::new("✅ SOLD")
                                                            .size(12.0)
                                                            .strong()
                                                            .color(egui::Color32::WHITE)
                                                    )
                                                    .fill(egui::Color32::from_rgb(100, 200, 100))
                                                    .rounding(egui::Rounding::same(4.0))
                                                );
                                            }
                                        );
                                    });
                                });
                        }
                    }
                }
            } else {
                ui.label("Tracker not initialized");
            }
        });
}

fn format_address_safe(addr: &str) -> String {
    if addr.len() > 12 {
        let start = &addr[..6];
        let end = &addr[addr.len()-6..];
        format!("{}...{}", start, end)
    } else {
        addr.to_string()
    }
}
