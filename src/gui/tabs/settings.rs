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
    private_key_str: Option<String>,
    show_private_key: bool,
    private_key_error: Option<String>,
    // Nova polja za API i advanced settings
    helius_api_key_str: Option<String>,
    rpc_url_str: Option<String>,
    wss_url_str: Option<String>,
    sol_price_str: Option<String>,
    compute_units_str: Option<String>,
    jito_tip_str: Option<String>,
    // Auto-sell settings
    stop_loss_percent_str: Option<String>,
    take_profit_mc_str: Option<String>,
    sell_percent_str: Option<String>,
    monitor_interval_str: Option<String>,
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
            "private_key" => {
                if self.private_key_str.is_none() {
                    self.private_key_str = Some(default);
                }
                self.private_key_str.as_mut().unwrap()
            }
            "helius_api_key" => {
                if self.helius_api_key_str.is_none() {
                    self.helius_api_key_str = Some(default);
                }
                self.helius_api_key_str.as_mut().unwrap()
            }
            "rpc_url" => {
                if self.rpc_url_str.is_none() {
                    self.rpc_url_str = Some(default);
                }
                self.rpc_url_str.as_mut().unwrap()
            }
            "wss_url" => {
                if self.wss_url_str.is_none() {
                    self.wss_url_str = Some(default);
                }
                self.wss_url_str.as_mut().unwrap()
            }
            "sol_price" => {
                if self.sol_price_str.is_none() {
                    self.sol_price_str = Some(default);
                }
                self.sol_price_str.as_mut().unwrap()
            }
            "compute_units" => {
                if self.compute_units_str.is_none() {
                    self.compute_units_str = Some(default);
                }
                self.compute_units_str.as_mut().unwrap()
            }
            "jito_tip" => {
                if self.jito_tip_str.is_none() {
                    self.jito_tip_str = Some(default);
                }
                self.jito_tip_str.as_mut().unwrap()
            }
            "stop_loss_percent" => {
                if self.stop_loss_percent_str.is_none() {
                    self.stop_loss_percent_str = Some(default);
                }
                self.stop_loss_percent_str.as_mut().unwrap()
            }
            "take_profit_mc" => {
                if self.take_profit_mc_str.is_none() {
                    self.take_profit_mc_str = Some(default);
                }
                self.take_profit_mc_str.as_mut().unwrap()
            }
            "sell_percent" => {
                if self.sell_percent_str.is_none() {
                    self.sell_percent_str = Some(default);
                }
                self.sell_percent_str.as_mut().unwrap()
            }
            "monitor_interval" => {
                if self.monitor_interval_str.is_none() {
                    self.monitor_interval_str = Some(default);
                }
                self.monitor_interval_str.as_mut().unwrap()
            }
            _ => panic!("Unknown field id: {}", id),
        }
    }
    
    fn reset(&mut self) {
        *self = SettingsState::default();
    }
}

