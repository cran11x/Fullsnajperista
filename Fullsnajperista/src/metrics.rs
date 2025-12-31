// metrics.rs - COMPREHENSIVE METRICS TRACKING FOR BOT PERFORMANCE
#![allow(unused_imports, dead_code, unused_variables)]

use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

/// Comprehensive bot metrics for tracking performance and success rates
#[derive(Debug, Clone)]
pub struct BotMetrics {
    // Detection and filtering
    pub total_detected: u32,
    pub total_filtered: u32,
    pub total_submitted: u32,
    pub total_successful: u32,
    pub total_failed: u32,

    // Per-method submission stats
    pub helius_success: u32,
    pub helius_failed: u32,
    pub jito_success: u32,
    pub jito_failed: u32,
    pub rpc_success: u32,
    pub rpc_failed: u32,

    // Timing metrics (in milliseconds)
    pub detection_times: Vec<u64>,
    pub filter_times: Vec<u64>,
    pub submission_times: Vec<u64>,
    pub total_processing_times: Vec<u64>,

    // Filter breakdown
    pub filtered_by_dev_buy: u32,
    pub filtered_by_socials: u32,
    pub filtered_by_creator_count: u32,
    pub filtered_by_validation: u32,

    // Error categorization
    pub network_errors: u32,
    pub rpc_errors: u32,
    pub validation_errors: u32,
    pub timeout_errors: u32,
    pub submission_errors: u32,

    // Slow operation tracking
    pub slow_operations: u32,
    pub slowest_operation_ms: u64,
}

impl Default for BotMetrics {
    fn default() -> Self {
        Self {
            total_detected: 0,
            total_filtered: 0,
            total_submitted: 0,
            total_successful: 0,
            total_failed: 0,
            helius_success: 0,
            helius_failed: 0,
            jito_success: 0,
            jito_failed: 0,
            rpc_success: 0,
            rpc_failed: 0,
            detection_times: Vec::new(),
            filter_times: Vec::new(),
            submission_times: Vec::new(),
            total_processing_times: Vec::new(),
            filtered_by_dev_buy: 0,
            filtered_by_socials: 0,
            filtered_by_creator_count: 0,
            filtered_by_validation: 0,
            network_errors: 0,
            rpc_errors: 0,
            validation_errors: 0,
            timeout_errors: 0,
            submission_errors: 0,
            slow_operations: 0,
            slowest_operation_ms: 0,
        }
    }
}

impl BotMetrics {
    /// Create new metrics instance
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a token detection
    pub fn record_detection(&mut self, detection_time_ms: u64) {
        self.total_detected += 1;
        self.detection_times.push(detection_time_ms);
    }

    /// Record a filter rejection with reason
    pub fn record_filter(&mut self, reason: FilterReason, filter_time_ms: u64) {
        self.total_filtered += 1;
        self.filter_times.push(filter_time_ms);
        
        match reason {
            FilterReason::DevBuy => self.filtered_by_dev_buy += 1,
            FilterReason::Socials => self.filtered_by_socials += 1,
            FilterReason::CreatorCount => self.filtered_by_creator_count += 1,
            FilterReason::Validation => self.filtered_by_validation += 1,
        }
    }

    /// Record a submission attempt
    pub fn record_submission(&mut self, method: SubmissionMethod, success: bool, submission_time_ms: u64) {
        self.total_submitted += 1;
        self.submission_times.push(submission_time_ms);
        
        if success {
            self.total_successful += 1;
            match method {
                SubmissionMethod::Helius => self.helius_success += 1,
                SubmissionMethod::Jito => self.jito_success += 1,
                SubmissionMethod::Rpc => self.rpc_success += 1,
            }
        } else {
            self.total_failed += 1;
            match method {
                SubmissionMethod::Helius => self.helius_failed += 1,
                SubmissionMethod::Jito => self.jito_failed += 1,
                SubmissionMethod::Rpc => self.rpc_failed += 1,
            }
        }
    }

    /// Record an error by category
    pub fn record_error(&mut self, error_type: ErrorType) {
        match error_type {
            ErrorType::Network => self.network_errors += 1,
            ErrorType::Rpc => self.rpc_errors += 1,
            ErrorType::Validation => self.validation_errors += 1,
            ErrorType::Timeout => self.timeout_errors += 1,
            ErrorType::Submission => self.submission_errors += 1,
        }
    }

    /// Record total processing time
    pub fn record_total_processing(&mut self, total_time_ms: u64) {
        self.total_processing_times.push(total_time_ms);
        
        // Track slow operations (>1000ms)
        if total_time_ms > 1000 {
            self.slow_operations += 1;
        }
        
        if total_time_ms > self.slowest_operation_ms {
            self.slowest_operation_ms = total_time_ms;
        }
    }

    /// Get average detection time
    pub fn avg_detection_time_ms(&self) -> f64 {
        if self.detection_times.is_empty() {
            return 0.0;
        }
        self.detection_times.iter().sum::<u64>() as f64 / self.detection_times.len() as f64
    }

