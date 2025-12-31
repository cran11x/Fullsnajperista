// errors.rs - ERROR CATEGORIZATION AND HANDLING
#![allow(unused, dead_code)]

use std::fmt;

/// Categorized bot errors for better error handling and reporting
#[derive(Debug, Clone)]
pub enum BotError {
    /// Network-related errors (connection failures, timeouts)
    NetworkError(String),
    /// RPC-related errors (Solana RPC failures)
    RpcError(String),
    /// Validation errors (pre-flight checks failed)
    ValidationError(String),
    /// Timeout errors (operation took too long)
    TimeoutError(String),
    /// Filter rejection (token didn't pass filters)
    FilterRejected { reason: String },
    /// Submission failure (transaction submission failed)
    SubmissionFailed { method: String, error: String },
}

impl fmt::Display for BotError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BotError::NetworkError(msg) => write!(f, "Network error: {}", msg),
            BotError::RpcError(msg) => write!(f, "RPC error: {}", msg),
            BotError::ValidationError(msg) => write!(f, "Validation error: {}", msg),
            BotError::TimeoutError(msg) => write!(f, "Timeout error: {}", msg),
            BotError::FilterRejected { reason } => write!(f, "Filter rejected: {}", reason),
            BotError::SubmissionFailed { method, error } => {
                write!(f, "Submission failed ({}): {}", method, error)
            }
        }
    }
}

impl std::error::Error for BotError {}

impl BotError {
    /// Check if error is retryable
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            BotError::NetworkError(_) | BotError::RpcError(_) | BotError::TimeoutError(_)
        )
    }

    /// Check if error should be logged
    pub fn should_log(&self) -> bool {
        !matches!(self, BotError::FilterRejected { .. })
    }

    /// Get error code for metrics/analytics
    pub fn error_code(&self) -> &'static str {
        match self {
            BotError::NetworkError(_) => "NETWORK",
            BotError::RpcError(_) => "RPC",
            BotError::ValidationError(_) => "VALIDATION",
            BotError::TimeoutError(_) => "TIMEOUT",
            BotError::FilterRejected { .. } => "FILTER",
            BotError::SubmissionFailed { .. } => "SUBMISSION",
        }
    }

    /// Get error category for metrics
    pub fn category(&self) -> ErrorCategory {
        match self {
            BotError::NetworkError(_) => ErrorCategory::Network,
            BotError::RpcError(_) => ErrorCategory::Rpc,
            BotError::ValidationError(_) => ErrorCategory::Validation,
            BotError::TimeoutError(_) => ErrorCategory::Timeout,
            BotError::FilterRejected { .. } => ErrorCategory::Filter,
            BotError::SubmissionFailed { .. } => ErrorCategory::Submission,
        }
    }
}

/// Error categories for metrics
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCategory {
    Network,
    Rpc,
    Validation,
    Timeout,
    Filter,
    Submission,
}

/// Convert from common error types
impl From<reqwest::Error> for BotError {
    fn from(err: reqwest::Error) -> Self {
        if err.is_timeout() {
            BotError::TimeoutError(err.to_string())
        } else if err.is_connect() {
            BotError::NetworkError(err.to_string())
        } else {
            BotError::NetworkError(err.to_string())
        }
    }
}

impl From<solana_client::client_error::ClientError> for BotError {
    fn from(err: solana_client::client_error::ClientError) -> Self {
        BotError::RpcError(err.to_string())
    }
}

impl From<anyhow::Error> for BotError {
    fn from(err: anyhow::Error) -> Self {
        let msg = err.to_string();
        if msg.contains("timeout") || msg.contains("Timeout") {
            BotError::TimeoutError(msg)
        } else if msg.contains("network") || msg.contains("Network") || msg.contains("connection") {
            BotError::NetworkError(msg)
        } else if msg.contains("SKIP") || msg.contains("Filter") {
            BotError::FilterRejected { reason: msg }
        } else {
            BotError::RpcError(msg)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let err = BotError::NetworkError("Connection failed".to_string());
        assert!(err.to_string().contains("Network error"));
        assert!(err.to_string().contains("Connection failed"));
    }

    #[test]
    fn test_is_retryable() {
        assert!(BotError::NetworkError("test".to_string()).is_retryable());
        assert!(BotError::RpcError("test".to_string()).is_retryable());
        assert!(BotError::TimeoutError("test".to_string()).is_retryable());
        assert!(!BotError::ValidationError("test".to_string()).is_retryable());
        assert!(!BotError::FilterRejected { reason: "test".to_string() }.is_retryable());
    }

    #[test]
    fn test_should_log() {
        assert!(BotError::NetworkError("test".to_string()).should_log());
        assert!(!BotError::FilterRejected { reason: "test".to_string() }.should_log());
    }

    #[test]
    fn test_error_code() {
        assert_eq!(BotError::NetworkError("test".to_string()).error_code(), "NETWORK");
        assert_eq!(BotError::RpcError("test".to_string()).error_code(), "RPC");
        assert_eq!(BotError::TimeoutError("test".to_string()).error_code(), "TIMEOUT");
        assert_eq!(BotError::FilterRejected { reason: "test".to_string() }.error_code(), "FILTER");
    }

    #[test]
    fn test_error_category() {
        assert_eq!(
            BotError::NetworkError("test".to_string()).category(),
            ErrorCategory::Network
        );
        assert_eq!(
            BotError::RpcError("test".to_string()).category(),
            ErrorCategory::Rpc
        );
    }

    #[test]
    fn test_from_anyhow() {
        let err: BotError = anyhow::anyhow!("timeout occurred").into();
        match err {
            BotError::TimeoutError(_) => {}
            _ => panic!("Expected TimeoutError"),
        }

        let err: BotError = anyhow::anyhow!("SKIP: Invalid token").into();
        match err {
            BotError::FilterRejected { .. } => {}
            _ => panic!("Expected FilterRejected"),
        }
    }
}
