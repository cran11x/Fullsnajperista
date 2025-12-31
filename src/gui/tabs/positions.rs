// tabs/positions.rs - Active positions table
#![allow(unused)]

use eframe::egui;
use std::sync::{Arc, RwLock, mpsc};
use std::collections::HashMap;
use std::rc::Rc;
use std::cell::RefCell;
use crate::accounts::{TokenTracker, TokenBuy};
use crate::gui::events::BotControl;
use crate::utils::{format_sol_with_usd_3dec, format_pnl_with_usd, format_mc_sol_with_usd};
use serde_json;
use chrono;

pub fn render(ui: &mut egui::Ui, tracker: &Arc<RwLock<Option<TokenTracker>>>, control_tx: &mpsc::Sender<BotControl>) {
    // Get or create state for tracking open info popups
    let mut open_popups: HashMap<String, bool> = ui.data_mut(|data| {
        data.get_temp(egui::Id::new("positions_info_popups"))
            .unwrap_or_default()
    });
    
    // Wrap in Rc<RefCell<>> to make it accessible in closure
    let open_popups_rc = Rc::new(RefCell::new(open_popups));
    
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(16.0);
                ui.label(egui::RichText::new("🎯 Active Positions")
                    .size(28.0)
                    .strong()
                    .color(egui::Color32::from_rgb(255, 50, 50))); // Crvena
                ui.add_space(4.0);
                ui.label(egui::RichText::new("Monitor your currently open trades")
                    .size(14.0)
                    .color(egui::Color32::from_rgb(160, 160, 170))); // Siva
            });
            ui.add_space(24.0);
            
            // ✅ FIX: Use try_read() to avoid blocking GUI thread if bot is holding lock
            let tracker_opt = match tracker.try_read() {
                Ok(t) => t,
                Err(_) => {
                    ui.vertical_centered(|ui| {
                        ui.add_space(60.0);
                        ui.label(egui::RichText::new("Loading positions...")
                            .size(16.0)
                            .color(egui::Color32::from_rgb(160, 170, 185)));
                    });
                    return;
                }
            };
            
            if let Some(tracker_ref) = tracker_opt.as_ref() {
                // Use the live tracker directly (don't reload from disk - it would have stale data)
                let active_positions = tracker_ref.get_active_positions();
                let sold_positions = tracker_ref.get_sold_positions();
                
                // ✅ FIX: Removed excessive debug logging that was causing performance issues
                
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
                            // ✅ FIX: Use try_write() to avoid blocking
                            if let Ok(mut tracker_guard) = tracker.try_write() {
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
                                            let mc_str = if let Some(mc_sol) = pos.mc_at_entry_sol {
                                                use crate::utils::{format_mc_sol_with_usd};
                                                format_mc_sol_with_usd(mc_sol)
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
                                            use crate::utils::format_sol_with_usd_3dec;
                                            ui.label(egui::RichText::new(format_sol_with_usd_3dec(pos.our_buy_sol))
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
                                            // DEBUG logging removed - was causing crashes due to unsafe string slicing
                                            if let Some(pnl) = pos.pnl_sol {
                                                let color = if pnl >= 0.0 {
                                                    egui::Color32::from_rgb(100, 255, 100)  // Green for profit
                                                } else {
                                                    egui::Color32::from_rgb(255, 100, 100)  // Red for loss
                                                };
                                                
                                                // Show SOL value with USD, and percentage if available
                                                ui.vertical(|ui| {
                                                    use crate::utils::format_pnl_with_usd;
                                                    ui.label(egui::RichText::new(format_pnl_with_usd(pnl))
                                                        .size(12.0)
                                                        .strong()
                                                        .color(color));
                                                    if let Some(pnl_pct) = pos.pnl_percent {
                                                        // ✅ FIX: Calculate sign from pnl_pct, not pnl
                                                        // This prevents sign mismatch when values update rapidly
                                                        // and ensures consistent display even if calculation method switches
                                                        let sign = if pnl_pct >= 0.0 { "+" } else { "" };
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
                                                // ✅ FIX: Show better message when PnL is not available
                                                // Determine reason why PnL is not available
                                                let status_text = if pos.bonding_curve.is_none() {
                                                    "No BC"
                                                } else if pos.token_amount.is_none() && pos.token_price_sol.is_none() {
                                                    "Calc..."
                                                } else if pos.current_price_sol.is_some() && pos.current_price_sol.unwrap_or(0.0) <= 0.0 {
                                                    "Migrated"
                                                } else {
                                                    "Calc..."
                                                };
                                                
                                                ui.label(egui::RichText::new(status_text)
                                                    .size(12.0)
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
                                            ui.spacing_mut().item_spacing.x = 6.0;
                                            
                                            if ui.button("📈 Chart").clicked() {
                                                let url = format!(
                                                    "https://photon-sol.tinyastro.io/en/lp/{}",
                                                    pos.bonding_curve.as_ref().unwrap_or(&pos.mint)
                                                );
                                                let _ = open::that(url);
                                            }
                                            
                                            if ui.button("🔍 Solscan").clicked() {
                                                let _ = open::that(format!("https://solscan.io/token/{}", pos.mint));
                                            }
                                            
                                            if ui.button("📊 Axiom").clicked() {
                                                // Axiom uses bonding curve address
                                                if let Some(bonding_curve) = &pos.bonding_curve {
                                                    let _ = open::that(format!("https://axiom.trade/meme/{}?chain=sol", bonding_curve));
                                                }
                                            }
                                            
                                            // Info button - make it more visible
                                            let popup_id = format!("token_info_{}", pos.mint);
                                            let is_open = {
                                                let popups = open_popups_rc.borrow();
                                                popups.get(&popup_id).copied().unwrap_or(false)
                                            };
                                            let button_color = if is_open {
                                                egui::Color32::from_rgb(100, 200, 255)
                                            } else {
                                                egui::Color32::from_rgb(200, 200, 200)
                                            };
                                            // Info button - make it more visible with better styling
                                            if ui.add(
                                                egui::Button::new(egui::RichText::new("ℹ️ Info").size(12.0).strong())
                                                    .fill(if is_open {
                                                        egui::Color32::from_rgb(100, 200, 255).linear_multiply(0.3)
                                                    } else {
                                                        egui::Color32::from_rgb(50, 50, 60)
                                                    })
                                                    .stroke(egui::Stroke::new(1.5, button_color))
                                                    .min_size(egui::vec2(60.0, 28.0))
                                                    .rounding(egui::Rounding::same(4.0))
                                            ).clicked() {
                                                let mut popups = open_popups_rc.borrow_mut();
                                                popups.insert(popup_id.clone(), !is_open);
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
                        
                        // Column widths (same as active, but with action column instead of status)
                        let col_time = 100.0;
                        let col_token = 160.0;
                        let col_mc = 100.0;
                        let col_invested = 110.0;
                        let col_pnl = 140.0;
                        let col_tokens = 100.0;
                        let col_action = 180.0;
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
                                egui::vec2(col_action, row_height),
                                egui::Layout::left_to_right(egui::Align::Center),
                                |ui| { ui.label(egui::RichText::new("Action").size(14.0).strong().color(header_color)); }
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
                                                ui.spacing_mut().item_spacing.x = 6.0;
                                                let mint_short = format_address_safe(&pos.mint);
                                                if ui.link(egui::RichText::new(mint_short)
                                                    .size(12.0)
                                                    .monospace()
                                                    .color(egui::Color32::from_rgb(140, 180, 200))
                                                ).clicked() {
                                                    let _ = open::that(format!("https://solscan.io/token/{}", pos.mint));
                                                }
                                                if ui.small_button("📊").clicked() {
                                                    // Axiom uses bonding curve address
                                                    if let Some(bonding_curve) = &pos.bonding_curve {
                                                        let _ = open::that(format!("https://axiom.trade/meme/{}?chain=sol", bonding_curve));
                                                    }
                                                }
                                            }
                                        );
                                        
                                        // Entry MC
                                        ui.allocate_ui_with_layout(
                                            egui::vec2(col_mc, row_height),
                                            egui::Layout::left_to_right(egui::Align::Center),
                                            |ui| {
                                                let mc_str = if let Some(mc_sol) = pos.mc_at_entry_sol {
                                                    use crate::utils::format_mc_sol_with_usd;
                                                    format_mc_sol_with_usd(mc_sol)
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
                                                use crate::utils::format_sol_with_usd_3dec;
                                                ui.label(egui::RichText::new(format_sol_with_usd_3dec(pos.our_buy_sol))
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
                                                    // ✅ FIX: Calculate sign from pnl_pct, not pnl
                                                    if let Some(pnl_pct) = pos.pnl_percent {
                                                        let sign = if pnl_pct >= 0.0 { "+" } else { "" };
                                                        ui.label(egui::RichText::new(format!("{}{:.2}%", sign, pnl_pct))
                                                            .size(12.0)
                                                            .color(color));
                                                    } else {
                                                        use crate::utils::format_pnl_with_usd;
                                                        ui.label(egui::RichText::new(format_pnl_with_usd(pnl))
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
                                        
                                        // Action buttons for sold positions
                                        ui.allocate_ui_with_layout(
                                            egui::vec2(col_action, row_height),
                                            egui::Layout::left_to_right(egui::Align::Center),
                                            |ui| {
                                                ui.spacing_mut().item_spacing.x = 6.0;
                                                
                                                // Axiom button
                                                if ui.button("📊 Axiom").clicked() {
                                                    // Axiom uses bonding curve address
                                                    if let Some(bonding_curve) = &pos.bonding_curve {
                                                        let _ = open::that(format!("https://axiom.trade/meme/{}?chain=sol", bonding_curve));
                                                    }
                                                }
                                                
                                                // Info button - same as active positions
                                                let popup_id = format!("token_info_{}", pos.mint);
                                                let is_open = {
                                                    let popups = open_popups_rc.borrow();
                                                    popups.get(&popup_id).copied().unwrap_or(false)
                                                };
                                                let button_color = if is_open {
                                                    egui::Color32::from_rgb(100, 200, 255)
                                                } else {
                                                    egui::Color32::from_rgb(200, 200, 200)
                                                };
                                                // Info button - make it more visible with better styling
                                                if ui.add(
                                                    egui::Button::new(egui::RichText::new("ℹ️ Info").size(12.0).strong())
                                                        .fill(if is_open {
                                                            egui::Color32::from_rgb(100, 200, 255).linear_multiply(0.3)
                                                        } else {
                                                            egui::Color32::from_rgb(50, 50, 60)
                                                        })
                                                        .stroke(egui::Stroke::new(1.5, button_color))
                                                        .min_size(egui::vec2(60.0, 28.0))
                                                        .rounding(egui::Rounding::same(4.0))
                                                ).clicked() {
                                                    let mut popups = open_popups_rc.borrow_mut();
                                                    popups.insert(popup_id.clone(), !is_open);
                                                }
                                                
                                                // SOLD tag
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
    
    // Render info popup windows for all open popups
    // Note: We need to get ctx from the outer ui, not from ScrollArea
    let ctx = ui.ctx().clone();
    if let Ok(tracker_opt) = tracker.try_read() {
        if let Some(tracker_ref) = tracker_opt.as_ref() {
            let active_positions = tracker_ref.get_active_positions();
            let sold_positions = tracker_ref.get_sold_positions();
            let all_positions: Vec<_> = active_positions.iter().chain(sold_positions.iter()).collect();
            
            for pos in all_positions {
                let popup_id = format!("token_info_{}", pos.mint);
                // Check if popup should be rendered
                let should_render = {
                    let popups = open_popups_rc.borrow();
                    popups.get(&popup_id).copied().unwrap_or(false)
                };
                if should_render {
                    render_token_info_window(&ctx, pos, &popup_id, &mut open_popups_rc.borrow_mut());
                }
            }
        }
    }
    
    // ✅ FIX: Save popup state back to ui.data so it persists between frames
    let final_popups = open_popups_rc.borrow().clone();
    ui.data_mut(|data| {
        data.insert_temp(egui::Id::new("positions_info_popups"), final_popups);
    });
}

fn render_token_info_window(
    ctx: &egui::Context,
    pos: &TokenBuy,
    popup_id: &str,
    open_popups: &mut HashMap<String, bool>,
) {
    let window_id = egui::Id::new(popup_id);
    // ✅ FIX: Get is_open from state, default to true if not set (first time opening)
    let mut is_open = open_popups.get(popup_id).copied().unwrap_or(true);
    
    egui::Window::new(format!("Token Info: {}", format_address_safe(&pos.mint)))
        .id(window_id)
        .open(&mut is_open)
        .resizable(true)
        .collapsible(true)
        .default_size(egui::vec2(600.0, 700.0))
        .show(ctx, |ui| {
            ui.set_max_width(600.0);
            
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing = egui::vec2(12.0, 8.0);
                    
                    // Basic Info Section
                    ui.group(|ui| {
                        ui.heading(egui::RichText::new("📋 Basic Info")
                            .size(18.0)
                            .strong()
                            .color(egui::Color32::from_rgb(255, 70, 70)));
                        ui.add_space(8.0);
                        
                        egui::Grid::new("basic_info_grid")
                            .spacing([16.0, 6.0])
                            .show(ui, |ui| {
                                ui.label(egui::RichText::new("Mint Address:").strong());
                                if ui.link(egui::RichText::new(format_address_safe(&pos.mint))
                                    .monospace()
                                    .color(egui::Color32::from_rgb(160, 210, 255))
                                ).clicked() {
                                    let _ = open::that(format!("https://solscan.io/token/{}", pos.mint));
                                }
                                ui.end_row();
                                
                                ui.label(egui::RichText::new("Creator:").strong());
                                if ui.link(egui::RichText::new(format_address_safe(&pos.creator))
                                    .monospace()
                                    .color(egui::Color32::from_rgb(160, 210, 255))
                                ).clicked() {
                                    let _ = open::that(format!("https://solscan.io/account/{}", pos.creator));
                                }
                                ui.end_row();
                                
                                ui.label(egui::RichText::new("Timestamp:").strong());
                                ui.label(pos.timestamp.format("%Y-%m-%d %H:%M:%S UTC").to_string());
                                ui.end_row();
                                
                                ui.label(egui::RichText::new("Detection Method:").strong());
                                ui.label(&pos.detection_method);
                                ui.end_row();
                                
                                ui.label(egui::RichText::new("Token Number:").strong());
                                ui.label(pos.token_number.to_string());
                                ui.end_row();
                            });
                    });
                    
                    ui.add_space(12.0);
                    
                    // Buy Information Section
                    ui.group(|ui| {
                        ui.heading(egui::RichText::new("💰 Buy Information")
                            .size(18.0)
                            .strong()
                            .color(egui::Color32::from_rgb(255, 70, 70)));
                        ui.add_space(8.0);
                        
                        egui::Grid::new("buy_info_grid")
                            .spacing([16.0, 6.0])
                            .show(ui, |ui| {
                                ui.label(egui::RichText::new("Dev Buy:").strong());
                                ui.label(format_sol_with_usd_3dec(pos.dev_buy_sol));
                                ui.end_row();
                                
                                ui.label(egui::RichText::new("Our Buy:").strong());
                                ui.label(format_sol_with_usd_3dec(pos.our_buy_sol));
                                ui.end_row();
                                
                                if let Some(fees) = pos.buy_fees_sol {
                                    ui.label(egui::RichText::new("Buy Fees:").strong());
                                    ui.label(format!("{:.6} SOL", fees));
                                    ui.end_row();
                                }
                            });
                    });
                    
                    ui.add_space(12.0);
                    
                    // Market Cap Section
                    ui.group(|ui| {
                        ui.heading(egui::RichText::new("📊 Market Cap")
                            .size(18.0)
                            .strong()
                            .color(egui::Color32::from_rgb(255, 70, 70)));
                        ui.add_space(8.0);
                        
                        egui::Grid::new("mc_info_grid")
                            .spacing([16.0, 6.0])
                            .show(ui, |ui| {
                                if let Some(mc) = pos.mc_at_detection_sol {
                                    ui.label(egui::RichText::new("MC at Detection:").strong());
                                    ui.label(format_mc_sol_with_usd(mc));
                                    ui.end_row();
                                }
                                
                                if let Some(mc) = pos.mc_at_entry_sol {
                                    ui.label(egui::RichText::new("MC at Entry:").strong());
                                    ui.label(format_mc_sol_with_usd(mc));
                                    ui.end_row();
                                }
                                
                                if let Some(peak_mc) = pos.peak_mc_sol {
                                    ui.label(egui::RichText::new("Peak MC:").strong());
                                    ui.label(format_mc_sol_with_usd(peak_mc));
                                    ui.end_row();
                                }
                            });
                    });
                    
                    ui.add_space(12.0);
                    
                    // Price Information Section
                    ui.group(|ui| {
                        ui.heading(egui::RichText::new("💵 Price Information")
                            .size(18.0)
                            .strong()
                            .color(egui::Color32::from_rgb(255, 70, 70)));
                        ui.add_space(8.0);
                        
                        egui::Grid::new("price_info_grid")
                            .spacing([16.0, 6.0])
                            .show(ui, |ui| {
                                if let Some(price) = pos.token_price_sol {
                                    ui.label(egui::RichText::new("Token Price (Detection):").strong());
                                    ui.label(format!("{:.10} SOL", price));
                                    ui.end_row();
                                }
                                
                                if let Some(price) = pos.current_price_sol {
                                    ui.label(egui::RichText::new("Current Price:").strong());
                                    ui.label(format!("{:.10} SOL", price));
                                    ui.end_row();
                                }
                            });
                    });
                    
                    ui.add_space(12.0);
                    
                    // Position Details Section
                    ui.group(|ui| {
                        ui.heading(egui::RichText::new("📦 Position Details")
                            .size(18.0)
                            .strong()
                            .color(egui::Color32::from_rgb(255, 70, 70)));
                        ui.add_space(8.0);
                        
                        egui::Grid::new("position_info_grid")
                            .spacing([16.0, 6.0])
                            .show(ui, |ui| {
                                if let Some(amount) = pos.token_amount {
                                    ui.label(egui::RichText::new("Token Amount:").strong());
                                    ui.label(format!("{:.0}", amount as f64 / 1e6));
                                    ui.end_row();
                                }
                                
                                if let Some(value) = pos.current_value_sol {
                                    ui.label(egui::RichText::new("Current Value:").strong());
                                    ui.label(format_sol_with_usd_3dec(value));
                                    ui.end_row();
                                }
                                
                                if let Some(ata) = &pos.user_token_account {
                                    ui.label(egui::RichText::new("Token Account:").strong());
                                    ui.label(egui::RichText::new(format_address_safe(ata))
                                        .monospace()
                                        .color(egui::Color32::from_rgb(200, 200, 200)));
                                    ui.end_row();
                                }
                                
                                if let Some(bc) = &pos.bonding_curve {
                                    ui.label(egui::RichText::new("Bonding Curve:").strong());
                                    if ui.link(egui::RichText::new(format_address_safe(bc))
                                        .monospace()
                                        .color(egui::Color32::from_rgb(160, 210, 255))
                                    ).clicked() {
                                        let _ = open::that(format!("https://solscan.io/account/{}", bc));
                                    }
                                    ui.end_row();
                                }
                            });
                    });
                    
                    ui.add_space(12.0);
                    
                    // PnL Information Section
                    ui.group(|ui| {
                        ui.heading(egui::RichText::new("📈 PnL Information")
                            .size(18.0)
                            .strong()
                            .color(egui::Color32::from_rgb(255, 70, 70)));
                        ui.add_space(8.0);
                        
                        egui::Grid::new("pnl_info_grid")
                            .spacing([16.0, 6.0])
                            .show(ui, |ui| {
                                if let Some(pnl) = pos.pnl_sol {
                                    let color = if pnl >= 0.0 {
                                        egui::Color32::from_rgb(100, 255, 100)
                                    } else {
                                        egui::Color32::from_rgb(255, 100, 100)
                                    };
                                    ui.label(egui::RichText::new("PnL:").strong());
                                    ui.label(egui::RichText::new(format_pnl_with_usd(pnl))
                                        .color(color)
                                        .strong());
                                    ui.end_row();
                                }
                                
                                if let Some(pnl_pct) = pos.pnl_percent {
                                    let color = if pnl_pct >= 0.0 {
                                        egui::Color32::from_rgb(100, 255, 100)
                                    } else {
                                        egui::Color32::from_rgb(255, 100, 100)
                                    };
                                    ui.label(egui::RichText::new("PnL %:").strong());
                                    ui.label(egui::RichText::new(format!("{:.2}%", pnl_pct))
                                        .color(color)
                                        .strong());
                                    ui.end_row();
                                }
                                
                                if let Some(peak_pnl) = pos.peak_pnl_percent {
                                    ui.label(egui::RichText::new("Peak PnL %:").strong());
                                    ui.label(format!("{:.2}%", peak_pnl));
                                    ui.end_row();
                                }
                                
                                if let Some(update_time) = pos.last_pnl_update {
                                    ui.label(egui::RichText::new("Last PnL Update:").strong());
                                    ui.label(update_time.format("%H:%M:%S").to_string());
                                    ui.end_row();
                                }
                            });
                    });
                    
                    ui.add_space(12.0);
                    
                    // Socials Section
                    ui.group(|ui| {
                        ui.heading(egui::RichText::new("🔗 Socials")
                            .size(18.0)
                            .strong()
                            .color(egui::Color32::from_rgb(255, 70, 70)));
                        ui.add_space(8.0);
                        
                        egui::Grid::new("socials_info_grid")
                            .spacing([16.0, 6.0])
                            .show(ui, |ui| {
                                if let Some(twitter) = &pos.twitter {
                                    if !twitter.is_empty() {
                                        ui.label(egui::RichText::new("Twitter:").strong());
                                        let twitter_display = if let Some(ref twitter_type) = pos.twitter_type {
                                            format!("{} ({})", twitter, twitter_type)
                                        } else {
                                            twitter.clone()
                                        };
                                        if ui.link(egui::RichText::new(twitter_display.clone())
                                            .color(egui::Color32::from_rgb(160, 210, 255))
                                        ).clicked() {
                                            let _ = open::that(twitter.clone());
                                        }
                                        ui.end_row();
                                    }
                                }
                                
                                if let Some(website) = &pos.website {
                                    if !website.is_empty() {
                                        ui.label(egui::RichText::new("Website:").strong());
                                        if ui.link(egui::RichText::new(website.clone())
                                            .color(egui::Color32::from_rgb(160, 210, 255))
                                        ).clicked() {
                                            let _ = open::that(website.clone());
                                        }
                                        ui.end_row();
                                    }
                                }
                                
                                if let Some(telegram) = &pos.telegram {
                                    if !telegram.is_empty() {
                                        ui.label(egui::RichText::new("Telegram:").strong());
                                        if ui.link(egui::RichText::new(telegram.clone())
                                            .color(egui::Color32::from_rgb(160, 210, 255))
                                        ).clicked() {
                                            let _ = open::that(telegram.clone());
                                        }
                                        ui.end_row();
                                    }
                                }
                                
                                if let Some(discord) = &pos.discord {
                                    if !discord.is_empty() {
                                        ui.label(egui::RichText::new("Discord:").strong());
                                        if ui.link(egui::RichText::new(discord.clone())
                                            .color(egui::Color32::from_rgb(160, 210, 255))
                                        ).clicked() {
                                            let _ = open::that(discord.clone());
                                        }
                                        ui.end_row();
                                    }
                                }
                                
                                if pos.twitter.is_none() && pos.website.is_none() && pos.telegram.is_none() && pos.discord.is_none() {
                                    ui.label(egui::RichText::new("Socials:").strong());
                                    ui.label(egui::RichText::new("None")
                                        .color(egui::Color32::from_rgb(160, 160, 160)));
                                    ui.end_row();
                                }
                            });
                    });
                    
                    ui.add_space(12.0);
                    
                    // Advanced Section
                    ui.group(|ui| {
                        ui.heading(egui::RichText::new("⚙️ Advanced")
                            .size(18.0)
                            .strong()
                            .color(egui::Color32::from_rgb(255, 70, 70)));
                        ui.add_space(8.0);
                        
                        egui::Grid::new("advanced_info_grid")
                            .spacing([16.0, 6.0])
                            .show(ui, |ui| {
                                ui.label(egui::RichText::new("Creator Token Count:").strong());
                                ui.label(pos.creator_token_count.to_string());
                                ui.end_row();
                                
                                ui.label(egui::RichText::new("Breakeven Mode:").strong());
                                ui.label(if pos.breakeven_mode_active { "Active" } else { "Inactive" });
                                ui.end_row();
                                
                                ui.label(egui::RichText::new("Partial Sells:").strong());
                                ui.label(pos.partial_sell_count.to_string());
                                ui.end_row();
                                
                                if pos.total_sold_percent > 0.0 {
                                    ui.label(egui::RichText::new("Total Sold %:").strong());
                                    ui.label(format!("{:.1}%", pos.total_sold_percent));
                                    ui.end_row();
                                }
                                
                                if !pos.executed_sell_rules.is_empty() {
                                    ui.label(egui::RichText::new("Executed Rules:").strong());
                                    ui.label(pos.executed_sell_rules.join(", "));
                                    ui.end_row();
                                }
                                
                                ui.label(egui::RichText::new("Status:").strong());
                                ui.label(if pos.sold { "Sold" } else { "Active" });
                                ui.end_row();
                                
                                if let Some(sell_sig) = &pos.sell_signature {
                                    ui.label(egui::RichText::new("Sell Signature:").strong());
                                    if ui.link(egui::RichText::new(format_address_safe(sell_sig))
                                        .monospace()
                                        .color(egui::Color32::from_rgb(160, 210, 255))
                                    ).clicked() {
                                        let _ = open::that(format!("https://solscan.io/tx/{}", sell_sig));
                                    }
                                    ui.end_row();
                                }
                            });
                    });
                });
        });
    
    // Update state based on is_open value (set by egui when window is closed)
    if !is_open {
        open_popups.remove(popup_id);
    } else {
        open_popups.insert(popup_id.to_string(), true);
    }
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
