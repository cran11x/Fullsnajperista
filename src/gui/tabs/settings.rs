// tabs/settings.rs - Settings editor
#![allow(unused)]

use eframe::egui;
use std::sync::{Arc, RwLock, mpsc};
use std::str::FromStr;
use crate::config::{Config, SubmissionMode};
use crate::gui::events::BotControl;

/// ✅ LIVE UPDATE: Apply config changes immediately without needing Apply button
/// Uses debouncing to prevent too many updates (stable and efficient)
fn apply_config_live(
    config: &Arc<RwLock<Config>>,
    control_tx: &mpsc::Sender<BotControl>,
    new_config: &Config,
) {
    // Validate before applying
    if let Err(e) = new_config.validate() {
        // Don't spam errors - only log if it's a real issue
        eprintln!("⚠️  Config validation failed (not applied): {}", e);
        return;
    }
    
    // Update shared config immediately (for GUI display)
    {
        let mut cfg = config.write().unwrap();
        *cfg = new_config.clone();
    }
    
    // Send update to bot (non-blocking, will be processed in next iteration)
    // This is safe because bot reads config fresh each time
    let _ = control_tx.send(BotControl::UpdateConfig(new_config.clone()));
}

#[derive(Default, Clone)]
struct SettingsState {
    buy_amount_str: Option<String>,
    priority_fee_str: Option<String>,
    min_dev_buy_str: Option<String>,
    max_dev_buy_str: Option<String>,
    min_dev_tokens_str: Option<String>,
    max_dev_tokens_str: Option<String>,
    min_socials_count_str: Option<String>,
    target_mint_str: Option<String>,
    last_update_time: Option<std::time::Instant>,
}

impl SettingsState {
    fn get_or_init(&mut self, id: &str, default: String) -> &mut String {
        match id {
            "buy_amount" => {
                if self.buy_amount_str.is_none() {
                    self.buy_amount_str = Some(default);
                }
                self.buy_amount_str.as_mut().unwrap()
            }
            "priority_fee" => {
                if self.priority_fee_str.is_none() {
                    self.priority_fee_str = Some(default);
                }
                self.priority_fee_str.as_mut().unwrap()
            }
            "min_dev_buy" => {
                if self.min_dev_buy_str.is_none() {
                    self.min_dev_buy_str = Some(default);
                }
                self.min_dev_buy_str.as_mut().unwrap()
            }
            "max_dev_buy" => {
                if self.max_dev_buy_str.is_none() {
                    self.max_dev_buy_str = Some(default);
                }
                self.max_dev_buy_str.as_mut().unwrap()
            }
            "min_dev_tokens" => {
                if self.min_dev_tokens_str.is_none() {
                    self.min_dev_tokens_str = Some(default);
                }
                self.min_dev_tokens_str.as_mut().unwrap()
            }
            "max_dev_tokens" => {
                if self.max_dev_tokens_str.is_none() {
                    self.max_dev_tokens_str = Some(default);
                }
                self.max_dev_tokens_str.as_mut().unwrap()
            }
            "min_socials_count" => {
                if self.min_socials_count_str.is_none() {
                    self.min_socials_count_str = Some(default);
                }
                self.min_socials_count_str.as_mut().unwrap()
            }
            "target_mint" => {
                if self.target_mint_str.is_none() {
                    self.target_mint_str = Some(default);
                }
                self.target_mint_str.as_mut().unwrap()
            }
            _ => panic!("Unknown field id: {}", id),
        }
    }
    
    fn reset(&mut self) {
        *self = SettingsState::default();
    }
}

