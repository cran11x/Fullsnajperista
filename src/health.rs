// health.rs - CONNECTION HEALTH MONITORING
#![allow(unused, dead_code)]

use anyhow::Result;
use solana_client::nonblocking::rpc_client::RpcClient;
use std::time::{Duration, Instant};

/// Health check timeout
const HEALTH_CHECK_TIMEOUT_SECS: u64 = 5;

/// Maximum consecutive failures before marking as unhealthy
const MAX_CONSECUTIVE_FAILURES: u32 = 3;

/// Health status
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HealthStatus {
    Healthy,
    Unhealthy,
    Unknown,
}

/// Health monitor for RPC and WebSocket connections
pub struct HealthMonitor {
    rpc_status: HealthStatus,
    ws_status: HealthStatus,
    consecutive_rpc_failures: u32,
    consecutive_ws_failures: u32,
    last_rpc_check: Option<Instant>,
    last_ws_check: Option<Instant>,
}

impl HealthMonitor {
    pub fn new() -> Self {
        Self {
            rpc_status: HealthStatus::Unknown,
            ws_status: HealthStatus::Unknown,
            consecutive_rpc_failures: 0,
            consecutive_ws_failures: 0,
            last_rpc_check: None,
            last_ws_check: None,
        }
    }

    /// Check RPC endpoint health
    pub async fn check_rpc(&mut self, rpc: &RpcClient) -> Result<bool> {
        match tokio::time::timeout(
            Duration::from_secs(HEALTH_CHECK_TIMEOUT_SECS),
            rpc.get_slot()
        ).await {
            Ok(Ok(_)) => {
                self.rpc_status = HealthStatus::Healthy;
                self.consecutive_rpc_failures = 0;
                self.last_rpc_check = Some(Instant::now());
                Ok(true)
            }
            Ok(Err(e)) => {
                self.consecutive_rpc_failures += 1;
                if self.consecutive_rpc_failures >= MAX_CONSECUTIVE_FAILURES {
                    self.rpc_status = HealthStatus::Unhealthy;
                }
                self.last_rpc_check = Some(Instant::now());
                Err(anyhow::anyhow!("RPC health check failed: {}", e))
            }
            Err(_) => {
                self.consecutive_rpc_failures += 1;
                if self.consecutive_rpc_failures >= MAX_CONSECUTIVE_FAILURES {
                    self.rpc_status = HealthStatus::Unhealthy;
                }
                self.last_rpc_check = Some(Instant::now());
                Err(anyhow::anyhow!("RPC health check timeout"))
            }
        }
    }

    /// Check WebSocket connection health (simple ping)
    pub fn check_websocket(&mut self, is_connected: bool) -> bool {
        if is_connected {
            self.ws_status = HealthStatus::Healthy;
            self.consecutive_ws_failures = 0;
            self.last_ws_check = Some(Instant::now());
            true
        } else {
            self.consecutive_ws_failures += 1;
            if self.consecutive_ws_failures >= MAX_CONSECUTIVE_FAILURES {
                self.ws_status = HealthStatus::Unhealthy;
            }
            self.last_ws_check = Some(Instant::now());
            false
        }
    }

    /// Get RPC health status
    pub fn rpc_status(&self) -> HealthStatus {
        self.rpc_status
    }

    /// Get WebSocket health status
    pub fn ws_status(&self) -> HealthStatus {
        self.ws_status
    }

    /// Check if RPC is healthy
    pub fn is_rpc_healthy(&self) -> bool {
        self.rpc_status == HealthStatus::Healthy
    }

    /// Check if WebSocket is healthy
    pub fn is_ws_healthy(&self) -> bool {
        self.ws_status == HealthStatus::Healthy
    }

    /// Get time since last RPC check
    pub fn time_since_rpc_check(&self) -> Option<Duration> {
        self.last_rpc_check.map(|t| t.elapsed())
    }

    /// Get time since last WS check
    pub fn time_since_ws_check(&self) -> Option<Duration> {
        self.last_ws_check.map(|t| t.elapsed())
    }
}

impl Default for HealthMonitor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_health_monitor_creation() {
        let monitor = HealthMonitor::new();
        assert_eq!(monitor.rpc_status(), HealthStatus::Unknown);
        assert_eq!(monitor.ws_status(), HealthStatus::Unknown);
    }

    #[test]
    fn test_websocket_health_check() {
        let mut monitor = HealthMonitor::new();
        
        assert!(monitor.check_websocket(true));
        assert_eq!(monitor.ws_status(), HealthStatus::Healthy);
        assert_eq!(monitor.consecutive_ws_failures, 0);
        
        assert!(!monitor.check_websocket(false));
        assert_eq!(monitor.consecutive_ws_failures, 1);
    }

    #[test]
    fn test_consecutive_failures() {
        let mut monitor = HealthMonitor::new();
        
        // Fail 3 times
        monitor.check_websocket(false);
        monitor.check_websocket(false);
        monitor.check_websocket(false);
        
        assert_eq!(monitor.ws_status(), HealthStatus::Unhealthy);
    }

    #[test]
    fn test_health_recovery() {
        let mut monitor = HealthMonitor::new();
        
        // Fail then recover
        monitor.check_websocket(false);
        assert_eq!(monitor.consecutive_ws_failures, 1);
        
        monitor.check_websocket(true);
        assert_eq!(monitor.ws_status(), HealthStatus::Healthy);
        assert_eq!(monitor.consecutive_ws_failures, 0);
    }
}

