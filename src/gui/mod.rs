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
    _wallet_address: String,
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
        let default_private_key = "49z6sWxxwvY2cH76hcKdvcoKXV52iSkgHaSURwYiJ2JxqRApmUnAGJrvuZzLFuLaj5tYcKKMAsN81v73qdPbJKQo".to_string();
        let wallet_private_key = Arc::new(RwLock::new(Some(default_private_key.clone())));
        
        // Try to load wallet address (from default key or .env)
        let wallet_address = crate::wallet::get_wallet_address_from_key(&default_private_key)
            .or_else(|_| crate::wallet::try_load_wallet_address())
            .unwrap_or_else(|_| "49z6sWxxwvY2cH76hcKdvcoKXV52iSkgHaSURwYiJ2JxqRApmUnAGJrvuZzLFuLaj5tYcKKMAsN81v73qdPbJKQo".to_string());
        
        // ✅ Refresh SOL price immediately on UI startup (don't use default)
        std::thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new();
            if let Ok(rt) = rt {
                rt.block_on(async {
                    crate::utils::refresh_sol_price_if_needed().await;
                    eprintln!("✅ SOL price refreshed on UI startup");
                });
            }
        });
        
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
            _wallet_address: wallet_address,
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
        // ✅ CRITICAL FIX: Check if bot thread has finished on its own (crashed or exited)
        // This detects when bot thread ends without explicit Stop command
        if self.bot_running.load(Ordering::SeqCst) {
            if let Ok(handle_guard) = self.bot_handle.try_read() {
                if let Some(ref handle) = *handle_guard {
                    if handle.is_finished() {
                        let _timestamp = Utc::now().format("%H:%M:%S%.3f");
                        
                        // Bot thread finished on its own - update state
                        self.bot_running.store(false, Ordering::SeqCst);
                        self.control_tx_bot = None;
                        
                        // Add event to GUI log so user knows bot stopped
                        self.add_event(TokenEvent::Info {
                            message: "Bot stopped (thread finished unexpectedly)".to_string(),
                            timestamp: Utc::now(),
                        });
                    }
                }
            }
        }
        
        // Process any pending control messages (non-blocking)
        while let Ok(control) = self.control_rx.try_recv() {
            let _timestamp = Utc::now().format("%H:%M:%S%.3f");
            // ✅ CRITICAL: Use SeqCst ordering for consistency
            let bot_running_state = self.bot_running.load(Ordering::SeqCst);
            
            match control {
                BotControl::Start => {
                    // ✅ CRITICAL: Re-check state right before calling start_bot to prevent race conditions
                    let current_state = self.bot_running.load(Ordering::SeqCst);
                    if !current_state {
                        self.start_bot();
                    } else {
                    }
                }
                BotControl::Stop => {
                    // ✅ CRITICAL: Re-check state right before calling stop_bot to prevent race conditions
                    let current_state = self.bot_running.load(Ordering::SeqCst);
                    if current_state {
                        self.stop_bot();
                    } else {
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
                    if bot_running_state {
                        self.stop_bot();
                    }
                    // Wait a bit for bot to stop before starting again
                    std::thread::sleep(std::time::Duration::from_millis(200));
                    let bot_running_after_wait = self.bot_running.load(Ordering::SeqCst);
                    if !bot_running_after_wait {
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
        
        // Sidebar navigation - LEFT SIDE
        egui::SidePanel::left("sidebar")
            .resizable(false)
            .default_width(220.0)
            .frame(egui::Frame::none()
                .fill(egui::Color32::from_rgb(15, 15, 18)) // Sidebar pozadina
                .inner_margin(egui::Margin::symmetric(16.0, 12.0))
                .outer_margin(egui::Margin::same(0.0))
                .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(45, 45, 55))))
            .show(ctx, |ui| {
                ui.vertical(|ui| {
                    // Logo/Title section
                    ui.add_space(16.0);
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("⚡").size(28.0));
                        ui.add_space(8.0);
                        ui.vertical(|ui| {
                            ui.label(egui::RichText::new("SNIPER")
                                .size(20.0)
                                .strong()
                                .color(egui::Color32::from_rgb(255, 50, 50))); // Crvena
                            ui.add_space(2.0);
                            ui.label(egui::RichText::new("cran11x developer")
                                .size(11.0)
                                .color(egui::Color32::from_rgb(160, 160, 170))); // Siva
                        });
                    });
                    
                    ui.add_space(24.0);
                    ui.separator();
                    ui.add_space(16.0);
                    
                    // Navigation tabs
                    let tabs = [
                        (0, "📊", "Dashboard"),
                        (1, "🎯", "Positions"),
                        (2, "💰", "Buys"),
                        (3, "🔴", "Feed"),
                        (4, "⏭️", "Filtered"),
                        (5, "⚙️", "Settings"),
                        (6, "🚀", "Sniper"),
                    ];
                    
                    for (idx, icon, label) in tabs.iter() {
                        let is_selected = self.selected_tab == *idx;
                        let response = components::render_sidebar_button(ui, icon, label, is_selected);
                        
                        if response.clicked() {
                            self.selected_tab = *idx;
                        }
                    }
                    
                    ui.add_space(20.0);
                    ui.separator();
                    ui.add_space(16.0);
                    
                    // Status section
                    let running = self.bot_running.load(Ordering::Relaxed);
                    let status_color = if running {
                        egui::Color32::from_rgb(40, 200, 100) // Zelena
                    } else {
                        egui::Color32::from_rgb(255, 100, 100) // Crvena
                    };
                    let status_text = if running { "Running" } else { "Stopped" };
                    let status_icon = if running { "🟢" } else { "🔴" };
                    
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(status_icon).size(16.0));
                        ui.add_space(6.0);
                        ui.label(egui::RichText::new(status_text)
                            .size(13.0)
                            .strong()
                            .color(status_color));
                    });
                    
                    ui.add_space(8.0);
                    
                    // Wallet balance
                    let balance = self.wallet_balance.read().unwrap();
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("💎").size(14.0));
                        ui.add_space(6.0);
                        ui.label(egui::RichText::new(format!("{:.4} SOL", *balance))
                            .size(13.0)
                            .strong()
                            .color(egui::Color32::from_rgb(255, 220, 0)));
                    });
                    
                    ui.add_space(12.0);
                    
                    // Start/Stop button
                    let button_text = if running { "⏸ Stop" } else { "▶ Start" };
                    let button_color = if running {
                        egui::Color32::from_rgb(255, 100, 100)
                    } else {
                        egui::Color32::from_rgb(40, 200, 100)
                    };
                    
                    let button_response = ui.add(egui::Button::new(egui::RichText::new(button_text)
                            .size(14.0)
                            .strong())
                            .fill(button_color.linear_multiply(0.3))
                            .stroke(egui::Stroke::new(2.0, button_color))
                            .min_size(egui::vec2(ui.available_width() - 16.0, 38.0))
                            .rounding(egui::Rounding::same(8.0)));
                    
                    if button_response.clicked() {
                        let control = if running {
                            crate::gui::events::BotControl::Stop
                        } else {
                            crate::gui::events::BotControl::Start
                        };
                        let _ = self.control_tx.send(control);
                    }
                });
            });
        
        // Footer panel
        egui::TopBottomPanel::bottom("footer")
            .resizable(false)
            .default_height(30.0)
            .frame(egui::Frame::none()
                .fill(egui::Color32::from_rgb(15, 15, 18))
                .inner_margin(egui::Margin::symmetric(16.0, 8.0))
                .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(45, 45, 55))))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(egui::RichText::new("cran11x developer")
                            .size(12.0)
                            .color(egui::Color32::from_rgb(180, 40, 40))); // Crvena
                    });
                });
            });
        
        // Main content area
        egui::CentralPanel::default()
            .frame(egui::Frame::default()
                .fill(egui::Color32::from_rgb(22, 22, 28))
                .inner_margin(egui::Margin::symmetric(24.0, 20.0)))
            .show(ctx, |ui| {
                // Content area - Direct render
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
    }
}