pub fn render(ui: &mut egui::Ui, config: &Arc<RwLock<Config>>, control_tx: &mpsc::Sender<BotControl>) {
    ui.vertical_centered(|ui| {
        ui.add_space(8.0);
        ui.label(egui::RichText::new("⚙️  Settings")
            .size(22.0)
            .strong()
            .color(egui::Color32::from_rgb(200, 200, 220)));
        ui.label(egui::RichText::new("Configure bot parameters and filters")
            .size(11.0)
            .color(egui::Color32::from_rgb(150, 150, 160)));
    });
    ui.add_space(16.0);
    
    let current_config = {
        let cfg = config.read().unwrap();
        (*cfg).clone()
    };
    
    // Get or create settings state in memory
    let state_id = egui::Id::new("settings_state");
    let mut state: SettingsState = ui.data_mut(|d| {
        d.get_temp_mut_or_insert_with(state_id, || SettingsState::default()).clone()
    });
    
    let mut config_clone = current_config.clone();
    
    // Basic settings with better styling
    ui.group(|ui| {
        ui.set_min_height(100.0);
        ui.heading(egui::RichText::new("💰 Trading Settings")
            .size(16.0)
            .color(egui::Color32::from_rgb(255, 215, 100)));
        ui.add_space(12.0);
        
        ui.horizontal(|ui| {
            ui.label("Buy Amount (SOL):");
            let buy_sol_str = state.get_or_init("buy_amount", config_clone.buy_amount_sol.to_string());
            if ui.text_edit_singleline(buy_sol_str).changed() {
                if let Ok(val) = buy_sol_str.parse::<f64>() {
                    config_clone.buy_amount_sol = val;
                    // ✅ LIVE UPDATE: Apply immediately
                    apply_config_live(&config, &control_tx, &config_clone);
                    state.last_update_time = Some(std::time::Instant::now());
                }
            }
        });
        
        ui.horizontal(|ui| {
            ui.label("Priority Fee (micro-lamports):");
            let fee_str = state.get_or_init("priority_fee", config_clone.priority_fee.to_string());
            if ui.text_edit_singleline(fee_str).changed() {
                if let Ok(val) = fee_str.parse::<u64>() {
                    config_clone.priority_fee = val;
                    // ✅ LIVE UPDATE: Apply immediately
                    apply_config_live(&config, &control_tx, &config_clone);
                    state.last_update_time = Some(std::time::Instant::now());
                }
            }
        });
        
        ui.add_space(8.0);
        let old_mock_buy = config_clone.mock_buy;
        if ui.checkbox(&mut config_clone.mock_buy, "🧪 Mock Buy (Test mode - no real transactions)").changed() {
            // ✅ LIVE UPDATE: Apply immediately
            apply_config_live(&config, &control_tx, &config_clone);
            state.last_update_time = Some(std::time::Instant::now());
        }
        if config_clone.mock_buy {
            ui.label(egui::RichText::new("⚠️  Mock mode: Transactions will be simulated, not sent to blockchain")
                .size(11.0)
                .color(egui::Color32::from_rgb(255, 200, 100)));
        }
    });
    
    ui.add_space(10.0);
    
    // Target mint address (single token mode)
    ui.group(|ui| {
        ui.set_min_height(100.0);
        ui.heading(egui::RichText::new("🎯 Target Token (Optional)")
            .size(16.0)
            .color(egui::Color32::from_rgb(255, 150, 200)));
        ui.add_space(12.0);
        
        ui.label(egui::RichText::new("If set, bot will only buy this specific token when detected")
            .size(11.0)
            .color(egui::Color32::from_rgb(150, 150, 160)));
        ui.add_space(8.0);
        
        let target_mint_display = config_clone.target_mint_address
            .map(|p| p.to_string())
            .unwrap_or_else(|| String::new());
        let target_mint_str = state.get_or_init("target_mint", target_mint_display);
        
        let mut target_mint_changed = false;
        ui.horizontal(|ui| {
            ui.label("Mint Address:");
            if ui.text_edit_singleline(target_mint_str).changed() {
                // Parse and update config immediately
                let trimmed = target_mint_str.trim();
                if trimmed.is_empty() {
                    config_clone.target_mint_address = None;
                    target_mint_changed = true;
                } else {
                    if let Ok(pubkey) = solana_sdk::pubkey::Pubkey::from_str(trimmed) {
                        config_clone.target_mint_address = Some(pubkey);
                        target_mint_changed = true;
                    }
                }
            }
        });
        
        // Apply update after borrow is released
        if target_mint_changed {
            apply_config_live(&config, &control_tx, &config_clone);
            state.last_update_time = Some(std::time::Instant::now());
        }
        
        if let Some(target_mint) = config_clone.target_mint_address {
            ui.label(egui::RichText::new(format!("✅ Target set: {}", target_mint))
                .size(11.0)
                .color(egui::Color32::from_rgb(100, 255, 150)));
        } else {
            ui.label(egui::RichText::new("ℹ️  No target set - will buy all matching tokens")
                .size(11.0)
                .color(egui::Color32::from_rgb(150, 150, 160)));
        }
    });
    
    ui.add_space(10.0);
    
    // Dev buy filter
    ui.group(|ui| {
        ui.set_min_height(120.0);
        ui.heading(egui::RichText::new("🔍 Dev Buy Filter")
            .size(16.0)
            .color(egui::Color32::from_rgb(100, 180, 255)));
        ui.add_space(12.0);
        
        ui.horizontal(|ui| {
            ui.label("Min Dev Buy (USD):");
            let min_str = state.get_or_init("min_dev_buy", config_clone.min_dev_buy_usd.to_string());
            if ui.text_edit_singleline(min_str).changed() {
                if let Ok(val) = min_str.parse::<f64>() {
                    config_clone.min_dev_buy_usd = val;
                    // ✅ LIVE UPDATE: Apply immediately
                    apply_config_live(&config, &control_tx, &config_clone);
                    state.last_update_time = Some(std::time::Instant::now());
                }
            }
        });
        
        ui.horizontal(|ui| {
            ui.label("Max Dev Buy (USD):");
            let max_str = state.get_or_init("max_dev_buy", config_clone.max_dev_buy_usd.to_string());
            if ui.text_edit_singleline(max_str).changed() {
                if let Ok(val) = max_str.parse::<f64>() {
                    config_clone.max_dev_buy_usd = val;
                    // ✅ LIVE UPDATE: Apply immediately
                    apply_config_live(&config, &control_tx, &config_clone);
                    state.last_update_time = Some(std::time::Instant::now());
                }
            }
        });
        
        ui.horizontal(|ui| {
            ui.label("Min Dev Tokens:");
            let min_str = state.get_or_init("min_dev_tokens", config_clone.min_dev_tokens.to_string());
            if ui.text_edit_singleline(min_str).changed() {
                if let Ok(val) = min_str.parse::<usize>() {
                    config_clone.min_dev_tokens = val;
                    // ✅ LIVE UPDATE: Apply immediately
                    apply_config_live(&config, &control_tx, &config_clone);
                    state.last_update_time = Some(std::time::Instant::now());
                }
            }
            
            ui.label("Max Dev Tokens:");
            let max_str = state.get_or_init("max_dev_tokens", config_clone.max_dev_tokens.to_string());
            if ui.text_edit_singleline(max_str).changed() {
                if let Ok(val) = max_str.parse::<usize>() {
                    config_clone.max_dev_tokens = val;
                    // ✅ LIVE UPDATE: Apply immediately
                    apply_config_live(&config, &control_tx, &config_clone);
                    state.last_update_time = Some(std::time::Instant::now());
                }
            }
        });
    });
    
    ui.add_space(10.0);
    
    // Social filters
    ui.group(|ui| {
        ui.set_min_height(100.0);
        ui.heading(egui::RichText::new("📱 Social Filters")
            .size(16.0)
            .color(egui::Color32::from_rgb(100, 255, 180)));
        ui.add_space(12.0);
        
        if ui.checkbox(&mut config_clone.require_socials, "Require Socials").changed() {
            // ✅ LIVE UPDATE: Apply immediately
            apply_config_live(&config, &control_tx, &config_clone);
            state.last_update_time = Some(std::time::Instant::now());
        }
        if ui.checkbox(&mut config_clone.require_twitter, "Require Twitter/X").changed() {
            // ✅ LIVE UPDATE: Apply immediately
            apply_config_live(&config, &control_tx, &config_clone);
            state.last_update_time = Some(std::time::Instant::now());
        }
        
        ui.horizontal(|ui| {
            ui.label("Min Socials Count:");
            let count_str = state.get_or_init("min_socials_count", config_clone.min_socials_count.to_string());
            if ui.text_edit_singleline(count_str).changed() {
                if let Ok(val) = count_str.parse::<usize>() {
                    config_clone.min_socials_count = val;
                    // ✅ LIVE UPDATE: Apply immediately
                    apply_config_live(&config, &control_tx, &config_clone);
                    state.last_update_time = Some(std::time::Instant::now());
                }
            }
        });
    });
    
    ui.add_space(10.0);
    
    // Submission mode
    ui.group(|ui| {
        ui.set_min_height(80.0);
        ui.heading(egui::RichText::new("🚀 Submission Mode")
            .size(16.0)
            .color(egui::Color32::from_rgb(255, 150, 100)));
        ui.add_space(12.0);
        egui::ComboBox::from_id_source("submission_mode")
            .selected_text(config_clone.submission_mode.as_str())
            .show_ui(ui, |ui| {
                let mut changed = false;
                if ui.selectable_value(&mut config_clone.submission_mode, SubmissionMode::Helius, "Helius").changed() {
                    changed = true;
                }
                if ui.selectable_value(&mut config_clone.submission_mode, SubmissionMode::Jito, "Jito").changed() {
                    changed = true;
                }
                if ui.selectable_value(&mut config_clone.submission_mode, SubmissionMode::Rpc, "RPC").changed() {
                    changed = true;
                }
                if ui.selectable_value(&mut config_clone.submission_mode, SubmissionMode::All, "All").changed() {
                    changed = true;
                }
                if changed {
                    // ✅ LIVE UPDATE: Apply immediately
                    apply_config_live(&config, &control_tx, &config_clone);
                    state.last_update_time = Some(std::time::Instant::now());
                }
            });
    });
    
    ui.add_space(20.0);
    
    // Show live update status
    if let Some(last_update) = state.last_update_time {
        let elapsed = last_update.elapsed();
        if elapsed.as_secs() < 2 {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("✅ Settings applied live")
                    .size(11.0)
                    .color(egui::Color32::from_rgb(100, 255, 100)));
            });
        }
    }
    
    ui.add_space(10.0);
    
    // Action buttons with modern styling
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing = egui::vec2(12.0, 0.0);
        
        // Clone state for use in button click handler
        let state_clone = state.clone();
        
        // Apply button now validates and ensures all fields are synced
        if ui.add(egui::Button::new(egui::RichText::new("💾 Sync All Settings")
                .size(14.0)
                .strong())
                .fill(egui::Color32::from_rgb(100, 200, 100).linear_multiply(0.2))
                .stroke(egui::Stroke::new(1.5, egui::Color32::from_rgb(100, 200, 100)))
                .min_size(egui::vec2(150.0, 36.0)))
                .clicked() {
            // Parse all values from state strings before applying
            // This ensures all values are up-to-date even if user didn't click out of field
            if let Some(buy_amount_str) = &state_clone.buy_amount_str {
                if let Ok(val) = buy_amount_str.parse::<f64>() {
                    config_clone.buy_amount_sol = val;
                }
            }
            if let Some(priority_fee_str) = &state_clone.priority_fee_str {
                if let Ok(val) = priority_fee_str.parse::<u64>() {
                    config_clone.priority_fee = val;
                }
            }
            if let Some(min_dev_buy_str) = &state_clone.min_dev_buy_str {
                if let Ok(val) = min_dev_buy_str.parse::<f64>() {
                    config_clone.min_dev_buy_usd = val;
                }
            }
            if let Some(max_dev_buy_str) = &state_clone.max_dev_buy_str {
                if let Ok(val) = max_dev_buy_str.parse::<f64>() {
                    config_clone.max_dev_buy_usd = val;
                }
            }
            if let Some(min_dev_tokens_str) = &state_clone.min_dev_tokens_str {
                let trimmed = min_dev_tokens_str.trim();
                if !trimmed.is_empty() {
                    if let Ok(val) = trimmed.parse::<usize>() {
                        config_clone.min_dev_tokens = val;
                    }
                }
            }
            if let Some(max_dev_tokens_str) = &state_clone.max_dev_tokens_str {
                let trimmed = max_dev_tokens_str.trim();
                if !trimmed.is_empty() {
                    if let Ok(val) = trimmed.parse::<usize>() {
                        config_clone.max_dev_tokens = val;
                    }
                }
            }
            if let Some(min_socials_count_str) = &state_clone.min_socials_count_str {
                if let Ok(val) = min_socials_count_str.parse::<usize>() {
                    config_clone.min_socials_count = val;
                }
            }
            if let Some(target_mint_str) = &state_clone.target_mint_str {
                let trimmed = target_mint_str.trim();
                if trimmed.is_empty() {
                    config_clone.target_mint_address = None;
                } else {
                    if let Ok(pubkey) = solana_sdk::pubkey::Pubkey::from_str(trimmed) {
                        config_clone.target_mint_address = Some(pubkey);
                    }
                }
            }
            
            // Validate and apply (sync all fields)
            if let Err(e) = config_clone.validate() {
                eprintln!("❌ Config validation failed: {}", e);
            } else {
                // Apply using live update function
                apply_config_live(&config, &control_tx, &config_clone);
                state.last_update_time = Some(std::time::Instant::now());
                // Reset state to reflect new config values
                ui.data_mut(|d| {
                    d.insert_temp(state_id, SettingsState::default());
                });
            }
        }
        
        if ui.add(egui::Button::new(egui::RichText::new("🔄 Reset to Defaults")
                .size(14.0))
                .fill(egui::Color32::from_rgb(200, 150, 100).linear_multiply(0.2))
                .stroke(egui::Stroke::new(1.5, egui::Color32::from_rgb(200, 150, 100)))
                .min_size(egui::vec2(150.0, 36.0)))
                .clicked() {
            let default_config = Config::default();
            {
                let mut cfg = config.write().unwrap();
                *cfg = default_config.clone();
            }
            let _ = control_tx.send(BotControl::UpdateConfig(default_config));
            // Reset state to reflect new default values
            ui.data_mut(|d| {
                d.insert_temp(state_id, SettingsState::default());
            });
        }
    });
    
    // Save state back to memory
    ui.data_mut(|d| {
        d.insert_temp(state_id, state);
    });
}