    /// Get average filter time
    pub fn avg_filter_time_ms(&self) -> f64 {
        if self.filter_times.is_empty() {
            return 0.0;
        }
        self.filter_times.iter().sum::<u64>() as f64 / self.filter_times.len() as f64
    }

    /// Get average submission time
    pub fn avg_submission_time_ms(&self) -> f64 {
        if self.submission_times.is_empty() {
            return 0.0;
        }
        self.submission_times.iter().sum::<u64>() as f64 / self.submission_times.len() as f64
    }

    /// Get average total processing time
    pub fn avg_total_processing_time_ms(&self) -> f64 {
        if self.total_processing_times.is_empty() {
            return 0.0;
        }
        self.total_processing_times.iter().sum::<u64>() as f64 / self.total_processing_times.len() as f64
    }

    /// Calculate percentile (p50, p95, p99)
    pub fn percentile(&self, times: &[u64], p: f64) -> u64 {
        if times.is_empty() {
            return 0;
        }
        let mut sorted = times.to_vec();
        sorted.sort();
        let index = ((sorted.len() - 1) as f64 * p / 100.0).ceil() as usize;
        sorted[index.min(sorted.len() - 1)]
    }

    /// Get success rate (0.0 to 1.0)
    pub fn success_rate(&self) -> f64 {
        if self.total_submitted == 0 {
            return 0.0;
        }
        self.total_successful as f64 / self.total_submitted as f64
    }

    /// Get filter pass rate (0.0 to 1.0)
    pub fn filter_pass_rate(&self) -> f64 {
        if self.total_detected == 0 {
            return 0.0;
        }
        let passed = self.total_detected - self.total_filtered;
        passed as f64 / self.total_detected as f64
    }

    /// Reset all metrics
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// Get formatted stats string
    pub fn get_stats_string(&self) -> String {
        format!(
            "📊 METRICS:\n\
            Detected: {} | Filtered: {} | Submitted: {} | Success: {} | Failed: {}\n\
            Success Rate: {:.1}% | Filter Pass Rate: {:.1}%\n\
            Helius: {}/{} | Jito: {}/{} | RPC: {}/{}\n\
            Avg Times: Detection {:.1}ms | Filter {:.1}ms | Submission {:.1}ms | Total {:.1}ms\n\
            Slow Ops: {} | Slowest: {}ms\n\
            Errors: Network {} | RPC {} | Validation {} | Timeout {} | Submission {}",
            self.total_detected,
            self.total_filtered,
            self.total_submitted,
            self.total_successful,
            self.total_failed,
            self.success_rate() * 100.0,
            self.filter_pass_rate() * 100.0,
            self.helius_success,
            self.helius_success + self.helius_failed,
            self.jito_success,
            self.jito_success + self.jito_failed,
            self.rpc_success,
            self.rpc_success + self.rpc_failed,
            self.avg_detection_time_ms(),
            self.avg_filter_time_ms(),
            self.avg_submission_time_ms(),
            self.avg_total_processing_time_ms(),
            self.slow_operations,
            self.slowest_operation_ms,
            self.network_errors,
            self.rpc_errors,
            self.validation_errors,
            self.timeout_errors,
            self.submission_errors,
        )
    }
}

/// Filter rejection reasons
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterReason {
    DevBuy,
    Socials,
    CreatorCount,
    Validation,
}

/// Submission methods
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubmissionMethod {
    Helius,
    Jito,
    Rpc,
}

/// Error types for categorization
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorType {
    Network,
    Rpc,
    Validation,
    Timeout,
    Submission,
}

/// Thread-safe metrics wrapper
pub type SharedMetrics = Arc<RwLock<BotMetrics>>;

