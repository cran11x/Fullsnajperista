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
        ui.spacing_mut().item_spacing = egui::vec2(12.0, 0.0);
        
        // Enhanced status indicator with premium glow effect
        let status_color = if running {
            egui::Color32::from_rgb(0, 255, 120)
        } else {
            egui::Color32::from_rgb(255, 100, 100)
        };
        
        let status_text = if running { "● Running" } else { "● Stopped" };
        let status_icon = if running { "🟢" } else { "🔴" };
        
        ui.label(egui::RichText::new(status_icon).size(18.0));
        ui.label(egui::RichText::new(status_text)
            .size(15.0)
            .color(status_color)
            .strong());
        
        ui.separator();
        
        // Premium button with enhanced styling and hover effects
        let button_text = if running { "⏸  Stop Bot" } else { "▶  Start Bot" };
        let button_color = if running {
            egui::Color32::from_rgb(255, 100, 100)
        } else {
            egui::Color32::from_rgb(0, 240, 120)
        };
        
        let button_response = ui.add(egui::Button::new(egui::RichText::new(button_text)
                .size(15.0)
                .strong())
                .fill(button_color.linear_multiply(0.25))
                .stroke(egui::Stroke::new(2.0, button_color))
                .min_size(egui::vec2(130.0, 38.0))
                .rounding(egui::Rounding::same(8.0)));
        
        // Enhanced hover effect
        if button_response.hovered() {
            ui.painter().rect_filled(
                button_response.rect,
                8.0,
                button_color.linear_multiply(0.15),
            );
        }
        
        if button_response.clicked() {
            let control = if running {
                BotControl::Stop
            } else {
                BotControl::Start
            };
            let _ = control_tx.send(control);
        }
        
        ui.separator();
        
        // Premium wallet info card with enhanced styling
        ui.group(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(8.0, 6.0);
            
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("💼").size(16.0));
                    ui.label(egui::RichText::new(format_address(wallet_address))
                        .size(13.0)
                        .color(egui::Color32::from_rgb(190, 210, 255))
                        .monospace());
                });
                
                let balance = wallet_balance.read().unwrap();
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("💎").size(16.0));
                    ui.label(egui::RichText::new(format!("{:.4} SOL", *balance))
                        .size(14.0)
                        .strong()
                        .color(egui::Color32::from_rgb(255, 220, 0)));
                });
                
                // Target mint address (if set)
                if let Ok(cfg) = config.read() {
                    if let Some(target_mint) = cfg.target_mint_address {
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new("🎯").size(16.0));
                            ui.label(egui::RichText::new(format_address(&target_mint.to_string()))
                                .size(12.0)
                                .color(egui::Color32::from_rgb(255, 160, 210))
                                .monospace());
                        });
                    }
                }
                
                // Enhanced compact metrics
                if let Ok(m) = metrics.read() {
                    let success_rate = m.success_rate() * 100.0;
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(format!("🔍 {}", m.total_detected))
                            .size(12.0)
                            .color(egui::Color32::from_rgb(160, 210, 255)));
                        ui.label(egui::RichText::new(format!("✓ {}", m.total_successful))
                            .size(12.0)
                            .color(egui::Color32::from_rgb(100, 255, 160)));
                        ui.label(egui::RichText::new(format!("{:.1}%", success_rate))
                            .size(12.0)
                            .strong()
                            .color(if success_rate > 50.0 {
                                egui::Color32::from_rgb(100, 255, 160)
                            } else {
                                egui::Color32::from_rgb(255, 190, 110)
                            }));
                    });
                }
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
        ui.set_min_width(190.0);
        ui.set_min_height(100.0);
        ui.spacing_mut().item_spacing = egui::vec2(0.0, 8.0);
        
        ui.vertical_centered(|ui| {
            ui.add_space(12.0);
            ui.label(egui::RichText::new(label.to_uppercase())
                .size(12.0)
                .color(egui::Color32::from_rgb(190, 195, 205)));
            
            ui.add_space(4.0);
            ui.label(egui::RichText::new(value)
                .size(36.0)
                .strong()
                .color(color));
            
            // Enhanced glow effect at bottom with gradient
            let rect = ui.available_rect_before_wrap();
            ui.painter().rect_filled(
                egui::Rect::from_min_max(
                    rect.min + egui::vec2(6.0, rect.height() - 3.0),
                    rect.max - egui::vec2(6.0, 0.0)
                ),
                4.0,
                color.linear_multiply(0.2),
            );
        });
    });
    
    // Add subtle shadow effect using painter
    ui.painter().rect_stroke(
        response.response.rect,
        8.0,
        egui::Stroke::new(1.0, egui::Color32::from_rgb(0, 0, 0).linear_multiply(0.3)),
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