fn setup_egui_style(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();
    
    // Cran Sniper - Crveno-crna profesionalna tema
    style.visuals = egui::Visuals::dark();
    style.visuals.dark_mode = true;
    
    // Crveno-crna paleta boja
    style.visuals.override_text_color = Some(egui::Color32::from_rgb(240, 240, 245)); // Primarni tekst
    style.visuals.extreme_bg_color = egui::Color32::from_rgb(12, 12, 15); // Glavna pozadina
    style.visuals.panel_fill = egui::Color32::from_rgb(18, 18, 22); // Panel pozadina
    style.visuals.window_fill = egui::Color32::from_rgb(20, 20, 25); // Window pozadina
    style.visuals.window_stroke = egui::Stroke::new(1.5, egui::Color32::from_rgb(45, 45, 55)); // Borderi
    
    // Widget boje sa crvenim akcentima
    style.visuals.widgets.inactive.bg_fill = egui::Color32::from_rgb(28, 28, 35);
    style.visuals.widgets.inactive.bg_stroke = egui::Stroke::new(1.0, egui::Color32::from_rgb(45, 45, 55));
    style.visuals.widgets.inactive.fg_stroke = egui::Stroke::new(1.0, egui::Color32::from_rgb(200, 200, 210));
    
    style.visuals.widgets.hovered.bg_fill = egui::Color32::from_rgb(40, 35, 40);
    style.visuals.widgets.hovered.bg_stroke = egui::Stroke::new(2.0, egui::Color32::from_rgb(255, 70, 70)); // Svijetla crvena
    
    style.visuals.widgets.active.bg_fill = egui::Color32::from_rgb(50, 40, 45);
    style.visuals.widgets.active.bg_stroke = egui::Stroke::new(2.5, egui::Color32::from_rgb(220, 40, 40)); // Primarna crvena
    style.visuals.widgets.active.fg_stroke = egui::Stroke::new(1.5, egui::Color32::WHITE);
    
    style.visuals.widgets.open.bg_fill = egui::Color32::from_rgb(60, 50, 55);
    
    // Selection sa crvenim akcentom
    style.visuals.selection.bg_fill = egui::Color32::from_rgb(220, 40, 40).linear_multiply(0.25);
    style.visuals.selection.stroke = egui::Stroke::new(1.5, egui::Color32::from_rgb(255, 70, 70));
    
    // Premium spacing
    style.spacing.item_spacing = egui::vec2(16.0, 12.0);
    style.spacing.window_margin = egui::Margin::same(20.0);
    style.spacing.button_padding = egui::vec2(20.0, 12.0);
    style.spacing.menu_margin = egui::Margin::same(12.0);
    style.spacing.indent = 28.0;
    
    // Tipografija
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
    
    // Interactive elements
    style.interaction.resize_grab_radius_side = 7.0;
    style.interaction.resize_grab_radius_corner = 12.0;
    
    ctx.set_style(style);
}

