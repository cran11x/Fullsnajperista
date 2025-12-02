// tabs/filtered.rs - Filtered tokens list with full mint addresses and reasons

use eframe::egui;
use std::collections::VecDeque;
use std::sync::{Arc, RwLock};
use crate::gui::events::TokenEvent;
use chrono::Utc;

#[derive(Clone)]
pub struct FilteredToken {
    pub mint: String,
    pub reason: String,
    pub timestamp: chrono::DateTime<Utc>,
}

pub fn render(ui: &mut egui::Ui, event_log: &Arc<RwLock<VecDeque<TokenEvent>>>) {
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.vertical_centered(|ui| {
                    ui.label(egui::RichText::new("⏭️  Filtered Tokens")
                        .size(28.0)
                        .strong()
                        .color(egui::Color32::from_rgb(255, 190, 120)));
                    ui.add_space(4.0);
            ui.label(egui::RichText::new("All detected tokens that were not bought with reasons")
                .size(14.0)
                .color(egui::Color32::from_rgb(160, 170, 185)));
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let clear_response = ui.add(egui::Button::new(egui::RichText::new("🗑️  Clear")
                    .size(14.0)
                    .strong())
                    .fill(egui::Color32::from_rgb(255, 100, 100).linear_multiply(0.2))
                    .stroke(egui::Stroke::new(1.5, egui::Color32::from_rgb(255, 100, 100)))
                    .min_size(egui::vec2(100.0, 36.0))
                    .rounding(egui::Rounding::same(8.0)));
            if clear_response.clicked() {
                // Clear only filtered tokens from log
                if let Ok(mut log) = event_log.write() {
                    log.retain(|e| !matches!(e, TokenEvent::Filtered { .. }));
                }
            }
            ui.add_space(10.0);
            let export_response = ui.add(egui::Button::new(egui::RichText::new("💾 Export CSV")
                    .size(14.0)
                    .strong())
                    .fill(egui::Color32::from_rgb(100, 200, 255).linear_multiply(0.2))
                    .stroke(egui::Stroke::new(1.5, egui::Color32::from_rgb(100, 200, 255)))
                    .min_size(egui::vec2(130.0, 36.0))
                    .rounding(egui::Rounding::same(8.0)));
            if export_response.clicked() {
                export_to_csv(event_log);
            }
        });
    });
    ui.add_space(20.0);
    
    let log = event_log.read().unwrap();
    
    // Extract all filtered tokens
    let filtered_tokens: Vec<FilteredToken> = log.iter()
        .filter_map(|e| {
            if let TokenEvent::Filtered { mint, reason, timestamp } = e {
                Some(FilteredToken {
                    mint: mint.clone(),
                    reason: reason.clone(),
                    timestamp: *timestamp,
                })
            } else {
                None
            }
        })
        .collect();
    
    if filtered_tokens.is_empty() {
        ui.vertical_centered(|ui| {
            ui.add_space(60.0);
            ui.label(egui::RichText::new("No filtered tokens yet")
                .size(18.0)
                .color(egui::Color32::from_rgb(160, 170, 185)));
            ui.label(egui::RichText::new("Filtered tokens will appear here when detected")
                .size(13.0)
                .color(egui::Color32::from_rgb(130, 140, 155)));
        });
        return;
    }
    
    // Enhanced summary with premium styling
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(format!("Total Filtered: {}", filtered_tokens.len()))
            .size(17.0)
            .strong()
            .color(egui::Color32::from_rgb(255, 190, 120)));
        ui.add_space(28.0);
        
        // Enhanced count by reason
        use std::collections::HashMap;
        let mut reason_counts: HashMap<&str, usize> = HashMap::new();
        for token in &filtered_tokens {
            *reason_counts.entry(&token.reason).or_insert(0) += 1;
        }
        
        for (reason, count) in reason_counts.iter() {
            ui.label(egui::RichText::new(format!("{}: {}", reason, count))
                .size(14.0)
                .strong()
                .color(egui::Color32::from_rgb(220, 230, 245)));
            ui.add_space(14.0);
        }
    });
    
    ui.add_space(16.0);
    
    // Table with filtered tokens
    // NOTE: Inner ScrollArea removed to prevent nested scrolling issues
    ui.vertical(|ui| {
            // Enhanced table header with premium styling
            ui.horizontal(|ui| {
                ui.set_min_height(36.0);
                ui.set_width(90.0); // Time column
                ui.label(egui::RichText::new("Time")
                    .size(14.0)
                    .strong()
                    .color(egui::Color32::from_rgb(220, 230, 245)));
                ui.add_space(12.0);
                ui.set_width(460.0); // Mint Address column - wider for full address
                ui.label(egui::RichText::new("Mint Address (Full)")
                    .size(14.0)
                    .strong()
                    .color(egui::Color32::from_rgb(220, 230, 245)));
                ui.add_space(12.0);
                ui.label(egui::RichText::new("Reason")
                    .size(14.0)
                    .strong()
                    .color(egui::Color32::from_rgb(220, 230, 245)));
            });
            ui.add_space(6.0);
            ui.separator();
            ui.add_space(6.0);
            
            // Enhanced tokens display with premium styling
            for token in filtered_tokens.iter().rev() {
                ui.horizontal(|ui| {
                    ui.set_min_height(30.0);
                    
                    // Enhanced time column
                    ui.set_width(90.0);
                    ui.label(egui::RichText::new(
                        token.timestamp.format("%H:%M:%S").to_string()
                    )
                    .size(12.0)
                    .monospace()
                    .color(egui::Color32::from_rgb(150, 170, 190)));
                    
                    ui.add_space(12.0);
                    
                    // Enhanced mint address column - FULL address, clickable to copy
                    ui.set_width(460.0);
                    let mint_response = ui.selectable_label(false, egui::RichText::new(&token.mint)
                        .size(12.0)
                        .monospace()
                        .color(egui::Color32::from_rgb(210, 230, 255)));
                    
                    if mint_response.clicked() {
                        ui.output_mut(|o| {
                            o.copied_text = token.mint.clone();
                        });
                    }
                    
                    ui.add_space(12.0);
                    
                    // Enhanced reason column with better color coding
                    let reason_color = if token.reason.contains("target") {
                        egui::Color32::from_rgb(255, 210, 110)
                    } else if token.reason.contains("Duplicate") {
                        egui::Color32::from_rgb(210, 220, 230)
                    } else if token.reason.contains("Dev buy") {
                        egui::Color32::from_rgb(255, 160, 110)
                    } else if token.reason.contains("Social") {
                        egui::Color32::from_rgb(255, 190, 130)
                    } else if token.reason.contains("Creator") {
                        egui::Color32::from_rgb(255, 170, 110)
                    } else {
                        egui::Color32::from_rgb(255, 180, 120)
                    };
                    
                    ui.label(egui::RichText::new(&token.reason)
                        .size(12.0)
                        .strong()
                        .color(reason_color));
                });
                
                ui.add_space(4.0);
            }
        });
    
    }); // End ScrollArea
}

