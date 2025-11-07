// seen_tokens.rs - DUPLICATE PROTECTION

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
}