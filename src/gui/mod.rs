// gui/mod.rs - Main GUI application

mod tabs;
mod components;
mod events;

pub use events::{TokenEvent, BotControl};

use eframe::egui;
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
    session_start: chrono::DateTime<Utc>,
}

impl GuiApp {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        // Setup egui styling
        setup_egui_style(&_cc.egui_ctx);
        
        let (control_tx, control_rx) = std_mpsc::channel();
        
        // Load config (will fail gracefully if .env not set)
        let config = Arc::new(RwLock::new(
            Config::from_env().unwrap_or_else(|_| Config::default())
        ));
        
        let metrics = crate::metrics::new_shared_metrics();
        let tracker = Arc::new(RwLock::new(None));
        // Limit to 500 events for better memory usage
        let event_log = Arc::new(RwLock::new(VecDeque::with_capacity(500)));
        let bot_handle = Arc::new(RwLock::new(None));
        let bot_running = Arc::new(AtomicBool::new(false));
        let wallet_balance = Arc::new(RwLock::new(0.0));
        let wallet_private_key = Arc::new(RwLock::new(None));
        
        // Ensure .env is loaded before trying to read wallet
        dotenv::dotenv().ok();
        
        // Try to load wallet address
        let wallet_address = crate::wallet::try_load_wallet_address()
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
            session_start: Utc::now(),
        }
    }
    
    fn handle_control_messages(&mut self) {
        // Process any pending control messages (non-blocking)
        while let Ok(control) = self.control_rx.try_recv() {
            // Forward to bot thread if it's running
            if let Some(ref tx) = self.control_tx_bot {
                let _ = tx.send(control.clone());
            }
            
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
                    if let Ok(mut config) = self.config.write() {
                        *config = new_config;
                    }
                }
                BotControl::Restart => {
                    if self.bot_running.load(Ordering::Relaxed) {
                        self.stop_bot();
                    }
                    if !self.bot_running.load(Ordering::Relaxed) {
                        self.start_bot();
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
        
        // Top bar with controls - modern header
        egui::TopBottomPanel::top("top_panel")
            .frame(egui::Frame::none()
                .fill(egui::Color32::from_rgb(22, 22, 28))
                .inner_margin(egui::Margin::same(16.0))
                .outer_margin(egui::Margin::same(0.0)))
            .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.set_height(50.0);
                
                // Logo/Title section
                ui.vertical(|ui| {
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("⚡").size(24.0));
                        ui.label(egui::RichText::new("Pump.fun Sniper Bot")
                            .size(20.0)
                            .strong()
                            .color(egui::Color32::from_rgb(100, 200, 255)));
                    });
                    ui.label(egui::RichText::new("Professional Token Sniping Platform")
                        .size(11.0)
                        .color(egui::Color32::from_rgb(150, 150, 160)));
                });
                
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
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
        });
        
        // Main content area with tabs
        egui::CentralPanel::default()
            .frame(egui::Frame::none()
                .fill(egui::Color32::from_rgb(24, 24, 30))
                .inner_margin(egui::Margin::same(16.0)))
            .show(ctx, |ui| {
            // Modern tab bar
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = egui::vec2(4.0, 0.0);
                
                let tabs = [
                    (0, "📊", "Dashboard"),
                    (1, "💰", "Recent Buys"),
                    (2, "🔴", "Live Feed"),
                    (3, "⏭️", "Filtered"),
                    (4, "⚙️", "Settings"),
                ];
                
                for (idx, icon, label) in tabs.iter() {
                    let is_selected = self.selected_tab == *idx;
                    let bg_color = if is_selected {
                        egui::Color32::from_rgb(100, 150, 255).linear_multiply(0.3)
                    } else {
                        egui::Color32::TRANSPARENT
                    };
                    let border_color = if is_selected {
                        egui::Color32::from_rgb(100, 150, 255)
                    } else {
                        egui::Color32::TRANSPARENT
                    };
                    
                    if ui.add(egui::Button::new(egui::RichText::new(format!("{} {}", icon, label))
                            .size(14.0)
                            .color(if is_selected {
                                egui::Color32::from_rgb(200, 220, 255)
                            } else {
                                egui::Color32::from_rgb(160, 160, 170)
                            }))
                            .fill(bg_color)
                            .stroke(egui::Stroke::new(if is_selected { 2.0 } else { 0.0 }, border_color))
                            .min_size(egui::vec2(130.0, 36.0)))
                            .clicked() {
                        self.selected_tab = *idx;
                    }
                }
            });
            
            ui.add_space(12.0);
            
            egui::ScrollArea::vertical().show(ui, |ui| {
                match self.selected_tab {
                    0 => tabs::dashboard::render(ui, &self.metrics),
                    1 => tabs::buys::render(ui, &self.tracker),
                    2 => tabs::feed::render(ui, &self.event_log, &mut self.auto_scroll_feed, &mut self.feed_state),
                    3 => tabs::filtered::render(ui, &self.event_log),
                    4 => tabs::settings::render(ui, &self.config, &self.control_tx, &self.wallet_private_key),
                    _ => {}
                }
            });
        });
    }
}