/// Create new shared metrics instance
pub fn new_shared_metrics() -> SharedMetrics {
    Arc::new(RwLock::new(BotMetrics::new()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_creation() {
        let metrics = BotMetrics::new();
        assert_eq!(metrics.total_detected, 0);
        assert_eq!(metrics.total_filtered, 0);
        assert_eq!(metrics.total_submitted, 0);
    }

    #[test]
    fn test_record_detection() {
        let mut metrics = BotMetrics::new();
        metrics.record_detection(50);
        assert_eq!(metrics.total_detected, 1);
        assert_eq!(metrics.detection_times.len(), 1);
        assert_eq!(metrics.detection_times[0], 50);
    }

    #[test]
    fn test_record_filter() {
        let mut metrics = BotMetrics::new();
        metrics.record_filter(FilterReason::DevBuy, 100);
        assert_eq!(metrics.total_filtered, 1);
        assert_eq!(metrics.filtered_by_dev_buy, 1);
        assert_eq!(metrics.filtered_by_socials, 0);
        
        metrics.record_filter(FilterReason::Socials, 150);
        assert_eq!(metrics.total_filtered, 2);
        assert_eq!(metrics.filtered_by_socials, 1);
    }

    #[test]
    fn test_record_submission() {
        let mut metrics = BotMetrics::new();
        metrics.record_submission(SubmissionMethod::Helius, true, 200);
        assert_eq!(metrics.total_submitted, 1);
        assert_eq!(metrics.total_successful, 1);
        assert_eq!(metrics.helius_success, 1);
        assert_eq!(metrics.helius_failed, 0);
        
        metrics.record_submission(SubmissionMethod::Helius, false, 300);
        assert_eq!(metrics.total_submitted, 2);
        assert_eq!(metrics.total_successful, 1);
        assert_eq!(metrics.total_failed, 1);
        assert_eq!(metrics.helius_failed, 1);
    }

    #[test]
    fn test_record_error() {
        let mut metrics = BotMetrics::new();
        metrics.record_error(ErrorType::Network);
        assert_eq!(metrics.network_errors, 1);
        
        metrics.record_error(ErrorType::Timeout);
        assert_eq!(metrics.timeout_errors, 1);
        assert_eq!(metrics.network_errors, 1);
    }

    #[test]
    fn test_avg_times() {
        let mut metrics = BotMetrics::new();
        metrics.record_detection(100);
        metrics.record_detection(200);
        metrics.record_detection(300);
        
        assert_eq!(metrics.avg_detection_time_ms(), 200.0);
        
        metrics.record_filter(FilterReason::DevBuy, 50);
        metrics.record_filter(FilterReason::Socials, 150);
        assert_eq!(metrics.avg_filter_time_ms(), 100.0);
    }

    #[test]
    fn test_success_rate() {
        let mut metrics = BotMetrics::new();
        assert_eq!(metrics.success_rate(), 0.0);
        
        metrics.record_submission(SubmissionMethod::Helius, true, 100);
        metrics.record_submission(SubmissionMethod::Jito, true, 200);
        metrics.record_submission(SubmissionMethod::Rpc, false, 300);
        
        assert!((metrics.success_rate() - 0.6666666666666666).abs() < 0.01);
    }

    #[test]
    fn test_filter_pass_rate() {
        let mut metrics = BotMetrics::new();
        assert_eq!(metrics.filter_pass_rate(), 0.0);
        
        metrics.record_detection(50);
        metrics.record_detection(60);
        metrics.record_filter(FilterReason::DevBuy, 100);
        
        assert!((metrics.filter_pass_rate() - 0.5).abs() < 0.01);
    }

    #[test]
    fn test_slow_operations() {
        let mut metrics = BotMetrics::new();
        metrics.record_total_processing(500);
        assert_eq!(metrics.slow_operations, 0);
        
        metrics.record_total_processing(1500);
        assert_eq!(metrics.slow_operations, 1);
        assert_eq!(metrics.slowest_operation_ms, 1500);
        
        metrics.record_total_processing(2000);
        assert_eq!(metrics.slow_operations, 2);
        assert_eq!(metrics.slowest_operation_ms, 2000);
    }

    #[test]
    fn test_percentile() {
        let mut metrics = BotMetrics::new();
        metrics.record_detection(10);
        metrics.record_detection(20);
        metrics.record_detection(30);
        metrics.record_detection(40);
        metrics.record_detection(50);
        
        let p50 = metrics.percentile(&metrics.detection_times, 50.0);
        assert_eq!(p50, 30);
        
        let p95 = metrics.percentile(&metrics.detection_times, 95.0);
        assert_eq!(p95, 50);
    }

    #[test]
    fn test_reset() {
        let mut metrics = BotMetrics::new();
        metrics.record_detection(100);
        metrics.record_filter(FilterReason::DevBuy, 50);
        metrics.record_submission(SubmissionMethod::Helius, true, 200);
        
        assert_eq!(metrics.total_detected, 1);
        assert_eq!(metrics.total_filtered, 1);
        assert_eq!(metrics.total_submitted, 1);
        
        metrics.reset();
        assert_eq!(metrics.total_detected, 0);
        assert_eq!(metrics.total_filtered, 0);
        assert_eq!(metrics.total_submitted, 0);
    }

    #[test]
    fn test_thread_safety() {
        use std::thread;
        
        let metrics = new_shared_metrics();
        let mut handles = vec![];
        
        // Spawn multiple threads that increment counters
        for _ in 0..10 {
            let metrics_clone = metrics.clone();
            let handle = thread::spawn(move || {
                for _ in 0..100 {
                    let mut m = metrics_clone.write().unwrap();
                    m.record_detection(50);
                    m.record_submission(SubmissionMethod::Helius, true, 100);
                }
            });
            handles.push(handle);
        }
        
        for handle in handles {
            handle.join().unwrap();
        }
        
        let m = metrics.read().unwrap();
        assert_eq!(m.total_detected, 1000);
        assert_eq!(m.total_submitted, 1000);
    }

    #[test]
    fn test_stats_string() {
        let mut metrics = BotMetrics::new();
        metrics.record_detection(100);
        metrics.record_submission(SubmissionMethod::Helius, true, 200);
        
        let stats = metrics.get_stats_string();
        assert!(stats.contains("Detected: 1"));
        assert!(stats.contains("Submitted: 1"));
        assert!(stats.contains("Success: 1"));
    }
}
