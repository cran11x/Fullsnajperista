// bin/extract_creators_from_tx_csv.rs - Extract creator counts from pump.fun transaction CSV
use anyhow::Result;
use redis::aio::ConnectionManager;
use std::collections::HashMap;
use std::env;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;

mod bulk_loader {
    include!("../bulk_loader.rs");
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .init();

    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: extract_creators_from_tx_csv <csv_file> [--output json|csv|redis]");
        eprintln!("\nExtracts creator counts from pump.fun transaction CSV file.");
        eprintln!("CSV format: block_time,slot,tx_idx,signing_wallet,direction,base_coin,...");
        eprintln!("\nOptions:");
        eprintln!("  --output json    Output as JSON file (default)");
        eprintln!("  --output csv     Output as CSV file");
        eprintln!("  --output redis   Load directly into Redis");
        std::process::exit(1);
    }

    let file_path = PathBuf::from(&args[1]);
    let output_format = args
        .iter()
        .position(|a| a == "--output")
        .and_then(|i| args.get(i + 1))
        .map(|s| s.as_str())
        .unwrap_or("json");

    if !file_path.exists() {
        eprintln!("Error: File not found: {:?}", file_path);
        std::process::exit(1);
    }

    tracing::info!("Reading CSV file: {:?}", file_path);

    // Step 1: Parse CSV and group by base_coin (mint address)
    let file_path_clone = file_path.clone();
    let mint_to_creator = parse_csv_and_get_creators(&file_path_clone).await?;

    tracing::info!("Found {} unique mints", mint_to_creator.len());

    // Step 2: Re-parse CSV to count transactions per mint
    let file = File::open(&file_path)?;
    let reader = BufReader::new(file);
    let mut lines = reader.lines();
    if let Some(Ok(_)) = lines.next() {} // Skip header

    let mut mint_counts: HashMap<String, u64> = HashMap::new();
    for (line_num, line) in lines.enumerate() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let parts: Vec<&str> = line.split(',').collect();
        if parts.len() >= 6 {
            let mint = parts[5].trim().to_string();
            if !mint.is_empty() && !mint.contains("base_coin") {
                *mint_counts.entry(mint).or_insert(0) += 1;
            }
        }
        if (line_num + 1) % 100000 == 0 {
            tracing::info!("Re-parsing CSV: processed {} lines", line_num + 1);
        }
    }

    // Step 3: Map mints to creators and sum counts
    let mut creator_counts: HashMap<String, u64> = HashMap::new();
    for (mint, count) in mint_counts.iter() {
        if let Some(creator) = mint_to_creator.get(mint) {
            *creator_counts.entry(creator.clone()).or_insert(0) += count;
        } else {
            tracing::warn!("No creator found for mint: {} ({} transactions)", mint, count);
        }
    }

    tracing::info!("Found {} unique creators", creator_counts.len());

    // Step 3: Output in requested format
    match output_format {
        "json" => {
            let output_path = file_path.with_extension("creators.json");
            output_json(&creator_counts, &output_path)?;
            tracing::info!("Output written to: {:?}", output_path);
        }
        "csv" => {
            let output_path = file_path.with_extension("creators.csv");
            output_csv(&creator_counts, &output_path)?;
            tracing::info!("Output written to: {:?}", output_path);
        }
        "redis" => {
            let redis_url = env::var("REDIS_URL")
                .unwrap_or_else(|_| "redis://localhost:6379".to_string());
            let redis_client = redis::Client::open(redis_url)?;
            let mut redis: ConnectionManager = redis_client.get_connection_manager().await?;
            let count = bulk_loader::bulk_write_to_redis(creator_counts, &mut redis).await?;
            tracing::info!("Loaded {} creators into Redis", count);
        }
        _ => {
            eprintln!("Error: Unknown output format '{}'. Use 'json', 'csv', or 'redis'", output_format);
            std::process::exit(1);
        }
    }

    Ok(())
}

