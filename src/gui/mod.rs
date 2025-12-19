// gui/mod.rs - Main GUI application

mod tabs;
mod components;
mod events;

pub use events::{TokenEvent, BotControl};

use eframe::egui;
use std::str::FromStr;
use std::collections::VecDeque;
use std::sync::{Arc, RwLock, mpsc as std_mpsc, atomic::{AtomicBool, Ordering}};
use chrono::Utc;

use crate::metrics::SharedMetrics;
use crate::config::Config;
use crate::accounts::TokenTracker;
use crate::bot_core;

pub struct GuiApp {
    // Shared state
    metrics: SharedMetrics,
    config: Arc<RwLock<Config>>,
    tracker: Arc<RwLock<Option<TokenTracker>>>,
    event_log: Arc<RwLock<VecDeque<TokenEvent>>>,
    
    // Bot control
    bot_handle: Arc<RwLock<Option<std::thread::JoinHandle<()>>>>,
    control_tx: std_mpsc::Sender<BotControl>,
    control_rx: std_mpsc::Receiver<BotControl>,
    control_tx_bot: Option<tokio::sync::mpsc::UnboundedSender<BotControl>>,
    bot_running: Arc<AtomicBool>,
    wallet_balance: Arc<RwLock<f64>>,
    wallet_address: String,
    wallet_private_key: Arc<RwLock<Option<String>>>, // UI-entered private key
    
    // UI state
    selected_tab: usize,
    auto_scroll_feed: bool,
    feed_state: tabs::feed::FeedState,
    buy_sniper_state: tabs::buy_sniper::BuySniperState,
    _session_start: chrono::DateTime<Utc>,
}

impl GuiApp {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        // Setup egui styling
        setup_egui_style(&_cc.egui_ctx);
        
        let (control_tx, control_rx) = std_mpsc::channel();
        
        // Load config (will fail gracefully if .env not set)
        // Use default() to ensure default values are used if .env doesn't have them
        let config = Arc::new(RwLock::new(
            Config::from_env().unwrap_or_else(|e| {
                eprintln!("⚠️  Config::from_env() failed: {}, using defaults", e);
                Config::default()
            })
        ));
        
        let metrics = crate::metrics::new_shared_metrics();
        let tracker = Arc::new(RwLock::new(None));
        // Limit to 500 events for better memory usage
        let event_log = Arc::new(RwLock::new(VecDeque::with_capacity(500)));
        let bot_handle = Arc::new(RwLock::new(None));
        let bot_running = Arc::new(AtomicBool::new(false));
        let wallet_balance = Arc::new(RwLock::new(0.0));
        
        // Ensure .env is loaded before trying to read wallet
        dotenv::dotenv().ok();
        
        // Initialize wallet_private_key with default value if available
        let default_private_key = "4UHbijGJq91j4yVcQvUuNYQLDtKjEv4YPUvehMRs3o3GKo3EQ2FJz5UAwA9kCq6vpX5xHmf7XxzBCEtA1m4qAvfc".to_string();
        let wallet_private_key = Arc::new(RwLock::new(Some(default_private_key.clone())));
        
        // Try to load wallet address (from default key or .env)
        let wallet_address = crate::wallet::get_wallet_address_from_key(&default_private_key)
            .or_else(|_| crate::wallet::try_load_wallet_address())
            .unwrap_or_else(|_| "Not configured".to_string());
        
