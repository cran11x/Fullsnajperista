// seen_tokens.rs - DUPLICATE PROTECTION
#![allow(dead_code)]

use std::collections::HashSet;
use std::sync::{Arc, RwLock};

/// Thread-safe storage for tracking which tokens have been processed
#[derive(Clone)]
pub struct SeenTokens {
    mints: Arc<RwLock<HashSet<String>>>,
}

impl SeenTokens {
    /// Create new seen tokens tracker
    pub fn new() -> Self {
        Self {
            mints: Arc::new(RwLock::new(HashSet::new())),
        }
    }

    /// Check if token has been seen before
    /// Returns true if this is the FIRST time seeing this token
    pub fn check_and_mark(&self, mint: &str) -> bool {
        let mut set = self.mints.write().unwrap();

        // insert() returns false if value was already present
        let is_new = set.insert(mint.to_string());

        if !is_new {
            println!("      ⚠️  DUPLICATE DETECTED: {} - SKIPPING!", mint);
        }

        is_new
    }

    /// Check if token was seen (without marking)
    pub fn has_seen(&self, mint: &str) -> bool {
        let set = self.mints.read().unwrap();
        set.contains(mint)
    }

    /// Get total count of seen tokens
    pub fn count(&self) -> usize {
        let set = self.mints.read().unwrap();
        set.len()
    }

    /// Clear all seen tokens (for testing or reset)
    pub fn clear(&self) {
        let mut set = self.mints.write().unwrap();
        set.clear();
    }
}

impl Default for SeenTokens {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_duplicate_detection() {
        let seen = SeenTokens::new();

        // First time should return true
        assert!(seen.check_and_mark("token1"));

        // Second time should return false (duplicate)
        assert!(!seen.check_and_mark("token1"));

        // Different token should return true
        assert!(seen.check_and_mark("token2"));

        assert_eq!(seen.count(), 2);
    }

    #[test]
    fn test_has_seen() {
        let seen = SeenTokens::new();

        assert!(!seen.has_seen("token1"));
        seen.check_and_mark("token1");
        assert!(seen.has_seen("token1"));
    }

    #[test]
    fn test_clear() {
        let seen = SeenTokens::new();
        seen.check_and_mark("token1");
        seen.check_and_mark("token2");

        assert_eq!(seen.count(), 2);

        seen.clear();
        assert_eq!(seen.count(), 0);
    }

    #[test]
    fn test_thread_safety() {
        use std::sync::Arc;
        use std::thread;

        let seen = Arc::new(SeenTokens::new());
        let mut handles = vec![];

        // Spawn 10 threads, each trying to mark 100 tokens
        for thread_id in 0..10 {
            let seen_clone = seen.clone();
            let handle = thread::spawn(move || {
                for i in 0..100 {
                    let token = format!("token_{}_{}", thread_id, i);
                    seen_clone.check_and_mark(&token);
                }
            });
            handles.push(handle);
        }

        // Wait for all threads
        for handle in handles {
            handle.join().unwrap();
        }

        // Should have exactly 1000 unique tokens (10 threads * 100 tokens)
        assert_eq!(seen.count(), 1000);
    }

    #[test]
    fn test_concurrent_duplicate_detection() {
        use std::sync::Arc;
        use std::thread;

        let seen = Arc::new(SeenTokens::new());
        let mut handles = vec![];

        // All threads try to mark the same token
        for _ in 0..10 {
            let seen_clone = seen.clone();
            let handle = thread::spawn(move || {
                seen_clone.check_and_mark("same_token")
            });
            handles.push(handle);
        }

        // Wait for all threads
        let mut first_time_count = 0;
        for handle in handles {
            let was_first = handle.join().unwrap();
            if was_first {
                first_time_count += 1;
            }
        }

        // Only one thread should have seen it as new
        assert_eq!(first_time_count, 1);
        assert_eq!(seen.count(), 1);
    }

    #[test]
    fn test_has_seen_concurrent() {
        use std::sync::Arc;
        use std::thread;

        let seen = Arc::new(SeenTokens::new());
        seen.check_and_mark("token1");

        let mut handles = vec![];

        // Multiple threads check if token was seen
        for _ in 0..10 {
            let seen_clone = seen.clone();
            let handle = thread::spawn(move || {
                assert!(seen_clone.has_seen("token1"));
                assert!(!seen_clone.has_seen("token2"));
            });
            handles.push(handle);
        }

        for handle in handles {
            handle.join().unwrap();
        }
    }

    #[test]
    fn test_large_scale_operations() {
        let seen = SeenTokens::new();
        
        // Add many tokens
        for i in 0..10000 {
            seen.check_and_mark(&format!("token_{}", i));
        }

        assert_eq!(seen.count(), 10000);

        // Check duplicates
        assert!(!seen.check_and_mark("token_0"));
        assert!(!seen.check_and_mark("token_5000"));
        assert!(!seen.check_and_mark("token_9999"));

        // Check new token
        assert!(seen.check_and_mark("token_new"));
        assert_eq!(seen.count(), 10001);
    }
}