/// Parse CSV and get creator for each mint address
async fn parse_csv_and_get_creators(file_path: &PathBuf) -> Result<HashMap<String, String>> {
    let file = File::open(file_path)?;
    let reader = BufReader::new(file);
    let mut lines = reader.lines();

    // Skip header
    if let Some(Ok(header)) = lines.next() {
        tracing::debug!("CSV header: {}", header);
    }

    // Get unique mint addresses (base_coin column, index 5)
    // Also track count per mint for faster processing
    let mut mint_counts: HashMap<String, u64> = HashMap::new();

    for (line_num, line) in lines.enumerate() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }

        // Parse CSV - handle quoted fields
        let parts: Vec<&str> = line.split(',').collect();
        if parts.len() >= 6 {
            let mint = parts[5].trim().to_string();
            if !mint.is_empty() && !mint.contains("base_coin") {
                // Count occurrences per mint (each row = 1 token transaction)
                *mint_counts.entry(mint).or_insert(0) += 1;
            }
        }

        if (line_num + 1) % 100000 == 0 {
            tracing::info!("Processed {} lines, found {} unique mints", line_num + 1, mint_counts.len());
        }
    }

    let unique_mints: Vec<String> = mint_counts.keys().cloned().collect();

    tracing::info!("Total unique mints: {}", unique_mints.len());

    // Get creator for each mint using DAS API
    let api_key = env::var("HELIUS_API_KEY")
        .expect("HELIUS_API_KEY must be set");

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()?;

    let mut mint_to_creator: HashMap<String, String> = HashMap::new();
    let mut processed = 0;

    for mint in unique_mints.iter() {
        match get_creator_from_mint(mint, &api_key, &client).await {
            Ok(Some(creator)) => {
                mint_to_creator.insert(mint.clone(), creator);
            }
            Ok(None) => {
                tracing::warn!("No creator found for mint: {}", mint);
            }
            Err(e) => {
                tracing::warn!("Error getting creator for {}: {}", mint, e);
            }
        }

        processed += 1;
        if processed % 100 == 0 {
            tracing::info!("Processed {}/{} mints", processed, unique_mints.len());
        }

        // Rate limiting - small delay
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }

    Ok(mint_to_creator)
}

/// Get creator address from mint using DAS API
async fn get_creator_from_mint(
    mint: &str,
    api_key: &str,
    client: &reqwest::Client,
) -> Result<Option<String>> {
    let url = format!("https://mainnet.helius-rpc.com/?api-key={}", api_key);

    // Try getAsset to get creator
    let body = serde_json::json!({
        "jsonrpc": "2.0",
        "id": "1",
        "method": "getAsset",
        "params": {
            "id": mint
        }
    });

    let response = client.post(&url).json(&body).send().await?;
    let json: serde_json::Value = response.json().await?;

    if let Some(error) = json.get("error") {
        tracing::debug!("DAS API error for {}: {:?}", mint, error);
        return Ok(None);
    }

        // Try to extract creator from result
        if let Some(result) = json.get("result") {
            // Try creators array - prefer verified creator
            if let Some(creators) = result.get("creators").and_then(|c| c.as_array()) {
                let mut verified_creator = None;
                let mut first_creator = None;

                for creator in creators {
                    if let Some(address) = creator.get("address").and_then(|a| a.as_str()) {
                        if creator.get("verified")
                            .and_then(|v| v.as_bool())
                            .unwrap_or(false)
                        {
                            verified_creator = Some(address.to_string());
                            break; // Prefer verified creator
                        }
                        if first_creator.is_none() {
                            first_creator = Some(address.to_string());
                        }
                    }
                }

                if let Some(creator) = verified_creator.or(first_creator) {
                    return Ok(Some(creator));
                }
            }

        // Try authority
        if let Some(authority) = result.get("authority").and_then(|a| a.as_str()) {
            return Ok(Some(authority.to_string()));
        }
    }

    Ok(None)
}

/// Output creator counts as JSON
fn output_json(creator_counts: &HashMap<String, u64>, output_path: &PathBuf) -> Result<()> {
    use std::io::Write;
    let mut file = File::create(output_path)?;
    let mut entries: Vec<_> = creator_counts.iter().collect();
    entries.sort_by_key(|(_, count)| std::cmp::Reverse(**count));

    let json_array: Vec<serde_json::Value> = entries
        .iter()
        .map(|(creator, count)| {
            serde_json::json!({
                "creator": creator,
                "count": count
            })
        })
        .collect();

    serde_json::to_writer_pretty(&mut file, &json_array)?;
    file.flush()?;
    Ok(())
}

/// Output creator counts as CSV
fn output_csv(creator_counts: &HashMap<String, u64>, output_path: &PathBuf) -> Result<()> {
    use std::io::Write;
    let mut file = File::create(output_path)?;
    writeln!(file, "creator,count")?;

    let mut entries: Vec<_> = creator_counts.iter().collect();
    entries.sort_by_key(|(_, count)| std::cmp::Reverse(**count));

    for (creator, count) in entries {
        writeln!(file, "{},{}", creator, count)?;
    }

    file.flush()?;
    Ok(())
}

