// rate_limiter.rs - SLIDING WINDOW RATE LIMITING
#![allow(unused, dead_code)]

use std::collections::VecDeque;
use std::time::{Duration, Instant};
use std::sync::{Arc, Mutex};

/// Rate limiter with sliding window
pub struct RateLimiter {
    requests: Arc<Mutex<VecDeque<Instant>>>,
    max_requests: u32,
    window_secs: u64,
}

impl RateLimiter {
    /// Create new rate limiter
    pub fn new(max_requests: u32, window_secs: u64) -> Self {
        Self {
            requests: Arc::new(Mutex::new(VecDeque::new())),
            max_requests,
            window_secs,
        }
    }

    /// Check if request is allowed
    pub fn check(&self) -> Result<(), RateLimitError> {
        let mut requests = self.requests.lock().unwrap();
        let now = Instant::now();
        let window_start = now - Duration::from_secs(self.window_secs);

        // Remove old requests outside the window
        while let Some(&front) = requests.front() {
            if front < window_start {
                requests.pop_front();
            } else {
                break;
            }
        }

        // Check if we're at the limit
        if requests.len() >= self.max_requests as usize {
            let oldest = requests.front().unwrap();
            let elapsed = oldest.elapsed().as_secs();
            let wait_time = self.window_secs.saturating_sub(elapsed);
            return Err(RateLimitError::RateLimited {
                wait_seconds: wait_time,
                max_requests: self.max_requests,
                window_secs: self.window_secs,
            });
        }

        // Add current request
        requests.push_back(now);
        Ok(())
    }

    /// Try to acquire permit (non-blocking)
    pub fn try_acquire(&self) -> Result<(), RateLimitError> {
        self.check()
    }

    /// Get current request count in window
    pub fn current_count(&self) -> usize {
        let mut requests = self.requests.lock().unwrap();
        let now = Instant::now();
        let window_start = now - Duration::from_secs(self.window_secs);

        // Clean old requests
        while let Some(&front) = requests.front() {
            if front < window_start {
                requests.pop_front();
            } else {
                break;
            }
        }

        requests.len()
    }

    /// Reset rate limiter
    pub fn reset(&self) {
        let mut requests = self.requests.lock().unwrap();
        requests.clear();
    }
}

/// Rate limit error
#[derive(Debug, Clone)]
pub enum RateLimitError {
    RateLimited {
        wait_seconds: u64,
        max_requests: u32,
        window_secs: u64,
    },
}

impl std::fmt::Display for RateLimitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RateLimitError::RateLimited {
                wait_seconds,
                max_requests,
                window_secs,
            } => {
                write!(
                    f,
                    "Rate limit exceeded: {}/{} requests in {}s window. Wait {}s",
                    max_requests, max_requests, window_secs, wait_seconds
                )
            }
        }
    }
}

impl std::error::Error for RateLimitError {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn test_rate_limiter_creation() {
        let limiter = RateLimiter::new(10, 60);
        assert_eq!(limiter.max_requests, 10);
        assert_eq!(limiter.window_secs, 60);
    }

    #[test]
    fn test_rate_limit_enforcement() {
        let limiter = RateLimiter::new(3, 60);
        
        // Should allow 3 requests
        assert!(limiter.check().is_ok());
        assert!(limiter.check().is_ok());
        assert!(limiter.check().is_ok());
        
        // 4th should fail
        assert!(limiter.check().is_err());
    }

    #[test]
    fn test_sliding_window() {
        let limiter = RateLimiter::new(2, 1); // 2 requests per second
        
        assert!(limiter.check().is_ok());
        assert!(limiter.check().is_ok());
        assert!(limiter.check().is_err());
        
        // Wait for window to slide
        thread::sleep(Duration::from_millis(1100));
        
        // Should allow again
        assert!(limiter.check().is_ok());
    }

    #[test]
    fn test_current_count() {
        let limiter = RateLimiter::new(5, 60);
        
        assert_eq!(limiter.current_count(), 0);
        
        limiter.check().unwrap();
        assert_eq!(limiter.current_count(), 1);
        
        limiter.check().unwrap();
        assert_eq!(limiter.current_count(), 2);
    }

    #[test]
    fn test_reset() {
        let limiter = RateLimiter::new(3, 60);
        
        limiter.check().unwrap();
        limiter.check().unwrap();
        assert_eq!(limiter.current_count(), 2);
        
        limiter.reset();
        assert_eq!(limiter.current_count(), 0);
    }
}

