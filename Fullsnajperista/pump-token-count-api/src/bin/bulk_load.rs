// bin/bulk_load.rs - CLI tool for bulk loading creator counts
use anyhow::Result;
use redis::aio::ConnectionManager;
use std::env;
use std::path::PathBuf;

// Import bulk_loader module
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
        eprintln!("Usage: bulk_load <file_path> [--format json|csv]");
        eprintln!("\nExamples:");
        eprintln!("  bulk_load data.json");
        eprintln!("  bulk_load data.csv --format csv");
        eprintln!("  bulk_load data.json --format json");
        std::process::exit(1);
    }

    let file_path = PathBuf::from(&args[1]);
    let format = args
        .iter()
        .position(|a| a == "--format")
        .and_then(|i| args.get(i + 1))
        .map(|s| s.as_str())
        .unwrap_or_else(|| {
            // Auto-detect from extension
            file_path
                .extension()
                .and_then(|ext| ext.to_str())
                .unwrap_or("json")
        });

    if !file_path.exists() {
        eprintln!("Error: File not found: {:?}", file_path);
        std::process::exit(1);
    }

    // Load Redis URL from env
    let redis_url = env::var("REDIS_URL")
        .unwrap_or_else(|_| "redis://localhost:6379".to_string());

    tracing::info!("Connecting to Redis at: {}", redis_url);
    let redis_client = redis::Client::open(redis_url)?;
    let mut redis: ConnectionManager = redis_client.get_connection_manager().await?;
    tracing::info!("Redis connected");

    tracing::info!("Loading data from: {:?} (format: {})", file_path, format);

    let start = std::time::Instant::now();
    let count = match format {
        "json" => bulk_loader::load_from_json_file(&file_path, &mut redis).await?,
        "csv" => bulk_loader::load_from_csv_file(&file_path, &mut redis).await?,
        _ => {
            eprintln!("Error: Unknown format '{}'. Use 'json' or 'csv'", format);
            std::process::exit(1);
        }
    };

    let elapsed = start.elapsed();
    tracing::info!(
        "Successfully loaded {} creators in {:?}",
        count,
        elapsed
    );

    Ok(())
}

