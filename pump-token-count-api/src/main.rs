// main.rs - Axum server with background workers
use anyhow::Result;
use axum::{
    extract::Path,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
    Router,
};
use redis::aio::ConnectionManager;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::signal;

mod bulk_loader;
mod fallback;
#[allow(dead_code)] // lib.rs is used for public API
mod lib;
mod refresh;
mod ws_listener;

pub use bulk_loader;

use lib::{cache_count, get_creator_count_from_redis};
use refresh::start_refresh_task;
use ws_listener::{get_last_update, start_listener};

#[derive(serde::Serialize)]
struct HealthResponse {
    ws_connected: bool,
    last_update_seconds_ago: u64,
}

#[derive(serde::Serialize)]
struct StatsResponse {
    redis_keys: Option<u64>,
    memory_usage: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .init();

    // Load environment variables
    dotenv::dotenv().ok();

    let helius_api_key = std::env::var("HELIUS_API_KEY")
        .expect("HELIUS_API_KEY must be set");
    let redis_url = std::env::var("REDIS_URL")
        .unwrap_or_else(|_| "redis://localhost:6379".to_string());
    let apify_dataset_id = std::env::var("APIFY_DATASET_ID")
        .expect("APIFY_DATASET_ID must be set");
    let apify_api_key = std::env::var("APIFY_API_KEY").ok();
    let port = std::env::var("PORT")
        .unwrap_or_else(|_| "3000".to_string())
        .parse::<u16>()
        .expect("PORT must be a valid number");

    tracing::info!("Connecting to Redis at: {}", redis_url);

    // Create Redis connection manager (connection pool)
    let redis_client = redis::Client::open(redis_url)?;
    let redis_pool: Arc<ConnectionManager> = Arc::new(
        redis_client
            .get_connection_manager()
            .await?,
    );

    tracing::info!("Redis connected");

    // Clone for background tasks
    let redis_pool_ws = Arc::clone(&redis_pool);
    let redis_pool_refresh = Arc::clone(&redis_pool);
    let redis_pool_api = Arc::clone(&redis_pool);
    let helius_api_key_ws = helius_api_key.clone();
    let helius_api_key_api = helius_api_key.clone();

    // Spawn WebSocket listener task
    tokio::spawn(async move {
        if let Err(e) = start_listener(helius_api_key_ws, redis_pool_ws).await {
            tracing::error!("WebSocket listener error: {}", e);
        }
    });

    // Spawn refresh task
    tokio::spawn(async move {
        if let Err(e) = start_refresh_task(apify_dataset_id, apify_api_key, redis_pool_refresh).await {
            tracing::error!("Refresh task error: {}", e);
        }
    });

    // Create Axum router
    let app = Router::new()
        .route("/count/:address", get(get_count_handler))
        .route("/health", get(health_handler))
        .route("/stats", get(stats_handler))
        .with_state((redis_pool_api, helius_api_key_api));

    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", port))
        .await?;
    tracing::info!("Server listening on 0.0.0.0:{}", port);

    // Graceful shutdown
    let server = axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal());

    server.await?;

    Ok(())
}

/// GET /count/:address - Returns plain text number
async fn get_count_handler(
    Path(address): Path<String>,
    axum::extract::State((redis_pool, api_key)): axum::extract::State<(
        Arc<ConnectionManager>,
        String,
    )>,
) -> Response {
    let start = std::time::Instant::now();
    let mut redis = (*redis_pool).clone();

    // Try Redis first (fast path - < 15ms)
    match get_creator_count_from_redis(&address, &mut redis).await {
        Ok(Some(count)) => {
            let elapsed = start.elapsed();
            if elapsed.as_millis() > 15 {
                tracing::warn!(
                    "Slow response: {}ms for address {}",
                    elapsed.as_millis(),
                    address
                );
            }
            return (StatusCode::OK, count.to_string()).into_response();
        }
        Ok(None) => {
            // Redis miss - use fallback API (CRITICAL FIX #3: Enhanced Transactions API first, then DAS)
            match fallback::get_count_via_api(&address, &api_key).await {
                Ok(count) => {
                    // Cache result with TTL (CRITICAL FIX #4: TTL on fallback cache)
                    if let Err(e) = cache_count(&address, count, &mut redis).await {
                        tracing::warn!("Failed to cache count: {}", e);
                    }
                    let elapsed = start.elapsed();
                    if elapsed.as_millis() > 15 {
                        tracing::warn!(
                            "Slow response (fallback): {}ms for address {}",
                            elapsed.as_millis(),
                            address
                        );
                    }
                    (StatusCode::OK, count.to_string()).into_response()
                }
                Err(e) => {
                    tracing::error!("Error getting count for {}: {}", address, e);
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        format!("Error: {}", e),
                    )
                        .into_response()
                }
            }
        }
        Err(e) => {
            tracing::error!("Redis error for {}: {}", address, e);
            // Try fallback even if Redis fails
            match fallback::get_count_via_api(&address, &api_key).await {
                Ok(count) => (StatusCode::OK, count.to_string()).into_response(),
                Err(e2) => {
                    tracing::error!("Fallback also failed for {}: {}", address, e2);
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        format!("Error: {}", e2),
                    )
                        .into_response()
                }
            }
        }
    }
}

/// GET /health - Returns WebSocket connection status and last update time
async fn health_handler(
    _: axum::extract::State<(Arc<ConnectionManager>, String)>,
) -> impl IntoResponse {
    let last_update = get_last_update();
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let last_update_seconds_ago = if last_update > 0 {
        now.saturating_sub(last_update)
    } else {
        0
    };

    // Consider WS connected if we got an update in last 5 minutes
    let ws_connected = last_update_seconds_ago < 300;

    let response = HealthResponse {
        ws_connected,
        last_update_seconds_ago,
    };

    (StatusCode::OK, axum::Json(response))
}

/// GET /stats - Returns Redis statistics
async fn stats_handler(
    axum::extract::State((redis_pool, _)): axum::extract::State<(Arc<ConnectionManager>, String)>,
) -> impl IntoResponse {
    let mut redis = (*redis_pool).clone();

    // Get Redis INFO
    let info: Option<String> = redis::cmd("INFO")
        .arg("memory")
        .query_async(&mut redis)
        .await
        .ok();

    let memory_usage = info.and_then(|info| {
        info.lines()
            .find(|line| line.starts_with("used_memory_human:"))
            .map(|line| line.split(':').nth(1).unwrap_or("").trim().to_string())
    });

    // Count keys with pattern (approximate)
    let keys_count: Option<u64> = redis::cmd("DBSIZE")
        .query_async(&mut redis)
        .await
        .ok();

    let response = StatsResponse {
        redis_keys: keys_count,
        memory_usage,
    };

    (StatusCode::OK, axum::Json(response))
}

/// Graceful shutdown signal handler
async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }

    tracing::info!("Shutting down gracefully...");
}

