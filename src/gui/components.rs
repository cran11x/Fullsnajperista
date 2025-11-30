// components.rs - Reusable UI components

use eframe::egui;
use std::sync::{Arc, mpsc, atomic::{AtomicBool, Ordering}};
use crate::metrics::SharedMetrics;
use crate::config::Config;
use super::events::BotControl;

pub fn render_control_panel(
    ui: &mut egui::Ui,
    bot_running: &Arc<AtomicBool>,
    wallet_address: &str,
    wallet_balance: &Arc<std::sync::RwLock<f64>>,
    metrics: &SharedMetrics,
    config: &Arc<std::sync::RwLock<Config>>,
    control_tx: &mpsc::Sender<BotControl>,
) {
    let running = bot_running.load(Ordering::Relaxed);
    
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing = egui::vec2(10.0, 0.0);
        
        // Status indicator - FIRST (compact)
        let status_color = if running {
            egui::Color32::from_rgb(0, 255, 120)
        } else {
            egui::Color32::from_rgb(255, 100, 100)
        };
        
        let status_text = if running { "Running" } else { "Stopped" };
        let status_icon = if running { "🟢" } else { "🔴" };
        
        ui.vertical_centered(|ui| {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(status_icon).size(16.0));
                ui.add_space(4.0);
                ui.label(egui::RichText::new(status_text)
                    .size(12.0)
                    .color(status_color)
                    .strong());
            });
        });
        
        ui.separator();
        
        // Compact wallet info - minimal to save space
        ui.vertical_centered(|ui| {
            let balance = wallet_balance.read().unwrap();
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("💎").size(14.0));
                ui.add_space(3.0);
                ui.label(egui::RichText::new(format!("{:.4} SOL", *balance))
                    .size(12.0)
                    .strong()
                    .color(egui::Color32::from_rgb(255, 220, 0)));
            });
        });
    });
}

fn format_address(addr: &str) -> String {
    if addr.len() > 16 {
        format!("{}...{}", &addr[..8], &addr[addr.len()-8..])
    } else {
        addr.to_string()
    }
}

pub fn render_metric_card(ui: &mut egui::Ui, label: &str, value: &str, color: egui::Color32) {
    let response = ui.group(|ui| {
        ui.set_min_width(200.0);
        ui.set_min_height(110.0);
        ui.spacing_mut().item_spacing = egui::vec2(0.0, 10.0);
        
        ui.vertical_centered(|ui| {
            ui.add_space(16.0);
            ui.label(egui::RichText::new(label.to_uppercase())
                .size(13.0)
                .color(egui::Color32::from_rgb(180, 190, 200)));
            
            ui.add_space(6.0);
            ui.label(egui::RichText::new(value)
                .size(38.0)
                .strong()
                .color(color));
            
            // Enhanced glow effect at bottom with gradient
            let rect = ui.available_rect_before_wrap();
            ui.painter().rect_filled(
                egui::Rect::from_min_max(
                    rect.min + egui::vec2(8.0, rect.height() - 4.0),
                    rect.max - egui::vec2(8.0, 0.0)
                ),
                6.0,
                color.linear_multiply(0.25),
            );
        });
    });
    
    // Add subtle shadow effect using painter
    ui.painter().rect_stroke(
        response.response.rect,
        10.0,
        egui::Stroke::new(1.5, egui::Color32::from_rgb(0, 0, 0).linear_multiply(0.4)),
    );
}

pub fn render_progress_bar(ui: &mut egui::Ui, label: &str, progress: f32, color: egui::Color32) {
    ui.group(|ui| {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(label)
                .size(15.0)
                .strong()
                .color(egui::Color32::from_rgb(220, 225, 235)));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(egui::RichText::new(format!("{:.1}%", progress * 100.0))
                    .size(15.0)
                    .strong()
                    .color(color));
            });
        });
        ui.add_space(8.0);
        let progress_bar = ui.add(egui::ProgressBar::new(progress.clamp(0.0, 1.0))
            .fill(color)
            .show_percentage()
            .desired_width(ui.available_width()));
        
        // Enhanced progress bar with glow effect
        if progress > 0.0 {
            ui.painter().rect_filled(
                progress_bar.rect,
                6.0,
                color.linear_multiply(0.3),
            );
        }
    });
}