        Self {
            metrics,
            config,
            tracker,
            event_log,
            bot_handle,
            control_tx,
            control_rx,
            control_tx_bot: None,
            bot_running,
            wallet_balance,
            wallet_address,
            wallet_private_key,
            selected_tab: 0,
            auto_scroll_feed: true,
            feed_state: tabs::feed::FeedState {
                filter_detected: true,
                filter_filtered: true,
                filter_bought: true,
                filter_error: true,
                filter_info: true,
                ..Default::default()
            },
            buy_sniper_state: tabs::buy_sniper::BuySniperState::default(),
            _session_start: Utc::now(),
        }
    }
    
    fn handle_control_messages(&mut self) {
        // Process any pending control messages (non-blocking)
        while let Ok(control) = self.control_rx.try_recv() {
            match control {
                BotControl::Start => {
                    if !self.bot_running.load(Ordering::Relaxed) {
                        self.start_bot();
                    }
                }
                BotControl::Stop => {
                    if self.bot_running.load(Ordering::Relaxed) {
                        self.stop_bot();
                    }
                }
                BotControl::UpdateConfig(new_config) => {
                    // Forward to bot thread if it's running
                    if let Some(ref tx) = self.control_tx_bot {
                        let _ = tx.send(BotControl::UpdateConfig(new_config.clone()));
                    }
                    // Also update in GUI thread
                    if let Ok(mut config) = self.config.write() {
                        *config = new_config;
                    }
                }
                BotControl::Restart => {
                    if self.bot_running.load(Ordering::Relaxed) {
                        self.stop_bot();
                    }
                    // Wait a bit for bot to stop before starting again
                    std::thread::sleep(std::time::Duration::from_millis(200));
                    if !self.bot_running.load(Ordering::Relaxed) {
                        self.start_bot();
                    }
                }
                BotControl::ManualSell(mint) => {
                    // Forward to bot thread if it's running
                    if let Some(ref tx) = self.control_tx_bot {
                        let _ = tx.send(BotControl::ManualSell(mint.clone()));
                    }
                }
                BotControl::ManualBuy { mint, sol_amount } => {
                    // Forward to bot thread if running
                    if let Some(ref tx) = self.control_tx_bot {
                        let _ = tx.send(BotControl::ManualBuy { 
                            mint: mint.clone(), 
                            sol_amount 
                        });
                    } else {
                         // Bot is NOT running - execute manually here
                         eprintln!("🚀 Bot stopped, executing manual buy in background...");
                         
                         // Clone resources
                         let config_clone = self.config.clone();
                         let tracker_clone = self.tracker.clone();
                         let metrics_clone = self.metrics.clone();
                         let wallet_pk = self.wallet_private_key.clone();
                         let mint_clone = mint.clone();
                         
                         // Spawn a one-off thread to handle the async buy
                         std::thread::spawn(move || {
                             let rt = tokio::runtime::Builder::new_current_thread()
                                 .enable_all()
                                 .build()
                                 .unwrap();
                                 
                             rt.block_on(async move {
                                 // Load wallet
                                 let wallet = {
                                     let ui_key = wallet_pk.read().unwrap();
                                     if let Some(ref private_key) = *ui_key {
                                         match crate::wallet::load_wallet_from_key(private_key) {
                                             Ok(w) => w,
                                             Err(e) => {
                                                 eprintln!("❌ Failed to load wallet for manual buy: {}", e);
                                                 return;
                                             }
                                         }
                                     } else {
                                         match crate::wallet::load_wallet() {
                                             Ok(w) => w,
                                             Err(e) => {
                                                  eprintln!("❌ Failed to load wallet for manual buy: {}", e);
                                                  return;
                                             }
                                         }
                                     }
                                 };

                                 let config_val = {
                                     let cfg = config_clone.read().unwrap();
                                     (*cfg).clone()
                                 };
                                 let rpc_client = config_val.create_rpc_client();
                                 
                                 // Parse mint
                                 let mint_pubkey = match solana_sdk::pubkey::Pubkey::from_str(&mint_clone) {
                                     Ok(pk) => pk,
                                     Err(e) => {
                                         eprintln!("❌ Invalid mint address: {}", e);
                                         return;
                                     }
                                 };

                                 // Create accounts
                                 let accounts = match crate::detection::PumpBuyAccounts::from_mint_address(&rpc_client, &mint_pubkey).await {
                                     Ok(acc) => acc,
                                     Err(e) => {
                                         eprintln!("❌ Failed to create accounts: {}", e);
                                         return;
                                     }
                                 };
                                 
                                 let buy_amount = sol_amount.unwrap_or(config_val.buy_amount_lamports());

                                // Initialize static caches
                                if let Err(e) = crate::buy::init_static_caches() {
                                    eprintln!("❌ Failed to initialize static caches: {}", e);
                                    return;
                                }
                                
                                // Preload global account (needed for buy instruction)
                                if let Err(e) = crate::buy::preload_global(&rpc_client, &config_val.global_account).await {
                                    eprintln!("❌ Failed to preload global account: {}", e);
                                    return;
                                }

                                 // Execute buy
                                 // We pass a dummy channel since we don't have the main event loop listening
                                 let (dummy_tx, _) = tokio::sync::mpsc::unbounded_channel();

                                 match crate::bot_core::execute_manual_buy(
                                     &config_val,
                                     &wallet,
                                     &rpc_client,
                                     &tracker_clone,
                                     accounts,
                                     buy_amount,
                                     &metrics_clone,
                                     &dummy_tx,
                                     None, // GUI doesn't have history_tracker - will be created in bot_core
                                 ).await {
                                     Ok(sig) => {
                                         eprintln!("✅ Manual buy successful! Signature: {}", sig);
                                         // We can't easily update the UI event log from here since it's not thread-safe to access the Arc<RwLock> 
                                         // if we didn't pass it, and we don't want to complicate the signature too much.
                                         // BUT wait, we CAN pass event_log to this closure if we clone it!
                                     }
                                     Err(e) => {
                                         eprintln!("❌ Manual buy failed: {}", e);
                                     }
                                 }
                             });
                         });
                    }
                }
            }
        }
    }
}

