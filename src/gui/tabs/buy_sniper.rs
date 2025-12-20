// tabs/buy_sniper.rs - Manual Buy Sniper Tab
#![allow(unused)]

use eframe::egui;
use std::sync::{Arc, RwLock, mpsc, atomic::{AtomicBool, Ordering}};
use crate::config::Config;
use crate::gui::events::BotControl;
use std::collections::VecDeque;
use crate::gui::events::TokenEvent;

#[derive(Default)]
pub struct BuySniperState {
    mint_input: String,
    sol_amount_input: String,
    status_message: Option<String>,
    status_is_error: bool,
    is_processing: bool,
}

pub fn render(
    ui: &mut egui::Ui,
    state: &mut BuySniperState,
    config: &Arc<RwLock<Config>>,
    wallet_private_key: &Arc<RwLock<Option<String>>>,
    control_tx: &mpsc::Sender<BotControl>,
    event_log: &Arc<RwLock<VecDeque<TokenEvent>>>,
    bot_running: &Arc<AtomicBool>,
) {
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
    
    // Check for recent events to update status
    {
        // ✅ FIX: Use try_read() to avoid blocking GUI thread
        let events = match event_log.try_read() {
            Ok(events) => events,
            Err(_) => {
                // Lock is held by bot thread - skip this update
                return;
            }
        };
        // Scan last 50 events
        for event in events.iter().rev().take(50) {
            match event {
                TokenEvent::Bought { mint, signature, .. } => {
                    if state.is_processing && (state.mint_input.trim() == mint || state.mint_input.is_empty()) {
                        state.status_message = Some(format!("✅ Buy successful! Signature: {}", 
                            if signature.len() > 16 { 
                                format!("{}...{}", &signature[..8], &signature[signature.len()-8..])
                            } else {
                                signature.clone()
                            }));
                        state.status_is_error = false;
                        state.is_processing = false;
                        break;
                    }
                }
                TokenEvent::Error { message, .. } => {
                    // If we are processing and encounter an error relevant to manual buy
                    if state.is_processing && (message.contains("Manual buy failed") || message.contains("Failed to create accounts") || message.contains("Invalid mint")) {
                        state.status_message = Some(format!("❌ Error: {}", message));
                        state.status_is_error = true;
                        state.is_processing = false;
                        break;
                    }
                }
                _ => {}
            }
        }
    }

    ui.vertical_centered(|ui| {
        ui.add_space(16.0);
        ui.label(egui::RichText::new("🎯 Buy Sniper")
            .size(28.0)
            .strong()
            .color(egui::Color32::from_rgb(100, 255, 160)));
        ui.add_space(4.0);
        ui.label(egui::RichText::new("Manually buy any token by mint address")
            .size(14.0)
            .color(egui::Color32::from_rgb(160, 170, 185)));
    });
    ui.add_space(24.0);

    // Form container with better alignment
    ui.group(|ui| {
        ui.set_min_width(700.0);
        
        // Mint address input - MUCH larger and more visible
        let available_width = ui.available_width();
        let input_width = available_width - 180.0;
        
        ui.vertical(|ui| {
            ui.label(egui::RichText::new("Mint Address:")
                .size(18.0)
                .strong()
                .color(egui::Color32::from_rgb(220, 230, 245)));
            ui.add_space(8.0);
            let mint_response = ui.add_sized(
                egui::vec2(input_width, 50.0), // Fixed height for larger text box
                egui::TextEdit::singleline(&mut state.mint_input)
                    .hint_text("Enter token mint address (base58)...")
                    .font(egui::FontId::proportional(16.0))
                    .margin(egui::vec2(12.0, 12.0)) // More padding inside
            );
            if mint_response.changed() {
                state.status_message = None;
            }
        });
        ui.add_space(20.0);

        // SOL amount input (optional) - MUCH larger
        ui.vertical(|ui| {
            ui.label(egui::RichText::new("SOL Amount:")
                .size(18.0)
                .strong()
                .color(egui::Color32::from_rgb(220, 230, 245)));
            ui.add_space(8.0);
            // Safely read config without panicking if the lock is poisoned or currently held
            let default_sol = match config.try_read() {
                Ok(cfg) => cfg.buy_amount_lamports() as f64 / 1e9,
                Err(_) => 0.0, // Fallback to 0 when config is inaccessible
            };
            let sol_hint = format!("Default: {:.6} SOL (leave empty to use default)", default_sol);
            
            let sol_response = ui.add_sized(
                egui::vec2(input_width, 50.0), // Fixed height for larger text box
                egui::TextEdit::singleline(&mut state.sol_amount_input)
                    .hint_text(&sol_hint)
                    .font(egui::FontId::proportional(16.0))
                    .margin(egui::vec2(12.0, 12.0)) // More padding inside
            );
            if sol_response.changed() {
                state.status_message = None;
            }
        });
        ui.add_space(20.0);

        // Buy button - properly centered
        let mint_valid = !state.mint_input.trim().is_empty() && state.mint_input.trim().len() >= 32;
        let button_enabled = !state.is_processing && mint_valid;
        
        ui.horizontal(|ui| {
            let button_width = 200.0;
            let spacer = (available_width - button_width).max(0.0) / 2.0;
            ui.add_space(spacer);
            
            let button_text = if state.is_processing {
                "Processing..."
            } else {
                "🚀 Buy Now"
            };
            
            let button_color = if button_enabled {
                egui::Color32::from_rgb(0, 240, 120)
            } else {
                egui::Color32::from_rgb(100, 100, 100)
            };
            
            let button = ui.add_enabled(
                button_enabled,
                egui::Button::new(egui::RichText::new(button_text)
                    .size(16.0)
                    .strong()
                    .color(egui::Color32::WHITE))
                    .fill(button_color.linear_multiply(0.4))
                    .stroke(egui::Stroke::new(2.0, button_color))
                    .min_size(egui::vec2(button_width, 45.0))
                    .rounding(egui::Rounding::same(10.0))
            );
            
            if button.clicked() {
                    let mint = state.mint_input.trim().to_string();
                    let sol_amount = if state.sol_amount_input.trim().is_empty() {
                        None
                    } else {
                        state.sol_amount_input.trim().parse::<f64>().ok()
                            .map(|sol| (sol * 1e9) as u64)
                    };
                    
                    state.is_processing = true;
                    state.status_message = Some("⏳ Processing buy transaction...".to_string());
                    state.status_is_error = false;
                    
                    if let Err(e) = control_tx.send(BotControl::ManualBuy { 
                        mint, 
                        sol_amount 
                    }) {
                        state.status_message = Some(format!("❌ Failed to send buy command: {}", e));
                        state.status_is_error = true;
                        state.is_processing = false;
                    }
                }
        });
        ui.add_space(20.0);

        // Status message - centered
        if let Some(ref message) = state.status_message {
            let color = if state.status_is_error {
                egui::Color32::from_rgb(255, 100, 100)
            } else {
                egui::Color32::from_rgb(100, 255, 160)
            };
            
            ui.horizontal(|ui| {
                let message_width = 400.0;
                let spacer = (available_width - message_width).max(0.0) / 2.0;
                ui.add_space(spacer);
                ui.label(egui::RichText::new(message)
                    .size(14.0)
                    .color(color));
            });
        }

        // Validation message - centered
        if !state.mint_input.trim().is_empty() && !mint_valid {
            ui.horizontal(|ui| {
                let message_width = 400.0;
                let spacer = (available_width - message_width).max(0.0) / 2.0;
                ui.add_space(spacer);
                ui.label(egui::RichText::new("⚠️  Invalid mint address format")
                    .size(12.0)
                    .color(egui::Color32::from_rgb(255, 200, 100)));
            });
        }
    });
    
    }); // End ScrollArea
}

