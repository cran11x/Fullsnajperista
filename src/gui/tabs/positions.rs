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
        // Try to reload tracker to get latest data
        let reloaded_tracker = crate::accounts::TokenTracker::new().ok();
        let tracker_to_use = reloaded_tracker.as_ref().unwrap_or(tracker_ref);
        
        // Note: Cleanup is done in background by monitor_positions task
        // We just display the current active positions
        let active_positions = tracker_to_use.get_active_positions();
        
        ui.label(egui::RichText::new(format!("Active positions: {}", active_positions.len()))
            .size(15.0)
            .strong()
            .color(egui::Color32::from_rgb(180, 200, 255)));
        ui.add_space(12.0);
        
        if active_positions.is_empty() {
            ui.vertical_centered(|ui| {
                ui.add_space(40.0);
                ui.label(egui::RichText::new("No active positions.")
                    .size(16.0)
                    .color(egui::Color32::from_rgb(160, 170, 185)));
            });
        } else {
            // Table handles its own layout, just direct render
            // Improved grid with proper column widths and responsive spacing
            let available_width = ui.available_width();
            let spacing = if available_width > 1000.0 { 20.0 } else { 12.0 };
            
            egui::Grid::new("positions_grid")
                .num_columns(6)
                .spacing([spacing, 10.0])
                .striped(true)
                .min_row_height(42.0)
                .show(ui, |ui| {
                        // Header with proper column widths
                        ui.set_width(90.0); // Time column
                        ui.label(egui::RichText::new("Time")
                            .size(15.0).strong().color(egui::Color32::from_rgb(220, 230, 245)));
                        ui.set_width(200.0); // Token column
                        ui.label(egui::RichText::new("Token")
                            .size(15.0).strong().color(egui::Color32::from_rgb(220, 230, 245)));
                        ui.set_width(100.0); // Entry MC column
                        ui.label(egui::RichText::new("Entry MC")
                            .size(15.0).strong().color(egui::Color32::from_rgb(220, 230, 245)));
                        ui.set_width(100.0); // Invested column
                        ui.label(egui::RichText::new("Invested")
                            .size(15.0).strong().color(egui::Color32::from_rgb(220, 230, 245)));
                        ui.set_width(100.0); // Tokens column
                        ui.label(egui::RichText::new("Tokens")
                            .size(15.0).strong().color(egui::Color32::from_rgb(220, 230, 245)));
                        ui.set_width(150.0); // Action column
                        ui.label(egui::RichText::new("Action")
                            .size(15.0).strong().color(egui::Color32::from_rgb(220, 230, 245)));
                        ui.end_row();
                        
                        for pos in active_positions.iter().rev() {
                            // Time column
                            ui.set_width(90.0);
                            let time_str = pos.timestamp.format("%H:%M:%S").to_string();
                            ui.label(egui::RichText::new(time_str)
                                .size(13.0).color(egui::Color32::from_rgb(170, 190, 210)).monospace());
                            
                            // Token column
                            ui.set_width(200.0);
                            let mint_short = format_address_safe(&pos.mint);
                            if ui.link(egui::RichText::new(mint_short)
                                .size(13.0).monospace().color(egui::Color32::from_rgb(160, 210, 255))).clicked() {
                                let _ = open::that(format!("https://solscan.io/token/{}", pos.mint));
                            }
                            
                            // Entry MC column
                            ui.set_width(100.0);
                            let mc_str = if let Some(mc) = pos.mc_at_entry_usd {
                                format!("${:.0}", mc)
                            } else {
                                "-".to_string()
                            };
                            ui.label(egui::RichText::new(mc_str)
                                .size(13.0).strong().color(egui::Color32::from_rgb(255, 230, 110)));
                                
                            // Invested column
                            ui.set_width(100.0);
                            ui.label(egui::RichText::new(format!("{:.3} SOL", pos.our_buy_sol))
                                .size(13.0).strong().color(egui::Color32::from_rgb(255, 255, 255)));
                                
                            // Tokens column
                            ui.set_width(100.0);
                            let tokens_str = if let Some(amt) = pos.token_amount {
                                format!("{:.0}", amt as f64 / 1e6) // Assuming 6 decimals for display simplicity
                            } else {
                                "?".to_string()
                            };
                            ui.label(egui::RichText::new(tokens_str)
                                .size(13.0).color(egui::Color32::from_rgb(200, 200, 200)));
                                
                            // Action column
                            ui.set_width(150.0);
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing = egui::vec2(6.0, 0.0);
                                if ui.button("📈 Chart").clicked() {
                                    let _ = open::that(format!("https://photon-sol.tinyastro.io/en/lp/{}", pos.bonding_curve.as_ref().unwrap_or(&pos.mint)));
                                }
                                
                                let sell_btn = ui.add(egui::Button::new(egui::RichText::new("🚨 SELL")
                                    .size(13.0)
                                    .strong()
                                    .color(egui::Color32::WHITE))
                                    .fill(egui::Color32::from_rgb(255, 80, 80))
                                    .rounding(egui::Rounding::same(4.0)));
                                    
                                if sell_btn.clicked() {
                                    eprintln!("🖱️ Sell button clicked for {}", pos.mint);
                                    if let Err(e) = control_tx.send(BotControl::ManualSell(pos.mint.clone())) {
                                        eprintln!("❌ Failed to send ManualSell command: {}", e);
                                    } else {
                                        eprintln!("✅ ManualSell command sent for {}", pos.mint);
                                    }
                                }
                            });
                            
                            ui.end_row();
                        }
                    });
        }
    } else {
        ui.label("Tracker not initialized");
    }
    
    }); // End ScrollArea
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

