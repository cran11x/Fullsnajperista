// tabs/dashboard.rs - Metrics dashboard
#![allow(unused)]

use eframe::egui;
use crate::metrics::SharedMetrics;

pub fn render(ui: &mut egui::Ui, metrics: &SharedMetrics) {
    let m = metrics.read().unwrap();
    
    ui.vertical_centered(|ui| {
        ui.add_space(8.0);
        ui.label(egui::RichText::new("📊 Performance Dashboard")
            .size(22.0)
            .strong()
            .color(egui::Color32::from_rgb(200, 220, 255)));
    });
    ui.add_space(16.0);
    
    // Main metrics row with better spacing
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing = egui::vec2(12.0, 0.0);
        
        crate::gui::components::render_metric_card(
            ui,
            "Detected",
            &m.total_detected.to_string(),
            egui::Color32::from_rgb(100, 180, 255),
        );
        
        crate::gui::components::render_metric_card(
            ui,
            "Filtered",
            &m.total_filtered.to_string(),
            egui::Color32::from_rgb(255, 170, 100),
        );
        
        crate::gui::components::render_metric_card(
            ui,
            "Submitted",
            &m.total_submitted.to_string(),
            egui::Color32::from_rgb(120, 255, 180),
        );
        
        crate::gui::components::render_metric_card(
            ui,
            "Successful",
            &m.total_successful.to_string(),
            egui::Color32::from_rgb(0, 255, 120),
        );
        
        crate::gui::components::render_metric_card(
            ui,
            "Failed",
            &m.total_failed.to_string(),
            egui::Color32::from_rgb(255, 100, 100),
        );
    });
    
    ui.add_space(20.0);
    
    // Success rate
    let success_rate = m.success_rate();
    crate::gui::components::render_progress_bar(
        ui,
        "Success Rate",
        success_rate as f32,
        egui::Color32::from_rgb(0, 200, 0),
    );
    
    ui.add_space(10.0);
    
    // Method breakdown with modern cards
    ui.group(|ui| {
        ui.set_min_height(120.0);
        ui.heading(egui::RichText::new("🚀 Submission Method Performance")
            .size(16.0)
            .color(egui::Color32::from_rgb(180, 200, 255)));
        ui.add_space(12.0);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(16.0, 0.0);
            ui.vertical(|ui| {
                ui.label("Helius:");
                ui.label(egui::RichText::new(format!("✓ {} / ✗ {}", m.helius_success, m.helius_failed))
                    .color(egui::Color32::from_rgb(180, 200, 255)));
                let helius_rate = if (m.helius_success + m.helius_failed) > 0 {
                    m.helius_success as f32 / (m.helius_success + m.helius_failed) as f32
                } else {
                    0.0
                };
                ui.add(egui::ProgressBar::new(helius_rate)
                    .fill(egui::Color32::from_rgb(100, 150, 255)));
            });
            
            ui.vertical(|ui| {
                ui.label("Jito:");
                ui.label(egui::RichText::new(format!("✓ {} / ✗ {}", m.jito_success, m.jito_failed))
                    .color(egui::Color32::from_rgb(255, 220, 150)));
                let jito_rate = if (m.jito_success + m.jito_failed) > 0 {
                    m.jito_success as f32 / (m.jito_success + m.jito_failed) as f32
                } else {
                    0.0
                };
                ui.add(egui::ProgressBar::new(jito_rate)
                    .fill(egui::Color32::from_rgb(255, 200, 100)));
            });
            
            ui.vertical(|ui| {
                ui.label("RPC:");
                ui.label(egui::RichText::new(format!("✓ {} / ✗ {}", m.rpc_success, m.rpc_failed))
                    .color(egui::Color32::from_rgb(150, 255, 180)));
                let rpc_rate = if (m.rpc_success + m.rpc_failed) > 0 {
                    m.rpc_success as f32 / (m.rpc_success + m.rpc_failed) as f32
                } else {
                    0.0
                };
                ui.add(egui::ProgressBar::new(rpc_rate)
                    .fill(egui::Color32::from_rgb(150, 255, 150)));
            });
        });
    });
    
    ui.add_space(10.0);
    
    // Timing statistics
    ui.group(|ui| {
        ui.heading("Timing Statistics (ms)");
        egui::Grid::new("timing_grid").show(ui, |ui| {
            ui.label("Operation");
            ui.label("Avg");
            ui.label("P50");
            ui.label("P95");
            ui.label("P99");
            ui.end_row();
            
            ui.label("Detection");
            ui.label(format!("{:.1}", m.avg_detection_time_ms()));
            ui.label(format!("{}", m.percentile(&m.detection_times, 50.0)));
            ui.label(format!("{}", m.percentile(&m.detection_times, 95.0)));
            ui.label(format!("{}", m.percentile(&m.detection_times, 99.0)));
            ui.end_row();
            
            ui.label("Filter");
            ui.label(format!("{:.1}", m.avg_filter_time_ms()));
            ui.label(format!("{}", m.percentile(&m.filter_times, 50.0)));
            ui.label(format!("{}", m.percentile(&m.filter_times, 95.0)));
            ui.label(format!("{}", m.percentile(&m.filter_times, 99.0)));
            ui.end_row();
            
            ui.label("Submission");
            ui.label(format!("{:.1}", m.avg_submission_time_ms()));
            ui.label(format!("{}", m.percentile(&m.submission_times, 50.0)));
            ui.label(format!("{}", m.percentile(&m.submission_times, 95.0)));
            ui.label(format!("{}", m.percentile(&m.submission_times, 99.0)));
            ui.end_row();
        });
    });
    
    ui.add_space(10.0);
    
    // Filter breakdown
    ui.group(|ui| {
        ui.heading("Filter Breakdown");
        ui.horizontal(|ui| {
            ui.label(format!("Dev Buy: {}", m.filtered_by_dev_buy));
            ui.label(format!("Socials: {}", m.filtered_by_socials));
            ui.label(format!("Creator Count: {}", m.filtered_by_creator_count));
            ui.label(format!("Validation: {}", m.filtered_by_validation));
        });
    });
    
    ui.add_space(10.0);
    
    // Error breakdown
    ui.group(|ui| {
        ui.heading("Error Breakdown");
        ui.horizontal(|ui| {
            ui.label(format!("Network: {}", m.network_errors));
            ui.label(format!("RPC: {}", m.rpc_errors));
            ui.label(format!("Validation: {}", m.validation_errors));
            ui.label(format!("Submission: {}", m.submission_errors));
            ui.label(format!("Timeout: {}", m.timeout_errors));
        });
    });
}

