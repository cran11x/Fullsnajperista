// tabs/feed.rs - Enhanced Live token feed with filters and full addresses

use eframe::egui;
use std::collections::VecDeque;
use std::sync::{Arc, RwLock};
use crate::gui::events::TokenEvent;

#[derive(Default)]
pub struct FeedState {
    pub show_full_addresses: bool,
    pub filter_detected: bool,
    pub filter_filtered: bool,
    pub filter_bought: bool,
    pub filter_sold: bool,
    pub filter_error: bool,
    pub filter_info: bool,
    pub search_text: String,
}

pub fn render(ui: &mut egui::Ui, event_log: &Arc<RwLock<VecDeque<TokenEvent>>>, auto_scroll: &mut bool, state: &mut FeedState) {
    // Enhanced header with premium styling
    ui.horizontal(|ui| {
        ui.vertical_centered(|ui| {
            ui.label(egui::RichText::new("🔴 Live Feed")
                .size(26.0)
                .strong()
                .color(egui::Color32::from_rgb(255, 120, 120)));
            ui.label(egui::RichText::new("Real-time token detection and trading events")
                .size(13.0)
                .color(egui::Color32::from_rgb(160, 170, 185)));
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let clear_response = ui.add(egui::Button::new(egui::RichText::new("🗑️  Clear")
                    .size(14.0)
                    .strong())
                    .fill(egui::Color32::from_rgb(255, 100, 100).linear_multiply(0.2))
                    .stroke(egui::Stroke::new(1.5, egui::Color32::from_rgb(255, 100, 100)))
                    .min_size(egui::vec2(90.0, 34.0))
                    .rounding(egui::Rounding::same(6.0)));
            if clear_response.clicked() {
                if let Ok(mut log) = event_log.write() {
                    log.clear();
                }
            }
            ui.add_space(8.0);
            ui.checkbox(auto_scroll, egui::RichText::new("Auto-scroll")
                .size(13.0));
        });
    });
    ui.add_space(12.0);
    
    // Enhanced filters and options with premium styling
    ui.group(|ui| {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Filters:")
                .size(13.0)
                .strong()
                .color(egui::Color32::from_rgb(220, 230, 245)));
            ui.add_space(12.0);
            
            ui.checkbox(&mut state.filter_detected, egui::RichText::new("🔍 Detected")
                .size(13.0));
            ui.add_space(6.0);
            ui.checkbox(&mut state.filter_filtered, egui::RichText::new("⏭️ Filtered")
                .size(13.0));
            ui.add_space(6.0);
            ui.checkbox(&mut state.filter_bought, egui::RichText::new("✅ Bought")
                .size(13.0));
            ui.add_space(6.0);
            ui.checkbox(&mut state.filter_sold, egui::RichText::new("💰 Sold")
                .size(13.0));
            ui.add_space(6.0);
            ui.checkbox(&mut state.filter_error, egui::RichText::new("❌ Error")
                .size(13.0));
            ui.add_space(6.0);
            ui.checkbox(&mut state.filter_info, egui::RichText::new("ℹ️ Info")
                .size(13.0));
            
            ui.add_space(24.0);
            
            ui.checkbox(&mut state.show_full_addresses, egui::RichText::new("Show Full Addresses")
                .size(13.0));
        });
        
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("🔍 Search:")
                .size(13.0)
                .strong()
                .color(egui::Color32::from_rgb(220, 230, 245)));
            ui.add_space(8.0);
            ui.add(egui::TextEdit::singleline(&mut state.search_text)
                .desired_width(300.0));
            if !state.search_text.is_empty() {
                ui.add_space(6.0);
                if ui.button(egui::RichText::new("✕")
                        .size(12.0))
                        .clicked() {
                    state.search_text.clear();
                }
            }
        });
    });
    
    ui.add_space(8.0);
    
    let log = event_log.read().unwrap();
    
    // Count events by type
    let (detected_count, filtered_count, bought_count, sold_count, error_count, _info_count) = {
        let mut counts = (0, 0, 0, 0, 0, 0);
        for event in log.iter() {
            match event {
                TokenEvent::Detected { .. } => counts.0 += 1,
                TokenEvent::Filtered { .. } => counts.1 += 1,
                TokenEvent::Bought { .. } => counts.2 += 1,
                TokenEvent::Sold { .. } => counts.3 += 1,
                TokenEvent::Error { .. } => counts.4 += 1,
                TokenEvent::Info { .. } => counts.5 += 1,
            }
        }
        counts
    };
    
    // Enhanced summary stats with premium styling
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(format!("Total: {}", log.len()))
            .size(14.0)
            .strong()
            .color(egui::Color32::from_rgb(220, 230, 245)));
        ui.add_space(14.0);
        ui.label(egui::RichText::new(format!("🔍 {}", detected_count))
            .size(13.0)
            .strong()
            .color(egui::Color32::from_rgb(120, 200, 255)));
        ui.add_space(10.0);
        ui.label(egui::RichText::new(format!("⏭️ {}", filtered_count))
            .size(13.0)
            .strong()
            .color(egui::Color32::from_rgb(255, 190, 120)));
        ui.add_space(10.0);
        ui.label(egui::RichText::new(format!("✅ {}", bought_count))
            .size(13.0)
            .strong()
            .color(egui::Color32::from_rgb(0, 255, 140)));
        ui.add_space(10.0);
        ui.label(egui::RichText::new(format!("💰 {}", sold_count))
            .size(13.0)
            .strong()
            .color(egui::Color32::from_rgb(255, 220, 0)));
        ui.add_space(10.0);
        ui.label(egui::RichText::new(format!("❌ {}", error_count))
            .size(13.0)
            .strong()
            .color(egui::Color32::from_rgb(255, 120, 120)));
    });
    
    ui.add_space(12.0);
    
    // Filter and search events
    let events_to_show: Vec<_> = log.iter().rev().take(500).filter(|event| {
        // Type filter
        let type_match = match event {
            TokenEvent::Detected { .. } => state.filter_detected,
            TokenEvent::Filtered { .. } => state.filter_filtered,
            TokenEvent::Bought { .. } => state.filter_bought,
            TokenEvent::Sold { .. } => state.filter_sold,
            TokenEvent::Error { .. } => state.filter_error,
            TokenEvent::Info { .. } => state.filter_info,
        };
        
        // Search filter
        let search_match = if state.search_text.is_empty() {
            true
        } else {
            let search_lower = state.search_text.to_lowercase();
            match event {
                TokenEvent::Detected { mint, .. } => mint.to_lowercase().contains(&search_lower),
                TokenEvent::Filtered { mint, reason, .. } => 
                    mint.to_lowercase().contains(&search_lower) || reason.to_lowercase().contains(&search_lower),
                TokenEvent::Bought { mint, signature, .. } => 
                    mint.to_lowercase().contains(&search_lower) || signature.to_lowercase().contains(&search_lower),
                TokenEvent::Sold { mint, signature, reason, .. } => 
                    mint.to_lowercase().contains(&search_lower) || signature.to_lowercase().contains(&search_lower) || reason.to_lowercase().contains(&search_lower),
                TokenEvent::Error { message, .. } => message.to_lowercase().contains(&search_lower),
                TokenEvent::Info { message, .. } => message.to_lowercase().contains(&search_lower),
            }
        };
        
        type_match && search_match
    }).collect();
    
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing = egui::vec2(8.0, 4.0);
            
            if events_to_show.is_empty() {
                ui.vertical_centered(|ui| {
                    ui.add_space(60.0);
                    ui.label(egui::RichText::new("No events match filters")
                        .size(16.0)
                        .color(egui::Color32::from_rgb(160, 170, 185)));
                });
                return;
            }
            
            for event in events_to_show {
                    let (color, icon, bg_color) = match event {
                    TokenEvent::Detected { .. } => (
                        egui::Color32::from_rgb(120, 200, 255),
                        "🔍",
                        egui::Color32::from_rgb(120, 200, 255).linear_multiply(0.12)
                    ),
                    TokenEvent::Filtered { .. } => (
                        egui::Color32::from_rgb(255, 190, 120),
                        "⏭️",
                        egui::Color32::from_rgb(255, 190, 120).linear_multiply(0.12)
                    ),
                    TokenEvent::Bought { .. } => (
                        egui::Color32::from_rgb(0, 255, 140),
                        "✅",
                        egui::Color32::from_rgb(0, 255, 140).linear_multiply(0.12)
                    ),
                    TokenEvent::Sold { .. } => (
                        egui::Color32::from_rgb(255, 220, 0),
                        "💰",
                        egui::Color32::from_rgb(255, 220, 0).linear_multiply(0.12)
                    ),
                    TokenEvent::Error { .. } => (
                        egui::Color32::from_rgb(255, 120, 120),
                        "❌",
                        egui::Color32::from_rgb(255, 120, 120).linear_multiply(0.12)
                    ),
                    TokenEvent::Info { .. } => (
                        egui::Color32::from_rgb(190, 200, 220),
                        "ℹ️",
                        egui::Color32::from_rgb(190, 200, 220).linear_multiply(0.08)
                    ),
                };
                
                let group_response = ui.group(|ui| {
                    ui.set_min_height(38.0);
                    ui.vertical(|ui| {
                        ui.horizontal(|ui| {
                            ui.add_space(10.0);
                            ui.label(egui::RichText::new(icon).size(18.0));
                            ui.add_space(8.0);
                            
                            // Enhanced time display
                            ui.label(egui::RichText::new(
                                event.timestamp().format("%H:%M:%S").to_string()
                            )
                            .size(11.0)
                            .monospace()
                            .color(egui::Color32::from_rgb(150, 160, 180)));
                            
                            ui.add_space(12.0);
                            
                            // Event content with better formatting
                            match event {
                                TokenEvent::Detected { mint, .. } => {
                                    let mint_display = if state.show_full_addresses {
                                        mint.clone()
                                    } else {
                                        format_address(mint)
                                    };
                                    let mint_label = ui.selectable_label(false, egui::RichText::new(format!("Detected: {}", mint_display))
                                        .size(13.0)
                                        .strong()
                                        .color(color));
                                    if mint_label.clicked() {
                                        ui.output_mut(|o| {
                                            o.copied_text = mint.clone();
                                        });
                                    }
                                },
                                TokenEvent::Filtered { mint, reason, .. } => {
                                    let mint_display = if state.show_full_addresses {
                                        mint.clone()
                                    } else {
                                        format_address(mint)
                                    };
                                    ui.horizontal(|ui| {
                                        let mint_label = ui.selectable_label(false, egui::RichText::new(format!("Filtered: {}", mint_display))
                                            .size(13.0)
                                            .strong()
                                            .color(color));
                                        if mint_label.clicked() {
                                            ui.output_mut(|o| {
                                                o.copied_text = mint.clone();
                                            });
                                        }
                                        ui.add_space(10.0);
                                        ui.label(egui::RichText::new(format!("- {}", reason))
                                            .size(12.0)
                                            .color(egui::Color32::from_rgb(210, 215, 230)));
                                    });
                                },
                                TokenEvent::Bought { mint, signature, mc, .. } => {
                                    let mint_display = if state.show_full_addresses {
                                        mint.clone()
                                    } else {
                                        format_address(mint)
                                    };
                                    let sig_display = if state.show_full_addresses {
                                        signature.clone()
                                    } else {
                                        format_address(signature)
                                    };
                                    ui.horizontal(|ui| {
                                        let mint_label = ui.selectable_label(false, egui::RichText::new(format!("Bought: {}", mint_display))
                                            .size(13.0)
                                            .strong()
                                            .color(color));
                                        if mint_label.clicked() {
                                            ui.output_mut(|o| {
                                                o.copied_text = mint.clone();
                                            });
                                        }
                                        if let Some(mc_val) = mc {
                                            ui.add_space(10.0);
                                            ui.label(egui::RichText::new(format!("MC: ${:.0}", mc_val))
                                                .size(12.0)
                                                .strong()
                                                .color(egui::Color32::from_rgb(100, 255, 160)));
                                        }
                                        ui.add_space(10.0);
                                        let sig_label = ui.selectable_label(false, egui::RichText::new(format!("Sig: {}", sig_display))
                                            .size(12.0)
                                            .monospace()
                                            .color(egui::Color32::from_rgb(160, 210, 255)));
                                        if sig_label.clicked() {
                                            ui.output_mut(|o| {
                                                o.copied_text = signature.clone();
                                            });
                                        }
                                    });
                                },
                                TokenEvent::Sold { mint, signature, reason, pnl, .. } => {
                                    let mint_display = if state.show_full_addresses {
                                        mint.clone()
                                    } else {
                                        format_address(mint)
                                    };
                                    let sig_display = if state.show_full_addresses {
                                        signature.clone()
                                    } else {
                                        format_address(signature)
                                    };
                                    ui.horizontal(|ui| {
                                        let mint_label = ui.selectable_label(false, egui::RichText::new(format!("Sold: {}", mint_display))
                                            .size(13.0)
                                            .strong()
                                            .color(color));
                                        if mint_label.clicked() {
                                            ui.output_mut(|o| {
                                                o.copied_text = mint.clone();
                                            });
                                        }
                                        ui.add_space(10.0);
                                        ui.label(egui::RichText::new(format!("({})", reason))
                                            .size(12.0)
                                            .color(egui::Color32::from_rgb(210, 215, 230)));
                                        if let Some(pnl_val) = pnl {
                                            ui.add_space(10.0);
                                            let pnl_color = if *pnl_val >= 0.0 {
                                                egui::Color32::from_rgb(0, 255, 140)
                                            } else {
                                                egui::Color32::from_rgb(255, 120, 120)
                                            };
                                            ui.label(egui::RichText::new(format!("PnL: {:.4} SOL", pnl_val))
                                                .size(12.0)
                                                .strong()
                                                .color(pnl_color));
                                        }
                                        ui.add_space(10.0);
                                        let sig_label = ui.selectable_label(false, egui::RichText::new(format!("Sig: {}", sig_display))
                                            .size(12.0)
                                            .monospace()
                                            .color(egui::Color32::from_rgb(160, 210, 255)));
                                        if sig_label.clicked() {
                                            ui.output_mut(|o| {
                                                o.copied_text = signature.clone();
                                            });
                                        }
                                    });
                                },
                                TokenEvent::Error { message, .. } => {
                                    ui.label(egui::RichText::new(format!("Error: {}", message))
                                        .size(13.0)
                                        .strong()
                                        .color(color));
                                },
                                TokenEvent::Info { message, .. } => {
                                    ui.label(egui::RichText::new(message)
                                        .size(13.0)
                                        .color(color));
                                },
                            }
                        });
                    });
                });
                
                // Enhanced background color effect
                ui.painter().rect_filled(
                    group_response.response.rect,
                    6.0,
                    bg_color,
                );
            }
            
            // Scroll to bottom if auto-scroll enabled
            if *auto_scroll && log.len() > 0 {
                ui.scroll_to_cursor(Some(egui::Align::BOTTOM));
            }
        });
}

fn format_address(addr: &str) -> String {
    if addr.len() > 12 {
        format!("{}...{}", &addr[..6], &addr[addr.len()-6..])
    } else {
        addr.to_string()
    }
}