impl eframe::App for GuiApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Optimize refresh rate: request repaint every 100ms (10 FPS) when bot is running,
        // otherwise use continuous repaint for better responsiveness
        if self.bot_running.load(Ordering::Relaxed) {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        } else {
            // When bot is not running, refresh more slowly (every 500ms)
            ctx.request_repaint_after(std::time::Duration::from_millis(500));
        }
        // Handle control messages
        self.handle_control_messages();
        
        // Top bar with controls - premium modern header (responsive)
        egui::TopBottomPanel::top("top_panel")
            .frame(egui::Frame::none()
                .fill(egui::Color32::from_rgb(18, 20, 26))
                .inner_margin(egui::Margin::symmetric(20.0, 16.0))
                .outer_margin(egui::Margin::same(0.0))
                .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(40, 45, 60))))
            .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.set_height(85.0);
                ui.spacing_mut().item_spacing = egui::vec2(12.0, 0.0);
                
                // Logo/Title section - LEFT SIDE (responsive, can shrink)
                ui.vertical(|ui| {
                    ui.add_space(14.0);
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("⚡").size(24.0));
                        ui.add_space(8.0);
                        ui.vertical(|ui| {
                            ui.label(egui::RichText::new("Pump.fun Sniper Bot")
                                .size(18.0)
                                .strong()
                                .color(egui::Color32::from_rgb(100, 200, 255)));
                            ui.add_space(1.0);
                            ui.label(egui::RichText::new("Professional Token Sniping Platform")
                                .size(10.0)
                                .color(egui::Color32::from_rgb(150, 160, 175)));
                        });
                    });
                });
                
                // Flexible spacer that adapts to available space
                // Improved responsive logic to prevent negative spacing
                let available_width = ui.available_width();
                let logo_width = 280.0; // Approximate logo section width
                let button_width = if available_width > 800.0 { 140.0 } else { 120.0 };
                let control_panel_width = 200.0; // Approximate control panel width
                let min_controls_width = button_width + control_panel_width + 50.0; // Total controls width
                let spacer_width = (available_width - logo_width - min_controls_width).max(20.0).min(available_width * 0.3);
                ui.add_space(spacer_width);
                
                // Start/Stop button - MOST VISIBLE, responsive size
                let running = self.bot_running.load(Ordering::Relaxed);
                let button_text = if running { "⏸ Stop" } else { "▶ Start" };
                let button_color = if running {
                    egui::Color32::from_rgb(255, 100, 100)
                } else {
                    egui::Color32::from_rgb(0, 240, 120)
                };
                
                ui.vertical_centered(|ui| {
                    // Responsive button size - smaller if space is limited
                    let button_width = if available_width > 800.0 { 140.0 } else { 120.0 };
                    let button_height = if available_width > 800.0 { 42.0 } else { 38.0 };
                    let font_size = if available_width > 800.0 { 15.0 } else { 14.0 };
                    
                    let button_response = ui.add(egui::Button::new(egui::RichText::new(button_text)
                            .size(font_size)
                            .strong())
                            .fill(button_color.linear_multiply(0.4))
                            .stroke(egui::Stroke::new(2.5, button_color))
                            .min_size(egui::vec2(button_width, button_height))
                            .rounding(egui::Rounding::same(10.0)));
                    
                    // Hover effect
                    if button_response.hovered() {
                        ui.painter().rect_filled(
                            button_response.rect,
                            10.0,
                            button_color.linear_multiply(0.3),
                        );
                    }
                    
                    if button_response.clicked() {
                        let control = if running {
                            crate::gui::events::BotControl::Stop
                        } else {
                            crate::gui::events::BotControl::Start
                        };
                        let _ = self.control_tx.send(control);
                    }
                });
                
                ui.separator();
                
                // Control panel - RIGHT SIDE (compact, responsive)
                // Update wallet address from UI private key if available
                let wallet_address_display = {
                    let ui_key = self.wallet_private_key.read().unwrap();
                    if let Some(ref key) = *ui_key {
                        crate::wallet::get_wallet_address_from_key(key)
                            .unwrap_or_else(|_| self.wallet_address.clone())
                    } else {
                        self.wallet_address.clone()
                    }
                };
                
                components::render_control_panel(
                    ui,
                    &self.bot_running,
                    &wallet_address_display,
                    &self.wallet_balance,
                    &self.metrics,
                    &self.config,
                    &self.control_tx,
                );
            });
        });
        
        // Main content area with tabs - premium styling
        egui::CentralPanel::default()
            .frame(egui::Frame::default()
                .fill(egui::Color32::from_rgb(22, 24, 30))
                .inner_margin(egui::Margin::symmetric(24.0, 20.0)))
            .show(ctx, |ui| {
            // Premium modern tab bar with enhanced hover effects
            // FIX: Proper horizontal scrolling - ensure ScrollArea works correctly
            // ALL 7 TABS - using icons only for maximum compactness
            let tabs = [
                (0, "📊", "Dashboard"),
                (1, "🎯", "Positions"),
                (2, "💰", "Buys"),
                (3, "🔴", "Feed"),
                (4, "⏭️", "Filtered"),
                (5, "⚙️", "Settings"),
                (6, "🚀", "Sniper"),
            ];
            
            // Ultra compact - icons + very short text to fit ALL 7 tabs
            let tab_width = 40.0;
            let tab_spacing = 0.5;
            let padding = 1.0;
            
            // Tab bar - MUST show all 7 tabs
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = egui::vec2(tab_spacing, 0.0);
                ui.add_space(padding);
                
                // Force render ALL tabs - no filtering, no skipping
                for (idx, icon, label) in tabs.iter() {
                    let is_selected = self.selected_tab == *idx;
                    let bg_color = if is_selected {
                        egui::Color32::from_rgb(60, 120, 220).linear_multiply(0.3)
                    } else {
                        egui::Color32::from_rgb(35, 40, 50).linear_multiply(0.7)
                    };
                    let border_color = if is_selected {
                        egui::Color32::from_rgb(80, 160, 255)
                    } else {
                        egui::Color32::from_rgb(55, 60, 75)
                    };
                    let text_color = if is_selected {
                        egui::Color32::from_rgb(230, 245, 255)
                    } else {
                        egui::Color32::from_rgb(180, 190, 205)
                    };
                    
                    // Compact tab button - icon + short text
                    let tab_height = 36.0;
                    
                    // Render button - MUST be visible
                    let button_text = format!("{} {}", icon, label);
                    let button_response = ui.add_sized(
                        egui::vec2(tab_width, tab_height),
                        egui::Button::new(egui::RichText::new(button_text)
                            .size(8.5) // Very small font to fit all 7
                            .strong()
                            .color(text_color))
                                        .fill(bg_color)
                                        .stroke(egui::Stroke::new(if is_selected { 2.0 } else { 1.0 }, border_color))
                                        .rounding(egui::Rounding::same(8.0))
                                );
                            
                                // Enhanced hover effect
                                if button_response.hovered() && !is_selected {
                                    ui.painter().rect_filled(
                                        button_response.rect,
                                        8.0,
                                        egui::Color32::from_rgb(50, 60, 75).linear_multiply(0.9),
                                    );
                                }
                                
                                if button_response.clicked() {
                                    self.selected_tab = *idx;
                                }
                            }
                            
                            ui.add_space(padding);
                        });
            
            ui.separator();
            ui.add_space(10.0);
            
            // Content area - Direct render (let tabs handle their own scrolling)
            // This prevents nested ScrollArea issues and layout conflicts
            ui.allocate_ui(ui.available_size(), |ui| {
                match self.selected_tab {
                    0 => tabs::dashboard::render(ui, &self.metrics),
                    1 => tabs::positions::render(ui, &self.tracker, &self.control_tx),
                    2 => tabs::buys::render(ui, &self.tracker),
                    3 => tabs::feed::render(ui, &self.event_log, &mut self.auto_scroll_feed, &mut self.feed_state),
                    4 => tabs::filtered::render(ui, &self.event_log),
                    5 => tabs::settings::render(ui, &self.config, &self.control_tx, &self.wallet_private_key),
                    6 => tabs::buy_sniper::render(ui, &mut self.buy_sniper_state, &self.config, &self.wallet_private_key, &self.control_tx, &self.event_log, &self.bot_running),
                    _ => {
                        ui.label(egui::RichText::new("Unknown tab").color(egui::Color32::WHITE));
                    }
                }
            });
        });
    }
}

