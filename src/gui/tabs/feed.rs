// tabs/feed.rs - Live token feed

use eframe::egui;
use std::collections::VecDeque;
use std::sync::{Arc, RwLock};
use crate::gui::events::TokenEvent;

pub fn render(ui: &mut egui::Ui, event_log: &Arc<RwLock<VecDeque<TokenEvent>>>, auto_scroll: &mut bool) {
    ui.horizontal(|ui| {
        ui.vertical_centered(|ui| {
            ui.label(egui::RichText::new("🔴 Live Feed")
                .size(22.0)
                .strong()
                .color(egui::Color32::from_rgb(255, 100, 100)));
            ui.label(egui::RichText::new("Real-time token detection and trading events")
                .size(11.0)
                .color(egui::Color32::from_rgb(150, 150, 160)));
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.add(egui::Button::new(egui::RichText::new("🗑️  Clear")
                    .size(13.0))
                    .min_size(egui::vec2(80.0, 30.0)))
                    .clicked() {
                if let Ok(mut log) = event_log.write() {
                    log.clear();
                }
            }
            ui.checkbox(auto_scroll, "Auto-scroll");
        });
    });
    ui.add_space(12.0);
    
    let log = event_log.read().unwrap();
    
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing = egui::vec2(8.0, 4.0);
            
            // Only render last 200 events for better performance
            let events_to_show: Vec<_> = log.iter().rev().take(200).collect();
            
            for event in events_to_show {
                let (color, icon, _bg_color) = match event {
                    TokenEvent::Detected { .. } => (
                        egui::Color32::from_rgb(100, 180, 255),
                        "🔍",
                        egui::Color32::from_rgb(100, 180, 255).linear_multiply(0.08)
                    ),
                    TokenEvent::Filtered { .. } => (
                        egui::Color32::from_rgb(255, 170, 100),
                        "⏭️",
                        egui::Color32::from_rgb(255, 170, 100).linear_multiply(0.08)
                    ),
                    TokenEvent::Bought { .. } => (
                        egui::Color32::from_rgb(0, 255, 120),
                        "✅",
                        egui::Color32::from_rgb(0, 255, 120).linear_multiply(0.08)
                    ),
                    TokenEvent::Error { .. } => (
                        egui::Color32::from_rgb(255, 100, 100),
                        "❌",
                        egui::Color32::from_rgb(255, 100, 100).linear_multiply(0.08)
                    ),
                    TokenEvent::Info { .. } => (
                        egui::Color32::from_rgb(180, 180, 200),
                        "ℹ️",
                        egui::Color32::from_rgb(180, 180, 200).linear_multiply(0.05)
                    ),
                };
                
                ui.group(|ui| {
                    ui.set_min_height(28.0);
                    ui.horizontal(|ui| {
                        ui.add_space(8.0);
                        ui.label(egui::RichText::new(icon).size(18.0));
                        ui.add_space(6.0);
                        
                        ui.label(egui::RichText::new(
                            event.timestamp().format("%H:%M:%S").to_string()
                        )
                        .size(11.0)
                        .monospace()
                        .color(egui::Color32::from_rgb(140, 140, 160)));
                        
                        ui.add_space(8.0);
                        ui.label(egui::RichText::new(event.format_for_display())
                            .size(13.0)
                            .color(color));
                    });
                });
            }
            
            // Scroll to bottom if auto-scroll enabled
            if *auto_scroll && log.len() > 0 {
                ui.scroll_to_cursor(Some(egui::Align::BOTTOM));
            }
        });
}

