// refresh.rs - Apify dataset refresh task (every 3 hours)
use anyhow::Result;
use redis::aio::ConnectionManager;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::time::{interval, Duration};

const REDIS_KEY_PREFIX: &str = "pump:creates:";
const REDIS_TTL_SECONDS: u64 = 30 * 24 * 60 * 60; // 30 days
const REFRESH_INTERVAL_HOURS: u64 = 3; // CRITICAL FIX #2: Every 3 hours (not 6)

/// Start refresh task that downloads Apify dataset and bulk loads to Redis
pub async fn start_refresh_task(
    dataset_id: String,
    api_key: Option<String>,
    redis_pool: Arc<ConnectionManager>,
) -> Result<()> {
    let mut interval_timer = interval(Duration::from_secs(REFRESH_INTERVAL_HOURS * 60 * 60));

    // Run immediately on startup
    if let Err(e) = refresh_dataset(&dataset_id, api_key.as_deref(), &redis_pool).await {
        tracing::error!("Initial refresh failed: {}", e);
    }

    // Then run every 3 hours
    loop {
        interval_timer.tick().await;

        tracing::info!("Starting Apify dataset refresh...");
        let start = std::time::Instant::now();

        match refresh_dataset(&dataset_id, api_key.as_deref(), &redis_pool).await {
            Ok(count) => {
                let elapsed = start.elapsed();
                tracing::info!(
                    "Refresh completed: {} creators updated in {:?}",
                    count,
                    elapsed
                );
            }
            Err(e) => {
                tracing::error!("Refresh failed: {}", e);
            }
        }
    }
}

/// Download Apify dataset and bulk load to Redis
async fn refresh_dataset(
    dataset_id: &str,
    api_key: Option<&str>,
    redis: &ConnectionManager,
) -> Result<usize> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(600)) // 10 minutes timeout
        .build()?;

    // Build URL with optional API key
    let mut url = format!(
        "https://api.apify.com/v2/datasets/{}/items?format=json",
        dataset_id
    );
    if let Some(key) = api_key {
        url = format!("{}&token={}", url, key);
    }

    tracing::info!("Downloading Apify dataset from: {}", url);

    // Download dataset
    let response = client.get(&url).send().await?;
    if !response.status().is_success() {
        return Err(anyhow::anyhow!(
            "Apify API failed: {}",
            response.status()
        ));
    }

    let tokens: Vec<serde_json::Value> = response.json().await?;
    tracing::info!("Downloaded {} tokens from Apify", tokens.len());

    // Group by creator_address and count
    let mut creator_counts: HashMap<String, u64> = HashMap::new();

    for token in tokens.iter() {
        if let Some(creator) = token["creator_address"].as_str() {
            *creator_counts.entry(creator.to_string()).or_insert(0) += 1;
        }
    }

    tracing::info!("Grouped into {} unique creators", creator_counts.len());

    // Bulk write to Redis using pipeline (chunked for performance)
    const CHUNK_SIZE: usize = 1000;
    let mut total_written = 0;
    let mut chunk = Vec::new();

    for (creator, count) in creator_counts.iter() {
        let redis_key = format!("{}{}", REDIS_KEY_PREFIX, creator);
        chunk.push((redis_key, *count));

        if chunk.len() >= CHUNK_SIZE {
            let mut redis_conn = redis.clone();
            write_chunk(&chunk, &mut redis_conn).await?;
            total_written += chunk.len();
            chunk.clear();
        }
    }

    // Write remaining chunk
    if !chunk.is_empty() {
        let mut redis_conn = redis.clone();
        write_chunk(&chunk, &mut redis_conn).await?;
        total_written += chunk.len();
    }

    Ok(total_written)
}

/// Write chunk to Redis using pipeline
async fn write_chunk(
    chunk: &[(String, u64)],
    redis: &mut ConnectionManager,
) -> Result<()> {
    let mut pipe = redis::pipe();

    for (key, count) in chunk.iter() {
        // Use SET instead of INCR to overwrite with latest count from Apify
        pipe.set(key, *count)
            .expire(key, REDIS_TTL_SECONDS as i64);
    }

    let _: () = pipe.query_async(redis).await?;
    Ok(())
}