fn export_to_csv(event_log: &Arc<RwLock<VecDeque<TokenEvent>>>) {
    let log = event_log.read().unwrap();
    
    let filtered_tokens: Vec<FilteredToken> = log.iter()
        .filter_map(|e| {
            if let TokenEvent::Filtered { mint, reason, timestamp } = e {
                Some(FilteredToken {
                    mint: mint.clone(),
                    reason: reason.clone(),
                    timestamp: *timestamp,
                })
            } else {
                None
            }
        })
        .collect();
    
    if filtered_tokens.is_empty() {
        // Add info message to event log
        if let Ok(mut log) = event_log.write() {
            log.push_back(TokenEvent::Info {
                message: "No filtered tokens to export".to_string(),
                timestamp: chrono::Utc::now(),
            });
        }
        return;
    }
    
    // Create CSV content with proper escaping
    let mut csv = String::from("Timestamp,Mint Address,Reason\n");
    for token in &filtered_tokens {
        // Escape commas and quotes in reason
        let escaped_reason = token.reason
            .replace('"', "\"\"")  // Escape quotes
            .replace(',', ";");     // Replace commas with semicolons
        
        csv.push_str(&format!(
            "{},\"{}\",\"{}\"\n",
            token.timestamp.format("%Y-%m-%d %H:%M:%S"),
            token.mint,
            escaped_reason
        ));
    }
    
    // Save to file in current directory
    let filename = format!("filtered_tokens_{}.csv", 
        chrono::Utc::now().format("%Y%m%d_%H%M%S"));
    
    match std::fs::write(&filename, csv) {
        Ok(_) => {
            // Add success message to event log
            if let Ok(mut log) = event_log.write() {
                log.push_back(TokenEvent::Info {
                    message: format!("✅ Exported {} filtered tokens to {}", filtered_tokens.len(), filename),
                    timestamp: chrono::Utc::now(),
                });
            }
            // Also print to console for debugging
            println!("✅ Exported {} filtered tokens to {}", filtered_tokens.len(), filename);
        }
        Err(e) => {
            // Add error message to event log
            if let Ok(mut log) = event_log.write() {
                log.push_back(TokenEvent::Error {
                    message: format!("Failed to export CSV: {}", e),
                    timestamp: chrono::Utc::now(),
                });
            }
            eprintln!("Failed to export CSV: {}", e);
        }
    }
}

