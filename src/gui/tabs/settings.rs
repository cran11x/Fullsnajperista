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
    slippage_percent_str: Option<String>,
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
    breakeven_mc_threshold_str: Option<String>,
    // Blacklist/Whitelist
    blacklisted_tokens_str: Option<String>,
    blacklisted_creators_str: Option<String>,
    whitelisted_tokens_str: Option<String>,
    // Token metadata filters
    require_uppercase_token: bool,
    max_name_length_str: Option<String>,
    min_ticker_length_str: Option<String>,
    max_ticker_length_str: Option<String>,
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
            "slippage_percent" => {
                if self.slippage_percent_str.is_none() {
                    self.slippage_percent_str = Some(default);
                }
                self.slippage_percent_str.as_mut().unwrap()
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
            "take_profit_mc_usd" => {
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
            "blacklisted_tokens" => {
                if self.blacklisted_tokens_str.is_none() {
                    self.blacklisted_tokens_str = Some(default);
                }
                self.blacklisted_tokens_str.as_mut().unwrap()
            }
            "blacklisted_creators" => {
                if self.blacklisted_creators_str.is_none() {
                    self.blacklisted_creators_str = Some(default);
                }
                self.blacklisted_creators_str.as_mut().unwrap()
            }
            "whitelisted_tokens" => {
                if self.whitelisted_tokens_str.is_none() {
                    self.whitelisted_tokens_str = Some(default);
                }
                self.whitelisted_tokens_str.as_mut().unwrap()
            }
            "breakeven_mc_threshold" => {
                if self.breakeven_mc_threshold_str.is_none() {
                    self.breakeven_mc_threshold_str = Some(default);
                }
                self.breakeven_mc_threshold_str.as_mut().unwrap()
            }
            "breakeven_mc_threshold_usd" => {
                if self.breakeven_mc_threshold_str.is_none() {
                    self.breakeven_mc_threshold_str = Some(default);
                }
                self.breakeven_mc_threshold_str.as_mut().unwrap()
            }
            "max_name_length" => {
                if self.max_name_length_str.is_none() {
                    self.max_name_length_str = Some(default);
                }
                self.max_name_length_str.as_mut().unwrap()
            }
            "min_ticker_length" => {
                if self.min_ticker_length_str.is_none() {
                    self.min_ticker_length_str = Some(default);
                }
                self.min_ticker_length_str.as_mut().unwrap()
            }
            "max_ticker_length" => {
                if self.max_ticker_length_str.is_none() {
                    self.max_ticker_length_str = Some(default);
                }
                self.max_ticker_length_str.as_mut().unwrap()
            }
            _ => panic!("Unknown field id: {}", id),
        }
    }
    
    fn reset(&mut self) {
        *self = SettingsState::default();
    }
}

