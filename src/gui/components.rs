// components.rs - Reusable UI components

use eframe::egui;
use std::sync::{Arc, mpsc, atomic::{AtomicBool, Ordering}};
use crate::metrics::SharedMetrics;
use crate::config::Config;
use super::events::BotControl;

#[allow(dead_code)]
pub fn render_control_panel(
    ui: &mut egui::Ui,
    bot_running: &Arc<AtomicBool>,
    _wallet_address: &str,
    wallet_balance: &Arc<std::sync::RwLock<f64>>,
    _metrics: &SharedMetrics,
    _config: &Arc<std::sync::RwLock<Config>>,
    _control_tx: &mpsc::Sender<BotControl>,
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


pub fn render_metric_card(ui: &mut egui::Ui, label: &str, value: &str, _color: egui::Color32) {
    let response = ui.group(|ui| {
        ui.set_min_width(200.0);
        ui.set_min_height(110.0);
        ui.spacing_mut().item_spacing = egui::vec2(0.0, 10.0);
        
        ui.vertical_centered(|ui| {
            ui.add_space(16.0);
            ui.label(egui::RichText::new(label.to_uppercase())
                .size(13.0)
                .color(egui::Color32::from_rgb(160, 160, 170))); // Sekundarni tekst
            
            ui.add_space(6.0);
            ui.label(egui::RichText::new(value)
                .size(38.0)
                .strong()
                .color(egui::Color32::from_rgb(240, 240, 245))); // Bijeli tekst
            
            // Crvena linija na dnu
            let rect = ui.available_rect_before_wrap();
            ui.painter().rect_filled(
                egui::Rect::from_min_max(
                    rect.min + egui::vec2(8.0, rect.height() - 4.0),
                    rect.max - egui::vec2(8.0, 0.0)
                ),
                6.0,
                egui::Color32::from_rgb(220, 40, 40), // Primarna crvena
            );
        });
    });
    
    // Crveni glow efekt na dnu
    ui.painter().rect_filled(
        egui::Rect::from_min_max(
            response.response.rect.min + egui::vec2(0.0, response.response.rect.height() - 2.0),
            response.response.rect.max,
        ),
        0.0,
        egui::Color32::from_rgb(220, 40, 40).linear_multiply(0.15),
    );
    
    // Border sa crvenim akcentom
    ui.painter().rect_stroke(
        response.response.rect,
        10.0,
        egui::Stroke::new(1.0, egui::Color32::from_rgb(45, 45, 55)),
    );
}

pub fn render_progress_bar(ui: &mut egui::Ui, label: &str, progress: f32, _color: egui::Color32) {
    ui.group(|ui| {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(label)
                .size(15.0)
                .strong()
                .color(egui::Color32::from_rgb(240, 240, 245))); // Bijeli tekst
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(egui::RichText::new(format!("{:.1}%", progress * 100.0))
                    .size(15.0)
                    .strong()
                    .color(egui::Color32::from_rgb(255, 70, 70))); // Svijetla crvena
            });
        });
        ui.add_space(8.0);
        let progress_bar = ui.add(egui::ProgressBar::new(progress.clamp(0.0, 1.0))
            .fill(egui::Color32::from_rgb(220, 40, 40)) // Primarna crvena
            .show_percentage()
            .desired_width(ui.available_width()));
        
        // Crveni glow efekt
        if progress > 0.0 {
            ui.painter().rect_filled(
                progress_bar.rect,
                6.0,
                egui::Color32::from_rgb(255, 70, 70).linear_multiply(0.2),
            );
        }
    });
}

pub fn render_sidebar_button(
    ui: &mut egui::Ui,
    icon: &str,
    label: &str,
    is_selected: bool,
) -> egui::Response {
    let button_height = 42.0;
    let button_width = ui.available_width() - 16.0;
    
    let bg_color = if is_selected {
        egui::Color32::from_rgb(220, 40, 40).linear_multiply(0.2) // Crvena pozadina
    } else {
        egui::Color32::TRANSPARENT
    };
    
    let text_color = if is_selected {
        egui::Color32::from_rgb(255, 70, 70) // Svijetla crvena
    } else {
        egui::Color32::from_rgb(160, 160, 170) // Sivi tekst
    };
    
    let response = ui.add_sized(
        egui::vec2(button_width, button_height),
        egui::Button::new(egui::RichText::new(format!("{} {}", icon, label))
            .size(14.0)
            .strong()
            .color(text_color))
            .fill(bg_color)
            .rounding(egui::Rounding::same(8.0))
    );
    
    // Crvena linija lijevo kad je selektiran
    if is_selected {
        ui.painter().rect_filled(
            egui::Rect::from_min_max(
                response.rect.left_top() + egui::vec2(0.0, 0.0),
                response.rect.left_bottom() + egui::vec2(3.0, 0.0),
            ),
            0.0,
            egui::Color32::from_rgb(220, 40, 40),
        );
    }
    
    // Hover efekt
    if response.hovered() && !is_selected {
        ui.painter().rect_filled(
            response.rect,
            8.0,
            egui::Color32::from_rgb(255, 70, 70).linear_multiply(0.1),
        );
    }
    
    response
}