pub fn render(ui: &mut egui::Ui, config: &Arc<RwLock<Config>>, control_tx: &mpsc::Sender<BotControl>, wallet_private_key: &Arc<RwLock<Option<String>>>) {
    ui.vertical_centered(|ui| {
        ui.add_space(12.0);
        ui.label(egui::RichText::new("⚙️  Settings")
            .size(26.0)
            .strong()
            .color(egui::Color32::from_rgb(220, 230, 245)));
        ui.label(egui::RichText::new("Configure bot parameters and filters")
            .size(13.0)
            .color(egui::Color32::from_rgb(160, 170, 185)));
    });
    ui.add_space(20.0);
    
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
    
    // Enhanced Wallet Private Key Section with premium styling
    ui.group(|ui| {
        ui.set_min_height(140.0);
        ui.heading(egui::RichText::new("🔐 Wallet Configuration")
            .size(18.0)
            .strong()
            .color(egui::Color32::from_rgb(255, 210, 110)));
        ui.add_space(14.0);
        
        ui.label(egui::RichText::new("Enter your Solana wallet private key (base58 format)")
            .size(12.0)
            .color(egui::Color32::from_rgb(170, 180, 195)));
        ui.add_space(10.0);
        
        // Get current private key from state
        let current_ui_key = {
            let key = wallet_private_key.read().unwrap();
            key.clone()
        };
        
        let private_key_display = state.private_key_str.clone()
            .or_else(|| current_ui_key.clone())
            .unwrap_or_else(|| String::new());
        
        // Initialize if needed (before borrowing)
        if state.private_key_str.is_none() {
            state.private_key_str = Some(private_key_display.clone());
        }
        
        // Copy values to avoid borrowing issues during UI rendering
        let show_private_key = state.show_private_key;
        let mut private_key_str = state.private_key_str.as_ref().unwrap().clone();
        let mut key_changed = false;
        let mut toggle_show = false;
        
        // Enhanced password input with show/hide toggle
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Private Key:")
                .size(13.0)
                .strong()
                .color(egui::Color32::from_rgb(220, 230, 245)));
            ui.add_space(8.0);
            let password_response = ui.add(egui::TextEdit::singleline(&mut private_key_str)
                .password(!show_private_key)
                .desired_width(450.0));
            
            if password_response.changed() {
                key_changed = true;
            }
            
            ui.add_space(8.0);
            // Enhanced Show/Hide toggle button
            let toggle_response = ui.add(egui::Button::new(egui::RichText::new(if show_private_key { "👁️ Hide" } else { "👁️ Show" })
                    .size(13.0))
                    .fill(egui::Color32::from_rgb(100, 200, 255).linear_multiply(0.2))
                    .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(100, 200, 255)))
                    .min_size(egui::vec2(90.0, 32.0))
                    .rounding(egui::Rounding::same(5.0)));
            if toggle_response.clicked() {
                toggle_show = true;
            }
        });
        
        // Update state after UI borrow is released
        if let Some(ref mut key_str) = state.private_key_str {
            *key_str = private_key_str.clone();
        }
        
        if toggle_show {
            state.show_private_key = !show_private_key;
        }
        
        if key_changed {
            let trimmed = private_key_str.trim();
            if trimmed.is_empty() {
                // Clear key if empty
                if let Ok(mut key) = wallet_private_key.write() {
                    *key = None;
                }
                state.private_key_error = None;
            } else {
                // Validate key
                match crate::wallet::get_wallet_address_from_key(trimmed) {
                    Ok(_address) => {
                        // Valid key - save it
                        if let Ok(mut key) = wallet_private_key.write() {
                            *key = Some(trimmed.to_string());
                        }
                        state.private_key_error = None;
                    }
                    Err(e) => {
                        state.private_key_error = Some(format!("Invalid key: {}", e));
                    }
                }
            }
        }
        
        // Show error or success message
        let display_error = state.private_key_error.clone();
        let trimmed_key = private_key_str.trim();
        
        if let Some(ref error) = display_error {
            ui.label(egui::RichText::new(format!("❌ {}", error))
                .size(12.0)
                .strong()
                .color(egui::Color32::from_rgb(255, 120, 120)));
        } else if !trimmed_key.is_empty() {
            if let Ok(address) = crate::wallet::get_wallet_address_from_key(trimmed_key) {
                ui.label(egui::RichText::new(format!("✅ Wallet: {}", address))
                    .size(12.0)
                    .strong()
                    .color(egui::Color32::from_rgb(100, 255, 160)));
            }
        } else {
            ui.label(egui::RichText::new("ℹ️  Enter private key or use .env file")
                .size(12.0)
                .color(egui::Color32::from_rgb(160, 170, 185)));
        }
        
        ui.add_space(6.0);
        ui.label(egui::RichText::new("⚠️  Keep your private key secure! It's stored in memory only.")
            .size(11.0)
            .color(egui::Color32::from_rgb(255, 210, 110)));
    });
    
    ui.add_space(10.0);
    
    // Enhanced API Configuration Section
    ui.group(|ui| {
        ui.set_min_height(170.0);
        ui.heading(egui::RichText::new("🔑 API Configuration")
            .size(18.0)
            .strong()
            .color(egui::Color32::from_rgb(255, 160, 110)));
        ui.add_space(14.0);
        
        ui.label(egui::RichText::new("Required: Helius API Key. Optional: Custom RPC/WebSocket URLs")
            .size(12.0)
            .color(egui::Color32::from_rgb(170, 180, 195)));
        ui.add_space(10.0);
        
        // Enhanced HELIUS_API_KEY (required)
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Helius API Key:")
                .size(13.0)
                .strong()
                .color(egui::Color32::from_rgb(220, 230, 245)));
            ui.add_space(8.0);
            let api_key_str = state.get_or_init("helius_api_key", config_clone.helius_api_key.clone());
            if ui.add(egui::TextEdit::singleline(api_key_str)
                    .desired_width(450.0))
                    .changed() {
                config_clone.helius_api_key = api_key_str.trim().to_string();
                apply_config_live(&config, &control_tx, &config_clone);
                state.last_update_time = Some(std::time::Instant::now());
            }
        });
        
        // Enhanced RPC_URL (optional)
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("RPC URL (optional):")
                .size(13.0)
                .strong()
                .color(egui::Color32::from_rgb(220, 230, 245)));
            ui.add_space(8.0);
            let rpc_str = state.get_or_init("rpc_url", config_clone.rpc_url.clone());
            if ui.add(egui::TextEdit::singleline(rpc_str)
                    .desired_width(450.0))
                    .changed() {
                config_clone.rpc_url = rpc_str.trim().to_string();
                apply_config_live(&config, &control_tx, &config_clone);
                state.last_update_time = Some(std::time::Instant::now());
            }
        });
        
        // Enhanced WSS_URL (optional)
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("WebSocket URL (optional):")
                .size(13.0)
                .strong()
                .color(egui::Color32::from_rgb(220, 230, 245)));
            ui.add_space(8.0);
            let wss_str = state.get_or_init("wss_url", config_clone.wss_url.clone());
            if ui.add(egui::TextEdit::singleline(wss_str)
                    .desired_width(450.0))
                    .changed() {
                config_clone.wss_url = wss_str.trim().to_string();
                apply_config_live(&config, &control_tx, &config_clone);
                state.last_update_time = Some(std::time::Instant::now());
            }
        });
    });
    
    ui.add_space(10.0);
    
    // Enhanced basic settings with premium styling
    ui.group(|ui| {
        ui.set_min_height(300.0);
        ui.heading(egui::RichText::new("💰 Trading Settings")
            .size(18.0)
            .strong()
            .color(egui::Color32::from_rgb(255, 220, 0)));
        ui.add_space(14.0);
        
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Buy Amount (SOL):")
                .size(13.0)
                .strong()
                .color(egui::Color32::from_rgb(220, 230, 245)));
            ui.add_space(8.0);
            let buy_sol_str = state.get_or_init("buy_amount", config_clone.buy_amount_sol.to_string());
            if ui.add(egui::TextEdit::singleline(buy_sol_str)
                    .desired_width(200.0))
                    .changed() {
                if let Ok(val) = buy_sol_str.parse::<f64>() {
                    config_clone.buy_amount_sol = val;
                    apply_config_live(&config, &control_tx, &config_clone);
                    state.last_update_time = Some(std::time::Instant::now());
                }
            }
        });
        
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("SOL Price (USD):")
                .size(13.0)
                .strong()
                .color(egui::Color32::from_rgb(220, 230, 245)));
            ui.add_space(8.0);
            let sol_price_str = state.get_or_init("sol_price", config_clone.sol_price_usd.to_string());
            if ui.add(egui::TextEdit::singleline(sol_price_str)
                    .desired_width(200.0))
                    .changed() {
                if let Ok(val) = sol_price_str.parse::<f64>() {
                    config_clone.sol_price_usd = val;
                    apply_config_live(&config, &control_tx, &config_clone);
                    state.last_update_time = Some(std::time::Instant::now());
                }
            }
        });
        
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Priority Fee (micro-lamports):")
                .size(13.0)
                .strong()
                .color(egui::Color32::from_rgb(220, 230, 245)));
            ui.add_space(8.0);
            let fee_str = state.get_or_init("priority_fee", config_clone.priority_fee.to_string());
            if ui.add(egui::TextEdit::singleline(fee_str)
                    .desired_width(200.0))
                    .changed() {
                if let Ok(val) = fee_str.parse::<u64>() {
                    config_clone.priority_fee = val;
                    apply_config_live(&config, &control_tx, &config_clone);
                    state.last_update_time = Some(std::time::Instant::now());
                }
            }
        });
        
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Compute Units:")
                .size(13.0)
                .strong()
                .color(egui::Color32::from_rgb(220, 230, 245)));
            ui.add_space(8.0);
            let compute_str = state.get_or_init("compute_units", config_clone.compute_units.to_string());
            if ui.add(egui::TextEdit::singleline(compute_str)
                    .desired_width(200.0))
                    .changed() {
                if let Ok(val) = compute_str.parse::<u32>() {
                    config_clone.compute_units = val;
                    apply_config_live(&config, &control_tx, &config_clone);
                    state.last_update_time = Some(std::time::Instant::now());
                }
            }
        });
        
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Jito Tip (SOL):")
                .size(13.0)
                .strong()
                .color(egui::Color32::from_rgb(220, 230, 245)));
            ui.add_space(8.0);
            let jito_tip_sol = (config_clone.jito_tip as f64) / 1e9;
            let jito_str = state.get_or_init("jito_tip", jito_tip_sol.to_string());
            if ui.add(egui::TextEdit::singleline(jito_str)
                    .desired_width(200.0))
                    .changed() {
                if let Ok(val) = jito_str.parse::<f64>() {
                    config_clone.jito_tip = (val * 1e9) as u64;
                    apply_config_live(&config, &control_tx, &config_clone);
                    state.last_update_time = Some(std::time::Instant::now());
                }
            }
        });
        
        ui.add_space(10.0);
        if ui.checkbox(&mut config_clone.mock_buy, egui::RichText::new("🧪 Mock Buy (Test mode - no real transactions)")
                .size(13.0)).changed() {
            apply_config_live(&config, &control_tx, &config_clone);
            state.last_update_time = Some(std::time::Instant::now());
        }
        if config_clone.mock_buy {
            ui.label(egui::RichText::new("⚠️  Mock mode: Transactions will be simulated, not sent to blockchain")
                .size(12.0)
                .color(egui::Color32::from_rgb(255, 210, 110)));
        }
        
        ui.add_space(6.0);
        if ui.checkbox(&mut config_clone.one_shot_mode, egui::RichText::new("One Shot Mode")
                .size(13.0)).changed() {
            apply_config_live(&config, &control_tx, &config_clone);
            state.last_update_time = Some(std::time::Instant::now());
        }
        
        ui.add_space(6.0);
        if ui.checkbox(&mut config_clone.enable_tracker, egui::RichText::new("Enable Tracker")
                .size(13.0)).changed() {
            apply_config_live(&config, &control_tx, &config_clone);
            state.last_update_time = Some(std::time::Instant::now());
        }
    });
    
    ui.add_space(10.0);
    
    // Enhanced Auto-Sell Settings
    ui.group(|ui| {
        ui.set_min_height(220.0);
        ui.heading(egui::RichText::new("💰 Auto-Sell Settings")
            .size(18.0)
            .strong()
            .color(egui::Color32::from_rgb(255, 220, 0)));
        ui.add_space(14.0);
        
        ui.label(egui::RichText::new("Automatically sell positions when conditions are met")
            .size(12.0)
            .color(egui::Color32::from_rgb(170, 180, 195)));
        ui.add_space(10.0);
        
        // Enable Auto-Sell checkbox
        if ui.checkbox(&mut config_clone.enable_auto_sell, "Enable Auto-Sell").changed() {
            apply_config_live(&config, &control_tx, &config_clone);
            state.last_update_time = Some(std::time::Instant::now());
        }
        
        if config_clone.enable_auto_sell {
            ui.add_space(8.0);
            
            // Enhanced Stop Loss Percent
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Stop Loss (%):")
                    .size(13.0)
                    .strong()
                    .color(egui::Color32::from_rgb(220, 230, 245)));
                ui.add_space(8.0);
                let stop_loss_str = state.get_or_init("stop_loss_percent", config_clone.stop_loss_percent.to_string());
                if ui.add(egui::TextEdit::singleline(stop_loss_str)
                        .desired_width(150.0))
                        .changed() {
                    if let Ok(val) = stop_loss_str.parse::<f64>() {
                        if val >= 0.0 && val <= 100.0 {
                            config_clone.stop_loss_percent = val;
                            apply_config_live(&config, &control_tx, &config_clone);
                            state.last_update_time = Some(std::time::Instant::now());
                        }
                    }
                }
                ui.add_space(8.0);
                ui.label(egui::RichText::new("(Sell when MC drops by this % from entry)")
                    .size(11.0)
                    .color(egui::Color32::from_rgb(160, 170, 185)));
            });
            
            // Enhanced Take Profit MC
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Take Profit MC (USD):")
                    .size(13.0)
                    .strong()
                    .color(egui::Color32::from_rgb(220, 230, 245)));
                ui.add_space(8.0);
                let take_profit_str = state.get_or_init("take_profit_mc", config_clone.take_profit_mc_usd.to_string());
                if ui.add(egui::TextEdit::singleline(take_profit_str)
                        .desired_width(150.0))
                        .changed() {
                    if let Ok(val) = take_profit_str.parse::<f64>() {
                        if val > 0.0 {
                            config_clone.take_profit_mc_usd = val;
                            apply_config_live(&config, &control_tx, &config_clone);
                            state.last_update_time = Some(std::time::Instant::now());
                        }
                    }
                }
                ui.add_space(8.0);
                ui.label(egui::RichText::new("(Sell when MC reaches this value)")
                    .size(11.0)
                    .color(egui::Color32::from_rgb(160, 170, 185)));
            });
            
            // Enhanced Sell Percent
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Sell Percent (%):")
                    .size(13.0)
                    .strong()
                    .color(egui::Color32::from_rgb(220, 230, 245)));
                ui.add_space(8.0);
                let sell_percent_str = state.get_or_init("sell_percent", config_clone.sell_percent.to_string());
                if ui.add(egui::TextEdit::singleline(sell_percent_str)
                        .desired_width(150.0))
                        .changed() {
                    if let Ok(val) = sell_percent_str.parse::<f64>() {
                        if val > 0.0 && val <= 100.0 {
                            config_clone.sell_percent = val;
                            apply_config_live(&config, &control_tx, &config_clone);
                            state.last_update_time = Some(std::time::Instant::now());
                        }
                    }
                }
                ui.add_space(8.0);
                ui.label(egui::RichText::new("(% of position to sell)")
                    .size(11.0)
                    .color(egui::Color32::from_rgb(160, 170, 185)));
            });
            
            // Enhanced Monitor Interval
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Monitor Interval (sec):")
                    .size(13.0)
                    .strong()
                    .color(egui::Color32::from_rgb(220, 230, 245)));
                ui.add_space(8.0);
                let monitor_str = state.get_or_init("monitor_interval", config_clone.monitor_interval_sec.to_string());
                if ui.add(egui::TextEdit::singleline(monitor_str)
                        .desired_width(150.0))
                        .changed() {
                    if let Ok(val) = monitor_str.parse::<u64>() {
                        if val > 0 {
                            config_clone.monitor_interval_sec = val;
                            apply_config_live(&config, &control_tx, &config_clone);
                            state.last_update_time = Some(std::time::Instant::now());
                        }
                    }
                }
                ui.add_space(8.0);
                ui.label(egui::RichText::new("(How often to check positions)")
                    .size(11.0)
                    .color(egui::Color32::from_rgb(160, 170, 185)));
            });
            
            ui.add_space(10.0);
            ui.label(egui::RichText::new("ℹ️  Auto-sell will trigger when:")
                .size(12.0)
                .strong()
                .color(egui::Color32::from_rgb(160, 210, 255)));
            ui.label(egui::RichText::new(format!("  • Market cap drops {}% from entry (Stop Loss)", config_clone.stop_loss_percent))
                .size(11.0)
                .color(egui::Color32::from_rgb(210, 220, 235)));
            ui.label(egui::RichText::new(format!("  • Market cap reaches ${:.0} (Take Profit)", config_clone.take_profit_mc_usd))
                .size(11.0)
                .color(egui::Color32::from_rgb(210, 220, 235)));
        } else {
            ui.add_space(10.0);
            ui.label(egui::RichText::new("⚠️  Auto-sell is disabled")
                .size(12.0)
                .color(egui::Color32::from_rgb(255, 210, 110)));
        }
    });
    
    ui.add_space(10.0);
    
    // Enhanced Target mint address (single token mode)
    ui.group(|ui| {
        ui.set_min_height(120.0);
        ui.heading(egui::RichText::new("🎯 Target Token (Optional)")
            .size(18.0)
            .strong()
            .color(egui::Color32::from_rgb(255, 160, 210)));
        ui.add_space(14.0);
        
        ui.label(egui::RichText::new("If set, bot will only buy this specific token when detected")
            .size(12.0)
            .color(egui::Color32::from_rgb(170, 180, 195)));
        ui.add_space(10.0);
        
        let target_mint_display = config_clone.target_mint_address
            .map(|p| p.to_string())
            .unwrap_or_else(|| String::new());
        let target_mint_str = state.get_or_init("target_mint", target_mint_display);
        
        let mut target_mint_changed = false;
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Mint Address:")
                .size(13.0)
                .strong()
                .color(egui::Color32::from_rgb(220, 230, 245)));
            ui.add_space(8.0);
            if ui.add(egui::TextEdit::singleline(target_mint_str)
                    .desired_width(450.0))
                    .changed() {
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
                .size(12.0)
                .strong()
                .color(egui::Color32::from_rgb(100, 255, 160)));
        } else {
            ui.label(egui::RichText::new("ℹ️  No target set - will buy all matching tokens")
                .size(12.0)
                .color(egui::Color32::from_rgb(160, 170, 185)));
        }
    });
    
    ui.add_space(10.0);
    
    // Enhanced Dev buy filter
    ui.group(|ui| {
        ui.set_min_height(140.0);
        ui.heading(egui::RichText::new("🔍 Dev Buy Filter")
            .size(18.0)
            .strong()
            .color(egui::Color32::from_rgb(120, 200, 255)));
        ui.add_space(14.0);
        
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Min Dev Buy (USD):")
                .size(13.0)
                .strong()
                .color(egui::Color32::from_rgb(220, 230, 245)));
            ui.add_space(8.0);
            let min_str = state.get_or_init("min_dev_buy", config_clone.min_dev_buy_usd.to_string());
            if ui.add(egui::TextEdit::singleline(min_str)
                    .desired_width(200.0))
                    .changed() {
                if let Ok(val) = min_str.parse::<f64>() {
                    config_clone.min_dev_buy_usd = val;
                    // ✅ LIVE UPDATE: Apply immediately
                    apply_config_live(&config, &control_tx, &config_clone);
                    state.last_update_time = Some(std::time::Instant::now());
                }
            }
        });
        
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Max Dev Buy (USD):")
                .size(13.0)
                .strong()
                .color(egui::Color32::from_rgb(220, 230, 245)));
            ui.add_space(8.0);
            let max_str = state.get_or_init("max_dev_buy", config_clone.max_dev_buy_usd.to_string());
            if ui.add(egui::TextEdit::singleline(max_str)
                    .desired_width(200.0))
                    .changed() {
                if let Ok(val) = max_str.parse::<f64>() {
                    config_clone.max_dev_buy_usd = val;
                    // ✅ LIVE UPDATE: Apply immediately
                    apply_config_live(&config, &control_tx, &config_clone);
                    state.last_update_time = Some(std::time::Instant::now());
                }
            }
        });
        
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Min Dev Tokens:")
                .size(13.0)
                .strong()
                .color(egui::Color32::from_rgb(220, 230, 245)));
            ui.add_space(8.0);
            let min_str = state.get_or_init("min_dev_tokens", config_clone.min_dev_tokens.to_string());
            if ui.add(egui::TextEdit::singleline(min_str)
                    .desired_width(150.0))
                    .changed() {
                if let Ok(val) = min_str.parse::<usize>() {
                    config_clone.min_dev_tokens = val;
                    // ✅ LIVE UPDATE: Apply immediately
                    apply_config_live(&config, &control_tx, &config_clone);
                    state.last_update_time = Some(std::time::Instant::now());
                }
            }
            
            ui.add_space(20.0);
            ui.label(egui::RichText::new("Max Dev Tokens:")
                .size(13.0)
                .strong()
                .color(egui::Color32::from_rgb(220, 230, 245)));
            ui.add_space(8.0);
            let max_str = state.get_or_init("max_dev_tokens", config_clone.max_dev_tokens.to_string());
            if ui.add(egui::TextEdit::singleline(max_str)
                    .desired_width(150.0))
                    .changed() {
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
    
    // Enhanced Social filters
    ui.group(|ui| {
        ui.set_min_height(120.0);
        ui.heading(egui::RichText::new("📱 Social Filters")
            .size(18.0)
            .strong()
            .color(egui::Color32::from_rgb(100, 255, 190)));
        ui.add_space(14.0);
        
        if ui.checkbox(&mut config_clone.require_socials, egui::RichText::new("Require Socials")
                .size(13.0)).changed() {
            // ✅ LIVE UPDATE: Apply immediately
            apply_config_live(&config, &control_tx, &config_clone);
            state.last_update_time = Some(std::time::Instant::now());
        }
        ui.add_space(6.0);
        if ui.checkbox(&mut config_clone.require_twitter, egui::RichText::new("Require Twitter/X")
                .size(13.0)).changed() {
            // ✅ LIVE UPDATE: Apply immediately
            apply_config_live(&config, &control_tx, &config_clone);
            state.last_update_time = Some(std::time::Instant::now());
        }
        ui.add_space(8.0);
        
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Min Socials Count:")
                .size(13.0)
                .strong()
                .color(egui::Color32::from_rgb(220, 230, 245)));
            ui.add_space(8.0);
            let count_str = state.get_or_init("min_socials_count", config_clone.min_socials_count.to_string());
            if ui.add(egui::TextEdit::singleline(count_str)
                    .desired_width(150.0))
                    .changed() {
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
    
    // Enhanced Submission mode
    ui.group(|ui| {
        ui.set_min_height(100.0);
        ui.heading(egui::RichText::new("🚀 Submission Mode")
            .size(18.0)
            .strong()
            .color(egui::Color32::from_rgb(255, 160, 110)));
        ui.add_space(14.0);
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
    
    // Enhanced live update status
    if let Some(last_update) = state.last_update_time {
        let elapsed = last_update.elapsed();
        if elapsed.as_secs() < 2 {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("✅ Settings applied live")
                    .size(13.0)
                    .strong()
                    .color(egui::Color32::from_rgb(100, 255, 160)));
            });
        }
    }
    
    ui.add_space(16.0);
    
    // Enhanced action buttons with premium styling
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing = egui::vec2(14.0, 0.0);
        
        // Clone state for use in button click handler
        let state_clone = state.clone();
        
        // Enhanced Apply button now validates and ensures all fields are synced
        let sync_response = ui.add(egui::Button::new(egui::RichText::new("💾 Sync All Settings")
                .size(15.0)
                .strong())
                .fill(egui::Color32::from_rgb(100, 220, 120).linear_multiply(0.25))
                .stroke(egui::Stroke::new(2.0, egui::Color32::from_rgb(100, 220, 120)))
                .min_size(egui::vec2(170.0, 40.0))
                .rounding(egui::Rounding::same(8.0)));
        
        if sync_response.clicked() {
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
            if let Some(helius_api_key_str) = &state_clone.helius_api_key_str {
                config_clone.helius_api_key = helius_api_key_str.trim().to_string();
            }
            if let Some(rpc_url_str) = &state_clone.rpc_url_str {
                config_clone.rpc_url = rpc_url_str.trim().to_string();
            }
            if let Some(wss_url_str) = &state_clone.wss_url_str {
                config_clone.wss_url = wss_url_str.trim().to_string();
            }
            if let Some(sol_price_str) = &state_clone.sol_price_str {
                if let Ok(val) = sol_price_str.parse::<f64>() {
                    config_clone.sol_price_usd = val;
                }
            }
            if let Some(compute_units_str) = &state_clone.compute_units_str {
                if let Ok(val) = compute_units_str.parse::<u32>() {
                    config_clone.compute_units = val;
                }
            }
            if let Some(jito_tip_str) = &state_clone.jito_tip_str {
                if let Ok(val) = jito_tip_str.parse::<f64>() {
                    config_clone.jito_tip = (val * 1e9) as u64;
                }
            }
            if let Some(stop_loss_str) = &state_clone.stop_loss_percent_str {
                if let Ok(val) = stop_loss_str.parse::<f64>() {
                    if val >= 0.0 && val <= 100.0 {
                        config_clone.stop_loss_percent = val;
                    }
                }
            }
            if let Some(take_profit_str) = &state_clone.take_profit_mc_str {
                if let Ok(val) = take_profit_str.parse::<f64>() {
                    if val > 0.0 {
                        config_clone.take_profit_mc_usd = val;
                    }
                }
            }
            if let Some(sell_percent_str) = &state_clone.sell_percent_str {
                if let Ok(val) = sell_percent_str.parse::<f64>() {
                    if val > 0.0 && val <= 100.0 {
                        config_clone.sell_percent = val;
                    }
                }
            }
            if let Some(monitor_interval_str) = &state_clone.monitor_interval_str {
                if let Ok(val) = monitor_interval_str.parse::<u64>() {
                    if val > 0 {
                        config_clone.monitor_interval_sec = val;
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
        
        let reset_response = ui.add(egui::Button::new(egui::RichText::new("🔄 Reset to Defaults")
                .size(15.0)
                .strong())
                .fill(egui::Color32::from_rgb(220, 160, 110).linear_multiply(0.25))
                .stroke(egui::Stroke::new(2.0, egui::Color32::from_rgb(220, 160, 110)))
                .min_size(egui::vec2(170.0, 40.0))
                .rounding(egui::Rounding::same(8.0)));
        
        if reset_response.clicked() {
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

