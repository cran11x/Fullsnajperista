// tabs/dashboard.rs - Metrics dashboard
#![allow(unused)]

use eframe::egui;
use crate::metrics::SharedMetrics;

pub fn render(ui: &mut egui::Ui, metrics: &SharedMetrics) {
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
    let m = match metrics.read() {
        Ok(m) => m,
        Err(e) => {
            ui.label(format!("Error reading metrics: {}", e));
            return;
        }
    };
    
    ui.vertical_centered(|ui| {
        ui.add_space(16.0);
        ui.label(egui::RichText::new("📊 Performance Dashboard")
            .size(28.0)
            .strong()
            .color(egui::Color32::from_rgb(255, 50, 50))); // Crvena
        ui.add_space(4.0);
        ui.label(egui::RichText::new("Real-time metrics and performance analytics")
            .size(14.0)
            .color(egui::Color32::from_rgb(160, 160, 170))); // Siva
    });
    ui.add_space(24.0);
    
    // Premium main metrics row with enhanced spacing
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing = egui::vec2(20.0, 0.0);
        
        crate::gui::components::render_metric_card(
            ui,
            "Detected",
            &m.total_detected.to_string(),
            egui::Color32::from_rgb(120, 200, 255),
        );
        
        crate::gui::components::render_metric_card(
            ui,
            "Filtered",
            &m.total_filtered.to_string(),
            egui::Color32::from_rgb(255, 190, 120),
        );
        
        crate::gui::components::render_metric_card(
            ui,
            "Submitted",
            &m.total_submitted.to_string(),
            egui::Color32::from_rgb(140, 255, 200),
        );
        
        crate::gui::components::render_metric_card(
            ui,
            "Successful",
            &m.total_successful.to_string(),
            egui::Color32::from_rgb(0, 255, 140),
        );
        
        crate::gui::components::render_metric_card(
            ui,
            "Failed",
            &m.total_failed.to_string(),
            egui::Color32::from_rgb(255, 120, 120),
        );
    });
    
    ui.add_space(28.0);
    
    // Enhanced success rate with premium styling
    let success_rate = m.success_rate();
    crate::gui::components::render_progress_bar(
        ui,
        "Success Rate",
        success_rate as f32,
        egui::Color32::from_rgb(220, 40, 40), // Crvena
    );
    
    ui.add_space(20.0);
    
    // Premium method breakdown with enhanced cards
    ui.group(|ui| {
        ui.set_min_height(160.0);
        ui.heading(egui::RichText::new("🚀 Submission Method Performance")
            .size(19.0)
            .strong()
            .color(egui::Color32::from_rgb(255, 70, 70))); // Svijetla crvena
        ui.add_space(18.0);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(20.0, 0.0);
            ui.vertical(|ui| {
                ui.label(egui::RichText::new("Helius")
                    .size(14.0)
                    .strong()
                    .color(egui::Color32::from_rgb(240, 240, 245)));
                ui.add_space(4.0);
                ui.label(egui::RichText::new(format!("✓ {} / ✗ {}", m.helius_success, m.helius_failed))
                    .size(13.0)
                    .color(egui::Color32::from_rgb(160, 160, 170)));
                ui.add_space(6.0);
                let helius_rate = if (m.helius_success + m.helius_failed) > 0 {
                    m.helius_success as f32 / (m.helius_success + m.helius_failed) as f32
                } else {
                    0.0
                };
                ui.add(egui::ProgressBar::new(helius_rate)
                    .fill(egui::Color32::from_rgb(220, 40, 40)) // Crvena
                    .desired_width(180.0));
            });
            
            ui.vertical(|ui| {
                ui.label(egui::RichText::new("Jito")
                    .size(14.0)
                    .strong()
                    .color(egui::Color32::from_rgb(240, 240, 245)));
                ui.add_space(4.0);
                ui.label(egui::RichText::new(format!("✓ {} / ✗ {}", m.jito_success, m.jito_failed))
                    .size(13.0)
                    .color(egui::Color32::from_rgb(160, 160, 170)));
                ui.add_space(6.0);
                let jito_rate = if (m.jito_success + m.jito_failed) > 0 {
                    m.jito_success as f32 / (m.jito_success + m.jito_failed) as f32
                } else {
                    0.0
                };
                ui.add(egui::ProgressBar::new(jito_rate)
                    .fill(egui::Color32::from_rgb(220, 40, 40)) // Crvena
                    .desired_width(180.0));
            });
            
            ui.vertical(|ui| {
                ui.label(egui::RichText::new("RPC")
                    .size(14.0)
                    .strong()
                    .color(egui::Color32::from_rgb(240, 240, 245)));
                ui.add_space(4.0);
                ui.label(egui::RichText::new(format!("✓ {} / ✗ {}", m.rpc_success, m.rpc_failed))
                    .size(13.0)
                    .color(egui::Color32::from_rgb(160, 160, 170)));
                ui.add_space(6.0);
                let rpc_rate = if (m.rpc_success + m.rpc_failed) > 0 {
                    m.rpc_success as f32 / (m.rpc_success + m.rpc_failed) as f32
                } else {
                    0.0
                };
                ui.add(egui::ProgressBar::new(rpc_rate)
                    .fill(egui::Color32::from_rgb(220, 40, 40)) // Crvena
                    .desired_width(180.0));
            });
        });
    });
    
    ui.add_space(20.0);
    
    // Enhanced timing statistics with premium styling
    ui.group(|ui| {
        ui.heading(egui::RichText::new("⏱️ Timing Statistics (ms)")
            .size(19.0)
            .strong()
            .color(egui::Color32::from_rgb(255, 70, 70))); // Svijetla crvena
        ui.add_space(14.0);
        egui::Grid::new("timing_grid")
            .spacing([20.0, 8.0])
            .show(ui, |ui| {
            ui.label(egui::RichText::new("Operation")
                .size(13.0)
                .strong()
                .color(egui::Color32::from_rgb(240, 240, 245)));
            ui.label(egui::RichText::new("Avg")
                .size(13.0)
                .strong()
                .color(egui::Color32::from_rgb(240, 240, 245)));
            ui.label(egui::RichText::new("P50")
                .size(13.0)
                .strong()
                .color(egui::Color32::from_rgb(240, 240, 245)));
            ui.label(egui::RichText::new("P95")
                .size(13.0)
                .strong()
                .color(egui::Color32::from_rgb(240, 240, 245)));
            ui.label(egui::RichText::new("P99")
                .size(13.0)
                .strong()
                .color(egui::Color32::from_rgb(240, 240, 245)));
            ui.end_row();
            
            ui.label(egui::RichText::new("Detection")
                .size(13.0)
                .color(egui::Color32::from_rgb(255, 70, 70))); // Crvena
            ui.label(egui::RichText::new(format!("{:.1}", m.avg_detection_time_ms()))
                .size(13.0)
                .color(egui::Color32::from_rgb(240, 240, 245)));
            ui.label(egui::RichText::new(format!("{}", m.percentile(&m.detection_times, 50.0)))
                .size(13.0)
                .color(egui::Color32::from_rgb(240, 240, 245)));
            ui.label(egui::RichText::new(format!("{}", m.percentile(&m.detection_times, 95.0)))
                .size(13.0)
                .color(egui::Color32::from_rgb(240, 240, 245)));
            ui.label(egui::RichText::new(format!("{}", m.percentile(&m.detection_times, 99.0)))
                .size(13.0)
                .color(egui::Color32::from_rgb(240, 240, 245)));
            ui.end_row();
            
            ui.label(egui::RichText::new("Filter")
                .size(13.0)
                .color(egui::Color32::from_rgb(255, 70, 70))); // Crvena
            ui.label(egui::RichText::new(format!("{:.1}", m.avg_filter_time_ms()))
                .size(13.0)
                .color(egui::Color32::from_rgb(240, 240, 245)));
            ui.label(egui::RichText::new(format!("{}", m.percentile(&m.filter_times, 50.0)))
                .size(13.0)
                .color(egui::Color32::from_rgb(240, 240, 245)));
            ui.label(egui::RichText::new(format!("{}", m.percentile(&m.filter_times, 95.0)))
                .size(13.0)
                .color(egui::Color32::from_rgb(240, 240, 245)));
            ui.label(egui::RichText::new(format!("{}", m.percentile(&m.filter_times, 99.0)))
                .size(13.0)
                .color(egui::Color32::from_rgb(240, 240, 245)));
            ui.end_row();
            
            ui.label(egui::RichText::new("Submission")
                .size(13.0)
                .color(egui::Color32::from_rgb(255, 70, 70))); // Crvena
            ui.label(egui::RichText::new(format!("{:.1}", m.avg_submission_time_ms()))
                .size(13.0)
                .color(egui::Color32::from_rgb(240, 240, 245)));
            ui.label(egui::RichText::new(format!("{}", m.percentile(&m.submission_times, 50.0)))
                .size(13.0)
                .color(egui::Color32::from_rgb(240, 240, 245)));
            ui.label(egui::RichText::new(format!("{}", m.percentile(&m.submission_times, 95.0)))
                .size(13.0)
                .color(egui::Color32::from_rgb(240, 240, 245)));
            ui.label(egui::RichText::new(format!("{}", m.percentile(&m.submission_times, 99.0)))
                .size(13.0)
                .color(egui::Color32::from_rgb(240, 240, 245)));
            ui.end_row();
        });
    });
    
    ui.add_space(20.0);
    
    // Enhanced filter breakdown with premium styling
    ui.group(|ui| {
        ui.heading(egui::RichText::new("🔍 Filter Breakdown")
            .size(19.0)
            .strong()
            .color(egui::Color32::from_rgb(255, 70, 70))); // Svijetla crvena
        ui.add_space(14.0);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(20.0, 0.0);
            ui.label(egui::RichText::new(format!("Dev Buy: {}", m.filtered_by_dev_buy))
                .size(13.0)
                .color(egui::Color32::from_rgb(240, 240, 245)));
            ui.label(egui::RichText::new(format!("Socials: {}", m.filtered_by_socials))
                .size(13.0)
                .color(egui::Color32::from_rgb(240, 240, 245)));
            ui.label(egui::RichText::new(format!("Creator Count: {}", m.filtered_by_creator_count))
                .size(13.0)
                .color(egui::Color32::from_rgb(240, 240, 245)));
            ui.label(egui::RichText::new(format!("Validation: {}", m.filtered_by_validation))
                .size(13.0)
                .color(egui::Color32::from_rgb(240, 240, 245)));
        });
    });
    
    ui.add_space(20.0);
    
    // Enhanced error breakdown with premium styling
    ui.group(|ui| {
        ui.heading(egui::RichText::new("⚠️ Error Breakdown")
            .size(19.0)
            .strong()
            .color(egui::Color32::from_rgb(255, 70, 70))); // Svijetla crvena
        ui.add_space(14.0);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(20.0, 0.0);
            ui.label(egui::RichText::new(format!("Network: {}", m.network_errors))
                .size(13.0)
                .color(egui::Color32::from_rgb(240, 240, 245)));
            ui.label(egui::RichText::new(format!("RPC: {}", m.rpc_errors))
                .size(13.0)
                .color(egui::Color32::from_rgb(240, 240, 245)));
            ui.label(egui::RichText::new(format!("Validation: {}", m.validation_errors))
                .size(13.0)
                .color(egui::Color32::from_rgb(240, 240, 245)));
            ui.label(egui::RichText::new(format!("Submission: {}", m.submission_errors))
                .size(13.0)
                .color(egui::Color32::from_rgb(240, 240, 245)));
            ui.label(egui::RichText::new(format!("Timeout: {}", m.timeout_errors))
                .size(13.0)
                .color(egui::Color32::from_rgb(240, 240, 245)));
        });
    });
    }); // End ScrollArea
}