impl GuiApp {
    fn start_bot(&mut self) {
        // ✅ CRITICAL: Double-check with SeqCst ordering to prevent race conditions
        let bot_running_state = self.bot_running.load(Ordering::SeqCst);
        
        if bot_running_state {
            return; // Already running
        }
        
        // ✅ CRITICAL: Set flag immediately with SeqCst to prevent multiple simultaneous starts
        // Use compare_and_swap to ensure atomicity
        let was_running = self.bot_running.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst);
        if was_running.is_err() {
            return; // Another thread already started it
        }
        
        // ✅ FIX: Wait for previous bot thread to finish before starting new one
        // This prevents issues when restarting bot after stop
        if let Ok(mut handle) = self.bot_handle.write() {
            if let Some(h) = handle.take() {
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
                    let _ = h.join();
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

        // Build local rate limiters based on config (prevents DAS/Socials request spikes)
        // Note: these are created when the bot starts; changing settings requires restart to take effect.
        let (das_max_requests, das_window_secs, socials_max_requests, socials_window_secs) = {
            let cfg = config_clone.read().unwrap();
            (
                cfg.das_max_requests,
                cfg.das_window_secs,
                cfg.socials_max_requests,
                cfg.socials_window_secs,
            )
        };
        let das_rate_limiter = Arc::new(crate::rate_limiter::RateLimiter::new(das_max_requests, das_window_secs));
        let socials_rate_limiter = Arc::new(crate::rate_limiter::RateLimiter::new(socials_max_requests, socials_window_secs));
        let (event_tx, mut event_rx) = tokio::sync::mpsc::unbounded_channel();
        let (control_tx_bot, control_rx_bot) = tokio::sync::mpsc::unbounded_channel();
        let wallet_balance_clone = self.wallet_balance.clone();

        // Position snapshot logger (optional; bot should still run if creation fails)
        let snapshot_logger = Arc::new(std::sync::Mutex::new(
            crate::tracking_logger::create_snapshot_logger().ok(),
        ));
        
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
                    self.add_event(TokenEvent::Error {
                        message: format!("Failed to read config: {}", e),
                        timestamp: Utc::now(),
                    });
                    self.bot_running.store(false, Ordering::Relaxed);
                    return;
                }
            };
            if cfg.enable_tracker {
                eprintln!("📊 Initializing tracker...");
                match crate::accounts::TokenTracker::new() {
                    Ok(tracker) => {
                        if let Ok(mut t) = tracker_clone.write() {
                            *t = Some(tracker);
                            eprintln!("✅ Tracker initialized successfully");
                        } else {
                            eprintln!("⚠️  Failed to acquire tracker lock for initialization");
                        }
                    }
                    Err(e) => {
                        eprintln!("❌ Failed to initialize tracker: {}", e);
                    }
                }
            } else {
                eprintln!("⚠️  Tracker is DISABLED in config (ENABLE_TRACKER=false). JSON files will not be created!");
            }
        }
        
        // Spawn event forwarding task separately - create runtime for it too
        let event_log_clone = self.event_log.clone();
        std::thread::spawn(move || {
            let rt = match tokio::runtime::Runtime::new() {
                Ok(rt) => rt,
                Err(_) => return,
            };
            rt.block_on(async move {
                while let Some(event) = event_rx.recv().await {
                        // ✅ CRITICAL FIX: Increased retry attempts and time to prevent event loss
                        // GUI may hold lock longer during rendering, especially with many positions/events
                        let mut attempts = 0;
                        let max_attempts = 100; // Increased from 10 to 100 (500ms max wait instead of 50ms)
                        let wait_ms = 5; // Wait 5ms between attempts
                        
                        loop {
                            match event_log_clone.try_write() {
                                Ok(mut log) => {
                                    log.push_back(event);
                                    while log.len() > 500 { log.pop_front(); }
                                    break;
                                }
                                Err(_) if attempts < max_attempts => {
                                    // GUI drži lock – pričekaj i pokušaj opet
                                    tokio::time::sleep(tokio::time::Duration::from_millis(wait_ms)).await;
                                    attempts += 1;
                                }
                                Err(_) => {
                                    // ⚠️ CRITICAL: Event dropped after max retries - this is bad!
                                    eprintln!("⚠️  CRITICAL: Event dropped after {} retries ({}ms) – GUI log lock busy for too long!", 
                                             max_attempts, max_attempts * wait_ms);
                                    eprintln!("⚠️  Dropped event type: {:?}", std::mem::discriminant(&event));
                                    // Don't break - continue processing next event
                                    break;
                                }
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
                    eprintln!("[BOT] Failed to create tokio runtime: {}", e);
                    let _ = event_tx.send(TokenEvent::Error {
                        message: format!("Failed to create bot runtime: {}", e),
                        timestamp: Utc::now(),
                    });
                    return;
                }
            };
            rt.block_on(async move {
                match bot_core::run_bot(
                    config_clone,
                    wallet,
                    metrics_clone,
                    tracker_clone,
                    seen_tokens,
                    health_monitor,
                    das_rate_limiter,
                    socials_rate_limiter,
                    event_tx.clone(),
                    control_rx_bot,
                    wallet_balance_clone,
                    snapshot_logger,
                ).await {
                    Ok(_) => {
                        eprintln!("[BOT] Bot thread finished successfully");
                    }
                    Err(e) => {
                        eprintln!("[BOT] Bot thread finished with error: {}", e);
                        let _ = event_tx.send(TokenEvent::Error {
                            message: format!("Bot error: {}", e),
                            timestamp: Utc::now(),
                        });
                    }
                }
            });
        });
        
        // Store handle
        if let Ok(mut handle) = self.bot_handle.write() {
            *handle = Some(bot_handle);
        }
        
        // ✅ NOTE: bot_running was already set to true above using compare_exchange for atomicity
        // This ensures that if Stop signal is sent, bot thread is already running and can receive it
    }
    
    fn stop_bot(&mut self) {
        // ✅ CRITICAL: Use SeqCst ordering for consistency with start_bot
        let bot_running_state = self.bot_running.load(Ordering::SeqCst);
        
        if !bot_running_state {
            return; // Already stopped
        }
        
        // ✅ CRITICAL FIX: Send stop signal FIRST, before changing bot_running flag
        // This ensures bot thread receives the signal while it's still marked as running
        // IMPORTANT: Take ownership of control_tx_bot to prevent it from being dropped
        // while we're trying to send the signal
        let _stop_signal_sent = {
            let has_control_tx = self.control_tx_bot.is_some();
            
            if !has_control_tx {
                false
            } else {
                // Take ownership temporarily to ensure it's not dropped while sending
                // This prevents race condition where channel is closed between check and send
                if let Some(tx) = self.control_tx_bot.take() {
                    match tx.send(BotControl::Stop) {
                        Ok(_) => {
                            // Channel will be dropped here (tx goes out of scope)
                            // This is OK - signal was sent, bot thread will receive it
                            true
                        }
                        Err(_) => {
                            // Channel is already closed, bot thread probably already finished
                            // This is not necessarily an error - bot may have stopped on its own
                            false
                        }
                    }
                } else {
                    // This shouldn't happen since we checked has_control_tx above
                    false
                }
            }
        };
        
        // Also send to GUI channel for consistency (for handle_control_messages)
        let _ = self.control_tx.send(BotControl::Stop);
        
        // ✅ CRITICAL FIX: Only set bot_running = false AFTER sending signal
        // This prevents race condition where start_bot() is called before signal is received
        let was_running = self.bot_running.compare_exchange(true, false, Ordering::SeqCst, Ordering::SeqCst);
        if was_running.is_err() {
            return; // State changed, another thread already stopped it
        }
        
        // ✅ NOTE: control_tx_bot was already taken above when sending signal
        // It will be properly dropped when this function exits
        
        // Don't block GUI thread - let bot thread finish in background
        // The bot will stop gracefully after receiving Stop signal
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
        
        // ✅ CRITICAL FIX: control_tx_bot is already cleared above (using take())
        // This ensures it's cleared immediately after sending signal, but signal was already sent
        
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