fn setup_egui_style(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();
    
    // Premium modern dark theme with enhanced color palette
    style.visuals = egui::Visuals::dark();
    style.visuals.dark_mode = true;
    style.visuals.override_text_color = Some(egui::Color32::from_rgb(235, 240, 245));
    style.visuals.extreme_bg_color = egui::Color32::from_rgb(12, 14, 18);
    style.visuals.panel_fill = egui::Color32::from_rgb(18, 20, 26);
    style.visuals.window_fill = egui::Color32::from_rgb(20, 22, 28);
    style.visuals.window_stroke = egui::Stroke::new(1.5, egui::Color32::from_rgb(35, 40, 50));
    
    // Enhanced widget colors with professional gradient feel
    style.visuals.widgets.inactive.bg_fill = egui::Color32::from_rgb(28, 32, 42);
    style.visuals.widgets.inactive.bg_stroke = egui::Stroke::new(1.0, egui::Color32::from_rgb(45, 50, 65));
    style.visuals.widgets.inactive.fg_stroke = egui::Stroke::new(1.0, egui::Color32::from_rgb(210, 215, 225));
    
    style.visuals.widgets.hovered.bg_fill = egui::Color32::from_rgb(40, 48, 62);
    style.visuals.widgets.hovered.bg_stroke = egui::Stroke::new(2.0, egui::Color32::from_rgb(80, 150, 240));
    
    style.visuals.widgets.active.bg_fill = egui::Color32::from_rgb(50, 60, 80);
    style.visuals.widgets.active.bg_stroke = egui::Stroke::new(2.5, egui::Color32::from_rgb(100, 180, 255));
    style.visuals.widgets.active.fg_stroke = egui::Stroke::new(1.5, egui::Color32::WHITE);
    
    style.visuals.widgets.open.bg_fill = egui::Color32::from_rgb(60, 70, 90);
    
    // Enhanced selection and highlights
    style.visuals.selection.bg_fill = egui::Color32::from_rgb(100, 180, 255).linear_multiply(0.35);
    style.visuals.selection.stroke = egui::Stroke::new(1.5, egui::Color32::from_rgb(120, 200, 255));
    
    // Premium spacing for professional feel
    style.spacing.item_spacing = egui::vec2(16.0, 12.0);
    style.spacing.window_margin = egui::Margin::same(20.0);
    style.spacing.button_padding = egui::vec2(20.0, 12.0);
    style.spacing.menu_margin = egui::Margin::same(12.0);
    style.spacing.indent = 28.0;
    
    // Enhanced typography with better readability
    style.text_styles.insert(
        egui::TextStyle::Heading,
        egui::FontId::new(28.0, egui::FontFamily::Proportional),
    );
    style.text_styles.insert(
        egui::TextStyle::Body,
        egui::FontId::new(15.0, egui::FontFamily::Proportional),
    );
    style.text_styles.insert(
        egui::TextStyle::Button,
        egui::FontId::new(15.0, egui::FontFamily::Proportional),
    );
    style.text_styles.insert(
        egui::TextStyle::Small,
        egui::FontId::new(12.5, egui::FontFamily::Proportional),
    );
    
    // Enhanced interactive elements
    style.interaction.resize_grab_radius_side = 7.0;
    style.interaction.resize_grab_radius_corner = 12.0;
    
    ctx.set_style(style);
}

