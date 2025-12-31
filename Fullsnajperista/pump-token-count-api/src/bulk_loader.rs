// bulk_loader.rs - Bulk load creator counts from JSON/CSV file into Redis
use anyhow::Result;
use redis::aio::ConnectionManager;
use serde::Deserialize;
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

const REDIS_KEY_PREFIX: &str = "pump:creates:";
const REDIS_TTL_SECONDS: u64 = 30 * 24 * 60 * 60; // 30 days

#[derive(Debug, Deserialize)]
struct CreatorCount {
    creator: String,
    count: u64,
}

#[derive(Debug, Deserialize)]
struct TokenEntry {
    creator_address: Option<String>,
    creator: Option<String>,
    #[serde(flatten)]
    other: serde_json::Value,
}

/// Load bulk data from JSON file (array of {creator, count} or array of tokens)
pub async fn load_from_json_file(
    file_path: &Path,
    redis: &mut ConnectionManager,
) -> Result<usize> {
    let file = File::open(file_path)?;
    let reader = BufReader::new(file);
    let data: serde_json::Value = serde_json::from_reader(reader)?;

    let creator_counts: HashMap<String, u64> = if let Some(array) = data.as_array() {
        // Try to parse as array of {creator, count}
        if let Some(first) = array.first() {
            if first.get("creator").is_some() && first.get("count").is_some() {
                // Format: [{"creator": "...", "count": 123}, ...]
                let counts: Vec<CreatorCount> = serde_json::from_value(data.clone())?;
                counts
                    .into_iter()
                    .map(|c| (c.creator, c.count))
                    .collect()
            } else if first.get("creator_address").is_some() || first.get("creator").is_some() {
                // Format: [{"creator_address": "...", ...}, ...] - group by creator
                let tokens: Vec<TokenEntry> = serde_json::from_value(data.clone())?;
                let mut counts: HashMap<String, u64> = HashMap::new();
                for token in tokens {
                    let creator = token.creator_address.or(token.creator);
                    if let Some(creator) = creator {
                        *counts.entry(creator).or_insert(0) += 1;
                    }
                }
                counts
            } else {
                return Err(anyhow::anyhow!("Unknown JSON format"));
            }
        } else {
            return Err(anyhow::anyhow!("Empty array"));
        }
    } else {
        return Err(anyhow::anyhow!("Expected JSON array"));
    };

    tracing::info!("Loaded {} creators from JSON file", creator_counts.len());

    // Bulk write to Redis
    bulk_write_to_redis(creator_counts, redis).await
}

/// Load bulk data from CSV file (creator,count format)
pub async fn load_from_csv_file(
    file_path: &Path,
    redis: &mut ConnectionManager,
) -> Result<usize> {
    let file = File::open(file_path)?;
    let reader = BufReader::new(file);
    let mut creator_counts: HashMap<String, u64> = HashMap::new();

    for (line_num, line) in reader.lines().enumerate() {
        let line = line?;
        if line.trim().is_empty() || line.starts_with('#') {
            continue; // Skip empty lines and comments
        }

        let parts: Vec<&str> = line.split(',').map(|s| s.trim()).collect();
        if parts.len() >= 2 {
            let creator = parts[0].to_string();
            let count: u64 = parts[1].parse().unwrap_or(0);
            creator_counts.insert(creator, count);
        } else {
            tracing::warn!("Skipping invalid CSV line {}: {}", line_num + 1, line);
        }
    }

    tracing::info!("Loaded {} creators from CSV file", creator_counts.len());

    // Bulk write to Redis
    bulk_write_to_redis(creator_counts, redis).await
}

/// Bulk write creator counts to Redis using pipeline
pub async fn bulk_write_to_redis(
    creator_counts: HashMap<String, u64>,
    redis: &mut ConnectionManager,
) -> Result<usize> {
    const CHUNK_SIZE: usize = 1000;
    let mut total_written = 0;
    let mut chunk = Vec::new();

    for (creator, count) in creator_counts.iter() {
        let redis_key = format!("{}{}", REDIS_KEY_PREFIX, creator);
        chunk.push((redis_key, *count));

        if chunk.len() >= CHUNK_SIZE {
            write_chunk(&chunk, redis).await?;
            total_written += chunk.len();
            chunk.clear();
            tracing::debug!("Written {} creators to Redis...", total_written);
        }
    }

    // Write remaining chunk
    if !chunk.is_empty() {
        write_chunk(&chunk, redis).await?;
        total_written += chunk.len();
    }

    tracing::info!("Bulk write completed: {} creators written to Redis", total_written);
    Ok(total_written)
}

/// Write chunk to Redis using pipeline
async fn write_chunk(
    chunk: &[(String, u64)],
    redis: &mut ConnectionManager,
) -> Result<()> {
    let mut pipe = redis::pipe();

    for (key, count) in chunk.iter() {
        pipe.set(key, *count)
            .expire(key, REDIS_TTL_SECONDS as i64);
    }

    let _: () = pipe.query_async(redis).await?;
    Ok(())
}