fn setup_egui_style(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();
    
    // Modern dark theme with vibrant accents
    style.visuals = egui::Visuals::dark();
    style.visuals.dark_mode = true;
    style.visuals.override_text_color = Some(egui::Color32::from_rgb(230, 230, 235));
    style.visuals.extreme_bg_color = egui::Color32::from_rgb(18, 18, 22);
    style.visuals.panel_fill = egui::Color32::from_rgb(22, 22, 28);
    style.visuals.window_fill = egui::Color32::from_rgb(24, 24, 30);
    style.visuals.window_stroke = egui::Stroke::new(1.0, egui::Color32::from_rgb(40, 40, 50));
    
    // Widget colors with gradient feel
    style.visuals.widgets.inactive.bg_fill = egui::Color32::from_rgb(32, 32, 40);
    style.visuals.widgets.inactive.bg_stroke = egui::Stroke::new(1.0, egui::Color32::from_rgb(50, 50, 60));
    style.visuals.widgets.inactive.fg_stroke = egui::Stroke::new(1.0, egui::Color32::from_rgb(200, 200, 210));
    
    style.visuals.widgets.hovered.bg_fill = egui::Color32::from_rgb(45, 45, 58);
    style.visuals.widgets.hovered.bg_stroke = egui::Stroke::new(1.5, egui::Color32::from_rgb(70, 130, 220));
    
    style.visuals.widgets.active.bg_fill = egui::Color32::from_rgb(55, 55, 72);
    style.visuals.widgets.active.bg_stroke = egui::Stroke::new(2.0, egui::Color32::from_rgb(90, 150, 255));
    style.visuals.widgets.active.fg_stroke = egui::Stroke::new(1.0, egui::Color32::WHITE);
    
    style.visuals.widgets.open.bg_fill = egui::Color32::from_rgb(65, 65, 85);
    
    // Selection and highlights
    style.visuals.selection.bg_fill = egui::Color32::from_rgb(100, 150, 255).linear_multiply(0.4);
    style.visuals.selection.stroke = egui::Stroke::new(1.0, egui::Color32::from_rgb(100, 150, 255));
    
    // Better spacing for modern feel
    style.spacing.item_spacing = egui::vec2(12.0, 8.0);
    style.spacing.window_margin = egui::Margin::same(12.0);
    style.spacing.button_padding = egui::vec2(16.0, 8.0);
    style.spacing.menu_margin = egui::Margin::same(8.0);
    style.spacing.indent = 20.0;
    
    // Modern typography
    style.text_styles.insert(
        egui::TextStyle::Heading,
        egui::FontId::new(26.0, egui::FontFamily::Proportional),
    );
    style.text_styles.insert(
        egui::TextStyle::Body,
        egui::FontId::new(14.5, egui::FontFamily::Proportional),
    );
    style.text_styles.insert(
        egui::TextStyle::Button,
        egui::FontId::new(14.5, egui::FontFamily::Proportional),
    );
    style.text_styles.insert(
        egui::TextStyle::Small,
        egui::FontId::new(12.0, egui::FontFamily::Proportional),
    );
    
    // Interactive elements
    style.interaction.resize_grab_radius_side = 6.0;
    style.interaction.resize_grab_radius_corner = 10.0;
    
    ctx.set_style(style);
}

impl GuiApp {
    fn start_bot(&mut self) {
        if self.bot_running.load(Ordering::Relaxed) {
            return; // Already running
        }
        
        self.bot_running.store(true, Ordering::Relaxed);
        
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
        let (control_tx_bot, mut control_rx_bot) = tokio::sync::mpsc::unbounded_channel();
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
                eprintln!("📊 Initializing tracker (enable_tracker=true)...");
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