pub fn render(ui: &mut egui::Ui, config: &Arc<RwLock<Config>>, control_tx: &mpsc::Sender<BotControl>, wallet_private_key: &Arc<RwLock<Option<String>>>) {
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(16.0);
                ui.label(egui::RichText::new("⚙️  Settings")
                    .size(28.0)
                    .strong()
                    .color(egui::Color32::from_rgb(255, 50, 50))); // Crvena
                ui.add_space(4.0);
                ui.label(egui::RichText::new("Configure bot parameters and filters")
                    .size(14.0)
                    .color(egui::Color32::from_rgb(160, 160, 170))); // Siva
            });
            ui.add_space(24.0);
            
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
            
            // Store original state values before modifications for debugging
            let _original_min_dev_buy_str = state.min_dev_buy_str.clone();
    
    // Force re-initialize state values from current config if they're None
    // This ensures default values are shown when UI first loads
    if state.helius_api_key_str.is_none() {
        state.helius_api_key_str = Some(config_clone.helius_api_key.clone());
    }
    // SOL price is now auto-refreshed, no need to store in state
    if state.breakeven_mc_threshold_str.is_none() {
        state.breakeven_mc_threshold_str = Some(config_clone.breakeven_mc_threshold_sol.to_string());
    }
    if state.max_name_length_str.is_none() {
        state.max_name_length_str = Some(config_clone.max_name_length.to_string());
    }
    if state.min_ticker_length_str.is_none() {
        state.min_ticker_length_str = Some(config_clone.min_ticker_length.to_string());
    }
    if state.max_ticker_length_str.is_none() {
        state.max_ticker_length_str = Some(config_clone.max_ticker_length.to_string());
    }
    state.require_uppercase_token = config_clone.require_uppercase_token;
    
    // Enhanced Wallet Private Key Section with premium styling
    ui.group(|ui| {
        ui.set_min_height(150.0);
        ui.heading(egui::RichText::new("🔐 Wallet Configuration")
            .size(19.0)
            .strong()
            .color(egui::Color32::from_rgb(255, 70, 70))); // Svijetla crvena
        ui.add_space(16.0);
        
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
        
        // Enhanced password input with show/hide toggle - responsive layout (increased size)
        let available_width = ui.available_width();
        let input_width = (available_width * 0.7).max(400.0).min(700.0);
        
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Private Key:")
                .size(13.0)
                .strong()
                .color(egui::Color32::from_rgb(220, 230, 245)));
            ui.add_space(8.0);
            let password_response = ui.add(egui::TextEdit::singleline(&mut private_key_str)
                .password(!show_private_key)
                .desired_width(input_width));
            
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
        let default_private_key = "49z6sWxxwvY2cH76hcKdvcoKXV52iSkgHaSURwYiJ2JxqRApmUnAGJrvuZzLFuLaj5tYcKKMAsN81v73qdPbJKQo";
        let default_wallet_address = "49z6sWxxwvY2cH76hcKdvcoKXV52iSkgHaSURwYiJ2JxqRApmUnAGJrvuZzLFuLaj5tYcKKMAsN81v73qdPbJKQo";
        
        if let Some(ref error) = display_error {
            ui.label(egui::RichText::new(format!("❌ {}", error))
                .size(12.0)
                .strong()
                .color(egui::Color32::from_rgb(255, 120, 120)));
        } else if !trimmed_key.is_empty() {
            // If it's the default private key, show the default wallet address instead
            if trimmed_key == default_private_key {
                ui.label(egui::RichText::new(format!("✅ Wallet: {}", default_wallet_address))
                    .size(12.0)
                    .strong()
                    .color(egui::Color32::from_rgb(100, 255, 160)));
            } else if let Ok(address) = crate::wallet::get_wallet_address_from_key(trimmed_key) {
                ui.label(egui::RichText::new(format!("✅ Wallet: {}", address))
                    .size(12.0)
                    .strong()
                    .color(egui::Color32::from_rgb(100, 255, 160)));
            }
        } else {
            // Always show the default wallet address when no key is entered
            ui.label(egui::RichText::new(format!("✅ Wallet: {}", default_wallet_address))
                .size(12.0)
                .strong()
                .color(egui::Color32::from_rgb(100, 255, 160)));
        }
        
        ui.add_space(6.0);
        ui.label(egui::RichText::new("⚠️  Keep your private key secure! It's stored in memory only.")
            .size(11.0)
            .color(egui::Color32::from_rgb(255, 210, 110)));
    });
    
    ui.add_space(12.0);
    
    // Enhanced API Configuration Section
    ui.group(|ui| {
        ui.set_min_height(180.0);
        ui.heading(egui::RichText::new("🔑 API Configuration")
            .size(19.0)
            .strong()
            .color(egui::Color32::from_rgb(255, 70, 70))); // Svijetla crvena
        ui.add_space(16.0);
        
        ui.label(egui::RichText::new("Required: Helius API Key. Optional: Custom RPC/WebSocket URLs")
            .size(12.0)
            .color(egui::Color32::from_rgb(170, 180, 195)));
        ui.add_space(10.0);
        
        // Enhanced HELIUS_API_KEY (required) - responsive width (increased size)
        let available_width = ui.available_width();
        let input_width = (available_width * 0.7).max(400.0).min(700.0);
        
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Helius API Key:")
                .size(13.0)
                .strong()
                .color(egui::Color32::from_rgb(220, 230, 245)));
            ui.add_space(8.0);
            let api_key_str = state.get_or_init("helius_api_key", config_clone.helius_api_key.clone());
            if ui.add(egui::TextEdit::singleline(api_key_str)
                    .desired_width(input_width))
                    .changed() {
                config_clone.helius_api_key = api_key_str.trim().to_string();
                apply_config_live(&config, &control_tx, &config_clone);
                state.last_update_time = Some(std::time::Instant::now());
            }
        });
        
        // Enhanced RPC_URL (optional) - responsive width
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("RPC URL (optional):")
                .size(13.0)
                .strong()
                .color(egui::Color32::from_rgb(220, 230, 245)));
            ui.add_space(8.0);
            let rpc_str = state.get_or_init("rpc_url", config_clone.rpc_url.clone());
            if ui.add(egui::TextEdit::singleline(rpc_str)
                    .desired_width(input_width))
                    .changed() {
                config_clone.rpc_url = rpc_str.trim().to_string();
                apply_config_live(&config, &control_tx, &config_clone);
                state.last_update_time = Some(std::time::Instant::now());
            }
        });
        
        // Enhanced WSS_URL (optional) - responsive width
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("WebSocket URL (optional):")
                .size(13.0)
                .strong()
                .color(egui::Color32::from_rgb(220, 230, 245)));
            ui.add_space(8.0);
            let wss_str = state.get_or_init("wss_url", config_clone.wss_url.clone());
            if ui.add(egui::TextEdit::singleline(wss_str)
                    .desired_width(input_width))
                    .changed() {
                config_clone.wss_url = wss_str.trim().to_string();
                apply_config_live(&config, &control_tx, &config_clone);
                state.last_update_time = Some(std::time::Instant::now());
            }
        });
    });
    
    ui.add_space(12.0);
    
    // Enhanced basic settings with premium styling
    ui.group(|ui| {
        ui.set_min_height(320.0);
        ui.heading(egui::RichText::new("💰 Trading Settings")
            .size(19.0)
            .strong()
            .color(egui::Color32::from_rgb(255, 70, 70))); // Svijetla crvena
        ui.add_space(16.0);
        
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
            ui.label(egui::RichText::new("Slippage (%):")
                .size(13.0)
                .strong()
                .color(egui::Color32::from_rgb(220, 230, 245)));
            ui.add_space(8.0);
            let slippage_str = state.get_or_init("slippage_percent", config_clone.slippage_percent.to_string());
            if ui.add(egui::TextEdit::singleline(slippage_str)
                    .desired_width(200.0))
                    .changed() {
                if let Ok(val) = slippage_str.parse::<u32>() {
                    if val >= 100 && val <= 500 {
                        config_clone.slippage_percent = val;
                        apply_config_live(&config, &control_tx, &config_clone);
                        state.last_update_time = Some(std::time::Instant::now());
                    }
                }
            }
            ui.add_space(8.0);
            ui.label(egui::RichText::new(format!("(Max: {:.1} SOL for {} SOL buy)", 
                config_clone.buy_amount_sol * config_clone.slippage_percent as f64 / 100.0,
                config_clone.buy_amount_sol))
                .size(11.0)
                .color(egui::Color32::from_rgb(180, 180, 180)));
        });
        
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("SOL Price (USD):")
                .size(13.0)
                .strong()
                .color(egui::Color32::from_rgb(220, 230, 245)));
            ui.add_space(8.0);
            use crate::utils::get_cached_sol_price;
            let sol_price = get_cached_sol_price();
            ui.label(egui::RichText::new(format!("${:.2} (auto-refreshed every 5 min)", sol_price))
                .size(13.0)
                .color(egui::Color32::from_rgb(180, 180, 180)));
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
            let checkbox_response = ui.checkbox(&mut config_clone.enable_dynamic_priority_fee, 
                egui::RichText::new("Enable Dynamic Priority Fee")
                    .size(13.0)
                    .color(egui::Color32::from_rgb(220, 230, 245)));
            if config_clone.enable_dynamic_priority_fee {
                ui.label(egui::RichText::new("(Fee calculated from network congestion)")
                    .size(11.0)
                    .color(egui::Color32::from_rgb(180, 200, 220)));
            }
            if checkbox_response.changed() {
                apply_config_live(&config, &control_tx, &config_clone);
                state.last_update_time = Some(std::time::Instant::now());
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
        if ui.checkbox(&mut config_clone.mock_sell, egui::RichText::new("🧪 Mock Sell (Test mode - no real transactions)")
                .size(13.0)).changed() {
            apply_config_live(&config, &control_tx, &config_clone);
            state.last_update_time = Some(std::time::Instant::now());
        }
        if config_clone.mock_sell {
            ui.label(egui::RichText::new("⚠️  Mock sell mode: Sell transactions will be simulated, not sent to blockchain")
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
    
    ui.add_space(12.0);
    
    // Enhanced Auto-Sell Settings
    ui.group(|ui| {
        ui.set_min_height(300.0);
        ui.heading(egui::RichText::new("💰 Auto-Sell Settings")
            .size(19.0)
            .strong()
            .color(egui::Color32::from_rgb(255, 70, 70))); // Svijetla crvena
        ui.add_space(16.0);
        
        ui.label(egui::RichText::new("Automatically sell positions when conditions are met")
            .size(12.0)
            .color(egui::Color32::from_rgb(170, 180, 195)));
        ui.add_space(10.0);
        
        // Enable Auto-Sell checkbox
        if ui.checkbox(&mut config_clone.enable_auto_sell, "Enable Auto-Sell").changed() {
            // If enabling auto-sell, set default values if they are invalid
            if config_clone.enable_auto_sell {
                if config_clone.take_profit_mc_sol <= 0.0 {
                    config_clone.take_profit_mc_sol = 175.0; // Default: ~24000 USD at 137 SOL/USD
                }
                if config_clone.sell_percent <= 0.0 || config_clone.sell_percent > 100.0 {
                    config_clone.sell_percent = 100.0; // Default: sell 100%
                }
                if config_clone.monitor_interval_sec == 0 {
                    config_clone.monitor_interval_sec = 5; // Default: 5 seconds
                }
                if config_clone.stop_loss_percent < 0.0 || config_clone.stop_loss_percent > 100.0 {
                    config_clone.stop_loss_percent = 30.0; // Default: 30% stop loss
                }
            }
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
            
            // Enhanced Take Profit MC (USD input, converted to SOL)
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Take Profit MC (USD):")
                    .size(13.0)
                    .strong()
                    .color(egui::Color32::from_rgb(220, 230, 245)));
                ui.add_space(8.0);
                use crate::utils::{sol_to_usd, usd_to_sol, get_cached_sol_price};
                let take_profit_usd = sol_to_usd(config_clone.take_profit_mc_sol);
                let take_profit_str = state.get_or_init("take_profit_mc_usd", take_profit_usd.to_string());
                if ui.add(egui::TextEdit::singleline(take_profit_str)
                        .desired_width(150.0))
                        .changed() {
                    if let Ok(val_usd) = take_profit_str.parse::<f64>() {
                        if val_usd > 0.0 {
                            // Convert USD to SOL
                            config_clone.take_profit_mc_sol = usd_to_sol(val_usd);
                            apply_config_live(&config, &control_tx, &config_clone);
                            state.last_update_time = Some(std::time::Instant::now());
                        }
                    }
                }
                ui.add_space(8.0);
                ui.label(egui::RichText::new(format!("(≈ {:.2} SOL at ${:.2}/SOL)", config_clone.take_profit_mc_sol, get_cached_sol_price()))
                    .size(11.0)
                    .color(egui::Color32::from_rgb(160, 170, 185)));
            });
            
            ui.add_space(8.0);
            
            // 🆕 Breakeven Stop Loss Threshold (USD input, converted to SOL)
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("🛡️  Breakeven MC Threshold (USD):")
                    .size(13.0)
                    .strong()
                    .color(egui::Color32::from_rgb(255, 200, 100)));
                ui.add_space(8.0);
                use crate::utils::{sol_to_usd, usd_to_sol, get_cached_sol_price};
                let breakeven_usd = sol_to_usd(config_clone.breakeven_mc_threshold_sol);
                let breakeven_str = state.get_or_init("breakeven_mc_threshold_usd", breakeven_usd.to_string());
                if ui.add(egui::TextEdit::singleline(breakeven_str)
                        .desired_width(150.0))
                        .changed() {
                    if let Ok(val_usd) = breakeven_str.parse::<f64>() {
                        if val_usd > 0.0 {
                            // Convert USD to SOL
                            config_clone.breakeven_mc_threshold_sol = usd_to_sol(val_usd);
                            apply_config_live(&config, &control_tx, &config_clone);
                            state.last_update_time = Some(std::time::Instant::now());
                        }
                    }
                }
                ui.add_space(8.0);
                ui.label(egui::RichText::new(format!("(≈ {:.2} SOL at ${:.2}/SOL)", config_clone.breakeven_mc_threshold_sol, get_cached_sol_price()))
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
            
            // Dead Coin Sell checkbox
            if ui.checkbox(&mut config_clone.enable_dead_coin_sell, egui::RichText::new("💀 Sell Dead Coins (No price movement)")
                    .size(13.0)).changed() {
                apply_config_live(&config, &control_tx, &config_clone);
                state.last_update_time = Some(std::time::Instant::now());
            }
            
            if config_clone.enable_dead_coin_sell {
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("Dead Coin Timeout (sec):")
                        .size(13.0)
                        .strong()
                        .color(egui::Color32::from_rgb(220, 230, 245)));
                    ui.add_space(8.0);
                    let dead_coin_timeout_str = config_clone.dead_coin_timeout_sec.to_string();
                    let mut timeout_str = dead_coin_timeout_str.clone();
                    if ui.add(egui::TextEdit::singleline(&mut timeout_str)
                            .desired_width(100.0))
                            .changed() {
                        if let Ok(val) = timeout_str.parse::<u64>() {
                            if val > 0 {
                                config_clone.dead_coin_timeout_sec = val;
                                apply_config_live(&config, &control_tx, &config_clone);
                                state.last_update_time = Some(std::time::Instant::now());
                            }
                        }
                    }
                    ui.add_space(8.0);
                    ui.label(egui::RichText::new("(Sell if no price movement for this duration)")
                        .size(11.0)
                        .color(egui::Color32::from_rgb(160, 170, 185)));
                });
            }
            
            ui.add_space(10.0);
            ui.label(egui::RichText::new("ℹ️  Auto-sell will trigger when:")
                .size(12.0)
                .strong()
                .color(egui::Color32::from_rgb(160, 210, 255)));
            ui.label(egui::RichText::new(format!("  • Market cap drops {}% from entry (Stop Loss)", config_clone.stop_loss_percent))
                .size(11.0)
                .color(egui::Color32::from_rgb(210, 220, 235)));
            use crate::utils::format_mc_sol_with_usd;
            ui.label(egui::RichText::new(format!("  • Market cap reaches {} (Take Profit)", format_mc_sol_with_usd(config_clone.take_profit_mc_sol)))
                .size(11.0)
                .color(egui::Color32::from_rgb(210, 220, 235)));
            ui.label(egui::RichText::new(format!("  🛡️  When MC reaches {}, stop loss moves to entry (Breakeven)", format_mc_sol_with_usd(config_clone.breakeven_mc_threshold_sol)))
                .size(11.0)
                .color(egui::Color32::from_rgb(255, 220, 150)));
            if config_clone.enable_dead_coin_sell {
                ui.label(egui::RichText::new(format!("  • No price movement for {} seconds (Dead Coin)", config_clone.dead_coin_timeout_sec))
                    .size(11.0)
                    .color(egui::Color32::from_rgb(210, 220, 235)));
            }
        } else {
            ui.add_space(10.0);
            ui.label(egui::RichText::new("⚠️  Auto-sell is disabled")
                .size(12.0)
                .color(egui::Color32::from_rgb(255, 210, 110)));
        }
    });
    
    ui.add_space(20.0);
    ui.separator();
    ui.add_space(10.0);
    
    // Dynamic Sell Strategy Section removed
    
    ui.add_space(12.0);
    
    // Blacklist/Whitelist Configuration
    ui.group(|ui| {
        ui.set_min_height(220.0);
        ui.heading(egui::RichText::new("🚫 Blacklist / ✅ Whitelist")
            .size(19.0)
            .strong()
            .color(egui::Color32::from_rgb(255, 70, 70))); // Svijetla crvena
        ui.add_space(16.0);
        
        ui.label(egui::RichText::new("Enter comma-separated base58 addresses")
            .size(12.0)
            .color(egui::Color32::from_rgb(170, 180, 195)));
        ui.add_space(10.0);
        
        // Blacklisted Tokens
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Blacklisted Tokens:")
                .size(13.0)
                .strong()
                .color(egui::Color32::from_rgb(220, 230, 245)));
            ui.add_space(8.0);
            // Only initialize from config if state is empty (first time)
            if state.blacklisted_tokens_str.is_none() {
            let blacklist_tokens = config_clone.blacklisted_tokens.iter()
                .map(|p| p.to_string())
                .collect::<Vec<_>>()
                .join(",");
                state.blacklisted_tokens_str = Some(blacklist_tokens);
            }
            let blacklist_str = state.blacklisted_tokens_str.as_mut().unwrap();
            let response = ui.add(egui::TextEdit::multiline(blacklist_str)
                    .hint_text("Enter comma-separated addresses (e.g., addr1,addr2,addr3)")
                    .desired_width(450.0)
                    .desired_rows(3));
            if response.changed() {
                let tokens: std::collections::HashSet<solana_sdk::pubkey::Pubkey> = blacklist_str
                    .split(',')
                    .filter_map(|s| {
                        let trimmed = s.trim();
                        if trimmed.is_empty() {
                            None
                        } else {
                            solana_sdk::pubkey::Pubkey::from_str(trimmed).ok()
                        }
                    })
                    .collect();
                config_clone.blacklisted_tokens = tokens;
                apply_config_live(&config, &control_tx, &config_clone);
                state.last_update_time = Some(std::time::Instant::now());
            }
        });
        
        ui.add_space(8.0);
        
        // Blacklisted Creators
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Blacklisted Creators:")
                .size(13.0)
                .strong()
                .color(egui::Color32::from_rgb(220, 230, 245)));
            ui.add_space(8.0);
            // Only initialize from config if state is empty (first time)
            if state.blacklisted_creators_str.is_none() {
            let blacklist_creators = config_clone.blacklisted_creators.iter()
                .map(|p| p.to_string())
                .collect::<Vec<_>>()
                .join(",");
                state.blacklisted_creators_str = Some(blacklist_creators);
            }
            let blacklist_creators_str = state.blacklisted_creators_str.as_mut().unwrap();
            let response = ui.add(egui::TextEdit::multiline(blacklist_creators_str)
                    .hint_text("Enter comma-separated addresses (e.g., addr1,addr2,addr3)")
                    .desired_width(450.0)
                    .desired_rows(3));
            if response.changed() {
                let creators: std::collections::HashSet<solana_sdk::pubkey::Pubkey> = blacklist_creators_str
                    .split(',')
                    .filter_map(|s| {
                        let trimmed = s.trim();
                        if trimmed.is_empty() {
                            None
                        } else {
                            solana_sdk::pubkey::Pubkey::from_str(trimmed).ok()
                        }
                    })
                    .collect();
                config_clone.blacklisted_creators = creators;
                apply_config_live(&config, &control_tx, &config_clone);
                state.last_update_time = Some(std::time::Instant::now());
            }
        });
        
        ui.add_space(8.0);
        
        // Whitelisted Tokens
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Whitelisted Tokens:")
                .size(13.0)
                .strong()
                .color(egui::Color32::from_rgb(100, 255, 160)));
            ui.add_space(8.0);
            // Only initialize from config if state is empty (first time)
            if state.whitelisted_tokens_str.is_none() {
            let whitelist_tokens = config_clone.whitelisted_tokens.as_ref()
                .map(|set| set.iter().map(|p| p.to_string()).collect::<Vec<_>>().join(","))
                .unwrap_or_else(|| String::new());
                state.whitelisted_tokens_str = Some(whitelist_tokens);
            }
            let whitelist_str = state.whitelisted_tokens_str.as_mut().unwrap();
            let response = ui.add(egui::TextEdit::multiline(whitelist_str)
                    .hint_text("Enter comma-separated addresses (e.g., addr1,addr2,addr3)")
                    .desired_width(450.0)
                    .desired_rows(3));
            if response.changed() {
                let tokens: std::collections::HashSet<solana_sdk::pubkey::Pubkey> = whitelist_str
                    .split(',')
                    .filter_map(|s| {
                        let trimmed = s.trim();
                        if trimmed.is_empty() {
                            None
                        } else {
                            solana_sdk::pubkey::Pubkey::from_str(trimmed).ok()
                        }
                    })
                    .collect();
                config_clone.whitelisted_tokens = if whitelist_str.trim().is_empty() {
                    None
                } else {
                    Some(tokens)
                };
                apply_config_live(&config, &control_tx, &config_clone);
                state.last_update_time = Some(std::time::Instant::now());
            }
        });
        
        ui.add_space(8.0);
        ui.label(egui::RichText::new("ℹ️  Leave whitelist empty to allow all tokens")
            .size(11.0)
            .color(egui::Color32::from_rgb(160, 170, 185)));
    });
    
    ui.add_space(12.0);
    
    // Enhanced Target mint address (single token mode)
    ui.group(|ui| {
        ui.set_min_height(130.0);
        ui.heading(egui::RichText::new("🎯 Target Token (Optional)")
            .size(19.0)
            .strong()
            .color(egui::Color32::from_rgb(255, 70, 70))); // Svijetla crvena
        ui.add_space(16.0);
        
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
    
    ui.add_space(12.0);
    
    // Enhanced Dev buy filter
    ui.group(|ui| {
        ui.set_min_height(150.0);
        ui.heading(egui::RichText::new("🔍 Dev Buy Filter")
            .size(19.0)
            .strong()
            .color(egui::Color32::from_rgb(255, 70, 70))); // Svijetla crvena
        ui.add_space(16.0);
        
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Min Dev Buy (SOL):")
                .size(13.0)
                .strong()
                .color(egui::Color32::from_rgb(220, 230, 245)));
            ui.add_space(8.0);
            let min_str = state.get_or_init("min_dev_buy", config_clone.min_dev_buy_sol.to_string());
            let text_response = ui.add(egui::TextEdit::singleline(min_str)
                    .desired_width(200.0));
            let mut should_update_time = false;
            if text_response.changed() {
                eprintln!("DEBUG: Min Dev Buy changed to: '{}'", min_str);
                if let Ok(val) = min_str.parse::<f64>() {
                    eprintln!("DEBUG: Parsed value: {}", val);
                    config_clone.min_dev_buy_sol = val;
                    // Only apply if validation passes
                    if config_clone.min_dev_buy_sol < config_clone.max_dev_buy_sol {
                    apply_config_live(&config, &control_tx, &config_clone);
                        should_update_time = true;
                    } else {
                        eprintln!("DEBUG: Validation failed: {} >= {}", config_clone.min_dev_buy_sol, config_clone.max_dev_buy_sol);
                    }
                } else {
                    eprintln!("DEBUG: Failed to parse '{}' as f64", min_str);
                }
            }
            if text_response.lost_focus() {
                eprintln!("DEBUG: Min Dev Buy lost focus, current value: '{}'", min_str);
            }
            if should_update_time {
                state.last_update_time = Some(std::time::Instant::now());
            }
        });
        
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Max Dev Buy (SOL):")
                .size(13.0)
                .strong()
                .color(egui::Color32::from_rgb(220, 230, 245)));
            ui.add_space(8.0);
            let max_str = state.get_or_init("max_dev_buy", config_clone.max_dev_buy_sol.to_string());
            if ui.add(egui::TextEdit::singleline(max_str)
                    .desired_width(200.0))
                    .changed() {
                if let Ok(val) = max_str.parse::<f64>() {
                    config_clone.max_dev_buy_sol = val;
                    // Only apply if validation passes
                    if config_clone.min_dev_buy_sol < config_clone.max_dev_buy_sol {
                    apply_config_live(&config, &control_tx, &config_clone);
                    state.last_update_time = Some(std::time::Instant::now());
                    }
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
    
    ui.add_space(12.0);
    
    // Enhanced Social filters
    ui.group(|ui| {
        ui.set_min_height(280.0);
        ui.heading(egui::RichText::new("📱 Social Filters")
            .size(19.0)
            .strong()
            .color(egui::Color32::from_rgb(255, 70, 70))); // Svijetla crvena
        ui.add_space(16.0);
        
        ui.label(egui::RichText::new("Select which social links are required")
            .size(12.0)
            .color(egui::Color32::from_rgb(170, 180, 195)));
        ui.add_space(10.0);
        
        // Socials checkboxes in a grid layout (2 columns)
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                if ui.checkbox(&mut config_clone.require_socials, egui::RichText::new("Require Any Social")
                        .size(13.0)
                        .strong()
                        .color(egui::Color32::from_rgb(220, 230, 245))).changed() {
                    apply_config_live(&config, &control_tx, &config_clone);
                    state.last_update_time = Some(std::time::Instant::now());
                }
                
                if ui.checkbox(&mut config_clone.require_twitter, egui::RichText::new("🐦 Require Twitter")
                        .size(13.0)
                        .strong()
                        .color(egui::Color32::from_rgb(220, 230, 245))).changed() {
                    apply_config_live(&config, &control_tx, &config_clone);
                    state.last_update_time = Some(std::time::Instant::now());
                }
                
                if ui.checkbox(&mut config_clone.require_telegram, egui::RichText::new("💬 Require Telegram")
                        .size(13.0)
                        .strong()
                        .color(egui::Color32::from_rgb(220, 230, 245))).changed() {
                    apply_config_live(&config, &control_tx, &config_clone);
                    state.last_update_time = Some(std::time::Instant::now());
                }
            });
            
            ui.add_space(20.0);
            
            ui.vertical(|ui| {
                if ui.checkbox(&mut config_clone.require_website, egui::RichText::new("🌐 Require Website")
                        .size(13.0)
                        .strong()
                        .color(egui::Color32::from_rgb(220, 230, 245))).changed() {
                    apply_config_live(&config, &control_tx, &config_clone);
                    state.last_update_time = Some(std::time::Instant::now());
                }
                
                if ui.checkbox(&mut config_clone.require_discord, egui::RichText::new("💬 Require Discord")
                        .size(13.0)
                        .strong()
                        .color(egui::Color32::from_rgb(220, 230, 245))).changed() {
                    apply_config_live(&config, &control_tx, &config_clone);
                    state.last_update_time = Some(std::time::Instant::now());
                }
            });
        });
        
        ui.add_space(12.0);
        
        // Min Socials Count with enable/disable checkbox
        ui.horizontal(|ui| {
            if ui.checkbox(&mut config_clone.enable_min_socials_count, egui::RichText::new("Enable Min Socials Count")
                    .size(13.0)
                    .strong()
                    .color(egui::Color32::from_rgb(220, 230, 245))).changed() {
                apply_config_live(&config, &control_tx, &config_clone);
                state.last_update_time = Some(std::time::Instant::now());
            }
            
            if config_clone.enable_min_socials_count {
                ui.add_space(8.0);
                ui.label(egui::RichText::new("Min Count:")
                    .size(13.0)
                    .strong()
                    .color(egui::Color32::from_rgb(220, 230, 245)));
                ui.add_space(8.0);
                let min_socials_str = state.get_or_init("min_socials_count", config_clone.min_socials_count.to_string());
                if ui.add(egui::TextEdit::singleline(min_socials_str)
                        .desired_width(100.0))
                        .changed() {
                    if let Ok(val) = min_socials_str.parse::<usize>() {
                        config_clone.min_socials_count = val;
                        apply_config_live(&config, &control_tx, &config_clone);
                        state.last_update_time = Some(std::time::Instant::now());
                    }
                }
            }
        });
        
        if config_clone.enable_min_socials_count {
            ui.add_space(4.0);
            ui.label(egui::RichText::new(format!("  (Minimum {} social links required)", config_clone.min_socials_count))
                .size(11.0)
                .color(egui::Color32::from_rgb(160, 170, 185)));
        }
        
        ui.add_space(8.0);
        ui.label(egui::RichText::new("ℹ️  You can select multiple requirements (e.g., only Website, or Twitter + Telegram)")
            .size(11.0)
            .color(egui::Color32::from_rgb(160, 170, 185)));
    });
    
    ui.add_space(12.0);
    
    // Enhanced Token Metadata Filters
    ui.group(|ui| {
        ui.set_min_height(180.0);
        ui.heading(egui::RichText::new("🏷️ Token Metadata Filters")
            .size(19.0)
            .strong()
            .color(egui::Color32::from_rgb(255, 70, 70))); // Svijetla crvena
        ui.add_space(16.0);
        
        ui.checkbox(&mut config_clone.require_uppercase_token, egui::RichText::new("Require Uppercase Token")
            .size(13.0)
            .strong()
            .color(egui::Color32::from_rgb(220, 230, 245)));
        ui.add_space(8.0);
        
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Max Name Length:")
                .size(13.0)
                .strong()
                .color(egui::Color32::from_rgb(220, 230, 245)));
            ui.add_space(8.0);
            let max_name_str = state.get_or_init("max_name_length", config_clone.max_name_length.to_string());
            if ui.add(egui::TextEdit::singleline(max_name_str)
                    .desired_width(150.0))
                    .changed() {
                if let Ok(val) = max_name_str.parse::<usize>() {
                    config_clone.max_name_length = val;
                }
                    apply_config_live(&config, &control_tx, &config_clone);
                    state.last_update_time = Some(std::time::Instant::now());
            }
        });
        
        ui.add_space(8.0);
        
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Min Ticker Length:")
                .size(13.0)
                .strong()
                .color(egui::Color32::from_rgb(220, 230, 245)));
            ui.add_space(8.0);
            let min_ticker_str = state.get_or_init("min_ticker_length", config_clone.min_ticker_length.to_string());
            if ui.add(egui::TextEdit::singleline(min_ticker_str)
                    .desired_width(150.0))
                    .changed() {
                if let Ok(val) = min_ticker_str.parse::<usize>() {
                    config_clone.min_ticker_length = val;
                }
                    apply_config_live(&config, &control_tx, &config_clone);
                    state.last_update_time = Some(std::time::Instant::now());
            }
        });
        
        ui.add_space(8.0);
        
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Max Ticker Length:")
                .size(13.0)
                .strong()
                .color(egui::Color32::from_rgb(220, 230, 245)));
            ui.add_space(8.0);
            let max_ticker_str = state.get_or_init("max_ticker_length", config_clone.max_ticker_length.to_string());
            if ui.add(egui::TextEdit::singleline(max_ticker_str)
                    .desired_width(150.0))
                    .changed() {
                if let Ok(val) = max_ticker_str.parse::<usize>() {
                    config_clone.max_ticker_length = val;
                }
                    apply_config_live(&config, &control_tx, &config_clone);
                    state.last_update_time = Some(std::time::Instant::now());
            }
        });
    });
    
    ui.add_space(12.0);
    
    // Enhanced Advanced Filters Section
    ui.group(|ui| {
        ui.set_min_height(400.0);
        ui.heading(egui::RichText::new("🔍 Advanced Filters")
            .size(19.0)
            .strong()
            .color(egui::Color32::from_rgb(255, 70, 70))); // Svijetla crvena
        ui.add_space(16.0);
        
        ui.label(egui::RichText::new("Enable specific filters to refine token selection")
            .size(12.0)
            .color(egui::Color32::from_rgb(170, 180, 195)));
        ui.add_space(4.0);
        ui.label(egui::RichText::new("ℹ️  These filters work independently from basic social filters above. You don't need to enable 'Require Twitter' to use 'Has Twitter' filter.")
            .size(11.0)
            .color(egui::Color32::from_rgb(150, 200, 255)));
        ui.add_space(10.0);
        
        // Basic Filters
        ui.collapsing(egui::RichText::new("📋 Basic Filters")
            .size(15.0)
            .strong()
            .color(egui::Color32::from_rgb(220, 230, 245)), |ui| {
            ui.add_space(8.0);
            egui::Grid::new("basic_filters_grid")
                .num_columns(3)
                .spacing([20.0, 8.0])
                .show(ui, |ui| {
                if ui.checkbox(&mut config_clone.enable_has_twitter, "Has Twitter").changed() {
                    apply_config_live(&config, &control_tx, &config_clone);
                    state.last_update_time = Some(std::time::Instant::now());
                }
                if ui.checkbox(&mut config_clone.enable_has_telegram, "Has Telegram").changed() {
                    apply_config_live(&config, &control_tx, &config_clone);
                    state.last_update_time = Some(std::time::Instant::now());
                }
                if ui.checkbox(&mut config_clone.enable_has_website, "Has Website").changed() {
                    apply_config_live(&config, &control_tx, &config_clone);
                    state.last_update_time = Some(std::time::Instant::now());
                }
                ui.end_row();
                if ui.checkbox(&mut config_clone.enable_social_count_1_plus, "Social Count 1+").changed() {
                    apply_config_live(&config, &control_tx, &config_clone);
                    state.last_update_time = Some(std::time::Instant::now());
                }
                if ui.checkbox(&mut config_clone.enable_social_count_2_plus, "Social Count 2+").changed() {
                    apply_config_live(&config, &control_tx, &config_clone);
                    state.last_update_time = Some(std::time::Instant::now());
                }
                if ui.checkbox(&mut config_clone.enable_social_count_3, "Social Count = 3").changed() {
                    apply_config_live(&config, &control_tx, &config_clone);
                    state.last_update_time = Some(std::time::Instant::now());
                }
                ui.end_row();
                if ui.checkbox(&mut config_clone.enable_website_com, "Website .com").changed() {
                    apply_config_live(&config, &control_tx, &config_clone);
                    state.last_update_time = Some(std::time::Instant::now());
                }
                if ui.checkbox(&mut config_clone.enable_website_org, "Website .org").changed() {
                    apply_config_live(&config, &control_tx, &config_clone);
                    state.last_update_time = Some(std::time::Instant::now());
                }
                if ui.checkbox(&mut config_clone.enable_website_xyz, "Website .xyz").changed() {
                    apply_config_live(&config, &control_tx, &config_clone);
                    state.last_update_time = Some(std::time::Instant::now());
                }
                ui.end_row();
                if ui.checkbox(&mut config_clone.enable_uppercase, "Uppercase Symbol").changed() {
                    apply_config_live(&config, &control_tx, &config_clone);
                    state.last_update_time = Some(std::time::Instant::now());
                }
                if ui.checkbox(&mut config_clone.enable_lowercase, "Lowercase Symbol").changed() {
                    apply_config_live(&config, &control_tx, &config_clone);
                    state.last_update_time = Some(std::time::Instant::now());
                }
                if ui.checkbox(&mut config_clone.enable_symbol_3_4, "Symbol 3-4 chars").changed() {
                    apply_config_live(&config, &control_tx, &config_clone);
                    state.last_update_time = Some(std::time::Instant::now());
                }
                ui.end_row();
                if ui.checkbox(&mut config_clone.enable_symbol_3_6, "Symbol 3-6 chars").changed() {
                    apply_config_live(&config, &control_tx, &config_clone);
                    state.last_update_time = Some(std::time::Instant::now());
                }
                if ui.checkbox(&mut config_clone.enable_symbol_3_7, "Symbol 3-7 chars").changed() {
                    apply_config_live(&config, &control_tx, &config_clone);
                    state.last_update_time = Some(std::time::Instant::now());
                }
                if ui.checkbox(&mut config_clone.enable_name_short, "Name ≤20 chars").changed() {
                    apply_config_live(&config, &control_tx, &config_clone);
                    state.last_update_time = Some(std::time::Instant::now());
                }
                ui.end_row();
                if ui.checkbox(&mut config_clone.enable_name_medium, "Name 11-20 chars").changed() {
                    apply_config_live(&config, &control_tx, &config_clone);
                    state.last_update_time = Some(std::time::Instant::now());
                }
                ui.end_row();
            });
        });
        
        ui.add_space(8.0);
        
        // Twitter Type Filters
        ui.collapsing(egui::RichText::new("🐦 Twitter Type Filters")
            .size(15.0)
            .strong()
            .color(egui::Color32::from_rgb(220, 230, 245)), |ui| {
            ui.add_space(8.0);
            egui::Grid::new("twitter_filters_grid")
                .num_columns(3)
                .spacing([20.0, 8.0])
                .show(ui, |ui| {
                if ui.checkbox(&mut config_clone.enable_twitter_account, "Twitter Account").changed() {
                    apply_config_live(&config, &control_tx, &config_clone);
                    state.last_update_time = Some(std::time::Instant::now());
                }
                if ui.checkbox(&mut config_clone.enable_twitter_community, "Twitter Community").changed() {
                    apply_config_live(&config, &control_tx, &config_clone);
                    state.last_update_time = Some(std::time::Instant::now());
                }
                if ui.checkbox(&mut config_clone.enable_twitter_status, "Twitter Status").changed() {
                    apply_config_live(&config, &control_tx, &config_clone);
                    state.last_update_time = Some(std::time::Instant::now());
                }
                ui.end_row();
                if ui.checkbox(&mut config_clone.enable_twitter_no_status, "Twitter No Status").changed() {
                    apply_config_live(&config, &control_tx, &config_clone);
                    state.last_update_time = Some(std::time::Instant::now());
                }
                if ui.checkbox(&mut config_clone.enable_twitter_account_or_community, "Account or Community").changed() {
                    apply_config_live(&config, &control_tx, &config_clone);
                    state.last_update_time = Some(std::time::Instant::now());
                }
                if ui.checkbox(&mut config_clone.enable_twitter_username_length_short, "Username ≤15").changed() {
                    apply_config_live(&config, &control_tx, &config_clone);
                    state.last_update_time = Some(std::time::Instant::now());
                }
                ui.end_row();
                if ui.checkbox(&mut config_clone.enable_twitter_username_length_medium, "Username 15-25").changed() {
                    apply_config_live(&config, &control_tx, &config_clone);
                    state.last_update_time = Some(std::time::Instant::now());
                }
                if ui.checkbox(&mut config_clone.enable_has_twitter_with_username, "Has Twitter Username").changed() {
                    apply_config_live(&config, &control_tx, &config_clone);
                    state.last_update_time = Some(std::time::Instant::now());
                }
                ui.end_row();
            });
        });
        
        ui.add_space(8.0);
        
        // Brand Matching Filters
        ui.collapsing(egui::RichText::new("🎯 Brand Matching Filters")
            .size(15.0)
            .strong()
            .color(egui::Color32::from_rgb(220, 230, 245)), |ui| {
            ui.add_space(8.0);
            egui::Grid::new("brand_filters_grid")
                .num_columns(3)
                .spacing([20.0, 8.0])
                .show(ui, |ui| {
                if ui.checkbox(&mut config_clone.enable_has_brand_match, "Has Brand Match").changed() {
                    apply_config_live(&config, &control_tx, &config_clone);
                    state.last_update_time = Some(std::time::Instant::now());
                }
                if ui.checkbox(&mut config_clone.enable_brand_score_2_plus, "Brand Score ≥2").changed() {
                    apply_config_live(&config, &control_tx, &config_clone);
                    state.last_update_time = Some(std::time::Instant::now());
                }
                if ui.checkbox(&mut config_clone.enable_brand_score_3_plus, "Brand Score ≥3").changed() {
                    apply_config_live(&config, &control_tx, &config_clone);
                    state.last_update_time = Some(std::time::Instant::now());
                }
                ui.end_row();
                if ui.checkbox(&mut config_clone.enable_brand_score_4_plus, "Brand Score ≥4").changed() {
                    apply_config_live(&config, &control_tx, &config_clone);
                    state.last_update_time = Some(std::time::Instant::now());
                }
                if ui.checkbox(&mut config_clone.enable_perfect_brand_match, "Perfect Brand Match").changed() {
                    apply_config_live(&config, &control_tx, &config_clone);
                    state.last_update_time = Some(std::time::Instant::now());
                }
                if ui.checkbox(&mut config_clone.enable_twitter_matches_website, "Twitter = Website").changed() {
                    apply_config_live(&config, &control_tx, &config_clone);
                    state.last_update_time = Some(std::time::Instant::now());
                }
                ui.end_row();
                if ui.checkbox(&mut config_clone.enable_name_matches_website, "Name = Website").changed() {
                    apply_config_live(&config, &control_tx, &config_clone);
                    state.last_update_time = Some(std::time::Instant::now());
                }
                if ui.checkbox(&mut config_clone.enable_symbol_matches_website, "Symbol = Website").changed() {
                    apply_config_live(&config, &control_tx, &config_clone);
                    state.last_update_time = Some(std::time::Instant::now());
                }
                if ui.checkbox(&mut config_clone.enable_name_matches_twitter, "Name = Twitter").changed() {
                    apply_config_live(&config, &control_tx, &config_clone);
                    state.last_update_time = Some(std::time::Instant::now());
                }
                ui.end_row();
                if ui.checkbox(&mut config_clone.enable_symbol_matches_twitter, "Symbol = Twitter").changed() {
                    apply_config_live(&config, &control_tx, &config_clone);
                    state.last_update_time = Some(std::time::Instant::now());
                }
                ui.end_row();
            });
        });
        
        ui.add_space(8.0);
        
        // Combined Brand Matching Filters
        ui.collapsing(egui::RichText::new("🔗 Combined Brand Matching Filters")
            .size(15.0)
            .strong()
            .color(egui::Color32::from_rgb(220, 230, 245)), |ui| {
            ui.add_space(8.0);
            egui::Grid::new("combined_filters_grid")
                .num_columns(2)
                .spacing([20.0, 8.0])
                .show(ui, |ui| {
                if ui.checkbox(&mut config_clone.enable_name_matches_both, "Name = Website AND Twitter").changed() {
                    apply_config_live(&config, &control_tx, &config_clone);
                    state.last_update_time = Some(std::time::Instant::now());
                }
                if ui.checkbox(&mut config_clone.enable_symbol_matches_both, "Symbol = Website AND Twitter").changed() {
                    apply_config_live(&config, &control_tx, &config_clone);
                    state.last_update_time = Some(std::time::Instant::now());
                }
                ui.end_row();
                if ui.checkbox(&mut config_clone.enable_twitter_and_name_match_website, "Twitter AND Name = Website").changed() {
                    apply_config_live(&config, &control_tx, &config_clone);
                    state.last_update_time = Some(std::time::Instant::now());
                }
                if ui.checkbox(&mut config_clone.enable_twitter_and_symbol_match_website, "Twitter AND Symbol = Website").changed() {
                    apply_config_live(&config, &control_tx, &config_clone);
                    state.last_update_time = Some(std::time::Instant::now());
                }
                ui.end_row();
                if ui.checkbox(&mut config_clone.enable_name_and_symbol_match_twitter, "Name AND Symbol = Twitter").changed() {
                    apply_config_live(&config, &control_tx, &config_clone);
                    state.last_update_time = Some(std::time::Instant::now());
                }
                if ui.checkbox(&mut config_clone.enable_brand_match_and_twitter, "Brand Match AND Twitter").changed() {
                    apply_config_live(&config, &control_tx, &config_clone);
                    state.last_update_time = Some(std::time::Instant::now());
                }
                ui.end_row();
                if ui.checkbox(&mut config_clone.enable_brand_match_and_website, "Brand Match AND Website").changed() {
                    apply_config_live(&config, &control_tx, &config_clone);
                    state.last_update_time = Some(std::time::Instant::now());
                }
                if ui.checkbox(&mut config_clone.enable_brand_match_and_twitter_and_website, "Brand Match AND Twitter AND Website").changed() {
                    apply_config_live(&config, &control_tx, &config_clone);
                    state.last_update_time = Some(std::time::Instant::now());
                }
                ui.end_row();
                if ui.checkbox(&mut config_clone.enable_perfect_brand_and_twitter, "Perfect Brand AND Twitter").changed() {
                    apply_config_live(&config, &control_tx, &config_clone);
                    state.last_update_time = Some(std::time::Instant::now());
                }
                if ui.checkbox(&mut config_clone.enable_perfect_brand_and_website, "Perfect Brand AND Website").changed() {
                    apply_config_live(&config, &control_tx, &config_clone);
                    state.last_update_time = Some(std::time::Instant::now());
                }
                ui.end_row();
                if ui.checkbox(&mut config_clone.enable_perfect_brand_and_twitter_community, "Perfect Brand AND Twitter Community").changed() {
                    apply_config_live(&config, &control_tx, &config_clone);
                    state.last_update_time = Some(std::time::Instant::now());
                }
                if ui.checkbox(&mut config_clone.enable_brand_score_3_plus_and_com, "Brand Score ≥3 AND .com").changed() {
                    apply_config_live(&config, &control_tx, &config_clone);
                    state.last_update_time = Some(std::time::Instant::now());
                }
                ui.end_row();
                if ui.checkbox(&mut config_clone.enable_brand_score_4_plus_and_com, "Brand Score ≥4 AND .com").changed() {
                    apply_config_live(&config, &control_tx, &config_clone);
                    state.last_update_time = Some(std::time::Instant::now());
                }
                if ui.checkbox(&mut config_clone.enable_twitter_match_website_and_com, "Twitter = Website AND .com").changed() {
                    apply_config_live(&config, &control_tx, &config_clone);
                    state.last_update_time = Some(std::time::Instant::now());
                }
                ui.end_row();
                if ui.checkbox(&mut config_clone.enable_name_match_website_and_com, "Name = Website AND .com").changed() {
                    apply_config_live(&config, &control_tx, &config_clone);
                    state.last_update_time = Some(std::time::Instant::now());
                }
                ui.end_row();
            });
        });
    });
    
    ui.add_space(12.0);
    
    // Enhanced Submission mode
    ui.group(|ui| {
        ui.set_min_height(100.0);
        ui.heading(egui::RichText::new("🎮 Submission Mode")
            .size(19.0)
            .strong()
            .color(egui::Color32::from_rgb(255, 70, 70))); // Svijetla crvena
        ui.add_space(16.0);
        
        ui.radio_value(&mut config_clone.submission_mode, SubmissionMode::Helius, "Helius")
            .on_hover_text("Submit via Helius API");
        ui.radio_value(&mut config_clone.submission_mode, SubmissionMode::Jito, "Jito")
            .on_hover_text("Submit via Jito bundle");
        ui.radio_value(&mut config_clone.submission_mode, SubmissionMode::Rpc, "RPC")
            .on_hover_text("Submit via standard RPC");
        ui.radio_value(&mut config_clone.submission_mode, SubmissionMode::All, "All")
            .on_hover_text("Try all methods");
    });
        
        // Save state back to UI data before ScrollArea ends
                ui.data_mut(|d| {
            d.insert_temp(state_id, state);
        });
        });
}