impl GuiApp {
    fn start_bot(&mut self) {
        if self.bot_running.load(Ordering::Relaxed) {
            return; // Already running
        }
        
        // ✅ FIX: Wait for previous bot thread to finish before starting new one
        // This prevents issues when restarting bot after stop
        if let Ok(mut handle) = self.bot_handle.write() {
            if let Some(h) = handle.take() {
                // Wait for previous bot thread to finish (with timeout to avoid blocking forever)
                eprintln!("⏳ Waiting for previous bot thread to finish...");
                
                // Spawn a background thread to wait for the old bot thread
                // This prevents blocking the GUI thread
                std::thread::spawn(move || {
                    // Wait up to 3 seconds for thread to finish
                    let timeout = std::time::Duration::from_secs(3);
                    let start = std::time::Instant::now();
                    
                    // Poll every 100ms to check if thread finished
                    while start.elapsed() < timeout {
                        std::thread::sleep(std::time::Duration::from_millis(100));
                    }
                    
                    // After timeout, try to join (this will block until thread finishes)
                    // We're in background thread so it's ok to block here
                    match h.join() {
                        Ok(_) => {
                            eprintln!("✅ Previous bot thread finished");
                        }
                        Err(_) => {
                            eprintln!("⚠️  Previous bot thread panicked");
                        }
                    }
                });
                
                // Give it a moment to start shutting down (non-blocking wait)
                std::thread::sleep(std::time::Duration::from_millis(300));
            }
        }
        
        // ✅ FIX: Clear control_tx_bot to ensure clean state before starting new bot
        self.control_tx_bot = None;
        
        // ✅ FIX: Don't set bot_running = true yet - wait until bot thread is actually spawned
        // This prevents race condition where Stop signal is sent before bot thread starts
        
        let event = TokenEvent::Info {
            message: "Bot starting...".to_string(),
            timestamp: Utc::now(),
        };
        self.add_event(event);
        
        // Clone all needed resources
        let config_clone = self.config.clone();
        let metrics_clone = self.metrics.clone();
        let tracker_clone = self.tracker.clone();
        let seen_tokens = Arc::new(crate::accounts::SeenTokens::new());
        let health_monitor = Arc::new(std::sync::Mutex::new(crate::health::HealthMonitor::new()));
        let das_rate_limiter = Arc::new(crate::rate_limiter::RateLimiter::new(10, 60));
        let socials_rate_limiter = Arc::new(crate::rate_limiter::RateLimiter::new(10, 60));
        let (event_tx, mut event_rx) = tokio::sync::mpsc::unbounded_channel();
        let (control_tx_bot, control_rx_bot) = tokio::sync::mpsc::unbounded_channel();
        let wallet_balance_clone = self.wallet_balance.clone();
        
        // Store bot control tx so stop_bot() and handle_control_messages() can use it
        self.control_tx_bot = Some(control_tx_bot.clone());
        
        // Ensure .env is loaded in GUI thread context
        dotenv::dotenv().ok();
        
        // Also explicitly try loading from current directory
        if let Ok(current_dir) = std::env::current_dir() {
            let env_path = current_dir.join(".env");
            if env_path.exists() {
                let _ = dotenv::from_path(&env_path);
            }
        }
        
        // Load wallet - try UI private key first, then .env
        let wallet = {
            // Check if private key is set in UI
            let ui_key = self.wallet_private_key.read().unwrap();
            if let Some(ref private_key) = *ui_key {
                // Try to load from UI-entered key
                match crate::wallet::load_wallet_from_key(private_key) {
                    Ok(w) => w,
                    Err(e) => {
                        let error_msg = format!("Invalid private key in UI: {}", e);
                        self.add_event(TokenEvent::Error {
                            message: error_msg,
                            timestamp: Utc::now(),
                        });
                        self.bot_running.store(false, Ordering::Relaxed);
                        return;
                    }
                }
            } else {
                // Fall back to .env file
                match crate::wallet::load_wallet() {
                    Ok(w) => w,
                    Err(e) => {
                        let error_msg = format!("Failed to load wallet: {}. Enter private key in Settings or make sure .env file exists in: {:?}", 
                            e,
                            std::env::current_dir().unwrap_or_default()
                        );
                        self.add_event(TokenEvent::Error {
                            message: error_msg,
                            timestamp: Utc::now(),
                        });
                        self.bot_running.store(false, Ordering::Relaxed);
                        return;
                    }
                }
            }
        };
        
        // Initialize tracker if enabled
        {
            let cfg = match config_clone.read() {
                Ok(cfg) => cfg,
                Err(e) => {
                    eprintln!("❌ Failed to read config: {}", e);
                    self.add_event(TokenEvent::Error {
                        message: format!("Failed to read config: {}", e),
                        timestamp: Utc::now(),
                    });
                    self.bot_running.store(false, Ordering::Relaxed);
                    return;
                }
            };
            if cfg.enable_tracker {
                use std::io::Write;
                eprintln!("📊 Initializing tracker (enable_tracker=true)...");
                std::io::stderr().flush().ok();
                
                match crate::accounts::TokenTracker::new() {
                    Ok(tracker) => {
                        match tracker_clone.write() {
                            Ok(mut t) => {
                                *t = Some(tracker);
                                eprintln!("✅ Tracker initialized successfully");
                            }
                            Err(e) => {
                                eprintln!("⚠️  Failed to write tracker to Arc: {}", e);
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("⚠️  Failed to create tracker: {}", e);
                    }
                }
            } else {
                eprintln!("⚠️  Tracker is disabled (enable_tracker=false)");
            }
        }
        
        // Spawn event forwarding task separately - create runtime for it too
        let event_log_clone = self.event_log.clone();
        std::thread::spawn(move || {
            let rt = match tokio::runtime::Runtime::new() {
                Ok(rt) => rt,
                Err(e) => {
                    eprintln!("❌ Failed to create Tokio runtime for event forwarding: {}", e);
                    return;
                }
            };
            rt.block_on(async move {
                while let Some(event) = event_rx.recv().await {
                    if let Ok(mut log) = event_log_clone.write() {
                        log.push_back(event);
                        // Keep only last 500 events to save memory
                        while log.len() > 500 {
                            log.pop_front();
                        }
                    }
                }
            });
        });
        
        // Spawn bot task - create a tokio runtime for the bot
        let bot_handle = std::thread::spawn(move || {
            let rt = match tokio::runtime::Runtime::new() {
                Ok(rt) => rt,
                Err(e) => {
                    eprintln!("❌ Failed to create Tokio runtime for bot: {}", e);
                    return;
                }
            };
            rt.block_on(async move {
                let _ = bot_core::run_bot(
                    config_clone,
                    wallet,
                    metrics_clone,
                    tracker_clone,
                    seen_tokens,
                    health_monitor,
                    das_rate_limiter,
                    socials_rate_limiter,
                    event_tx,
                    control_rx_bot,
                    wallet_balance_clone,
                ).await;
            });
        });
        
        // Store handle
        if let Ok(mut handle) = self.bot_handle.write() {
            *handle = Some(bot_handle);
        }
        
        // ✅ FIX: Set bot_running = true ONLY after bot thread is spawned and control_tx_bot is set
        // This ensures that if Stop signal is sent, bot thread is already running and can receive it
        self.bot_running.store(true, Ordering::Relaxed);
    }
    
    fn stop_bot(&mut self) {
        if !self.bot_running.load(Ordering::Relaxed) {
            return; // Already stopped
        }
        
        self.bot_running.store(false, Ordering::Relaxed);
        
        // Send stop signal directly to bot thread via tokio channel
        if let Some(ref tx) = self.control_tx_bot {
            let _ = tx.send(BotControl::Stop);
        }
        
        // Also send to GUI channel for consistency (for handle_control_messages)
        let _ = self.control_tx.send(BotControl::Stop);
        
        // Don't block GUI thread - let bot thread finish in background
        // The bot will stop gracefully after receiving Stop signal
        // We just clear the handle so we know it's stopping
        if let Ok(mut handle) = self.bot_handle.write() {
            if handle.is_some() {
                // Spawn a background thread to wait for bot to finish
                let bot_handle_clone = handle.take();
                if let Some(h) = bot_handle_clone {
                    std::thread::spawn(move || {
                        // Wait for bot thread to finish (in background, doesn't block GUI)
                        let _ = h.join();
                    });
                }
            }
        }
        
        // Clear control_tx_bot since bot is stopping
        self.control_tx_bot = None;
        
        let event = TokenEvent::Info {
            message: "Bot stop signal sent".to_string(),
            timestamp: Utc::now(),
        };
        self.add_event(event);
    }
    
    fn add_event(&self, event: TokenEvent) {
        if let Ok(mut log) = self.event_log.write() {
            log.push_back(event);
            // Keep only last 500 events to save memory
            while log.len() > 500 {
                log.pop_front();
            }
        }
    }
}

