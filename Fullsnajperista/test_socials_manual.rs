// Quick test for socials pull
use std::env;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let api_key = env::var("HELIUS_API_KEY")
        .unwrap_or_else(|_| "7ef7af02-aa9d-4f5c-9c98-d5fa303d1f04".to_string());

    println!("🧪 Testing Socials Pull with Real Mint Addresses\n");

    let test_mints = vec![
        "4CvPL8T69MEWcRC8qXegZq9GrhVAGyja1L7JbQ3mpump",
        "AkoUu6Zh9aA9R4tyxs9vVQK7DUf7EB4HgmmxaGqvpump",
        "DDeroySR8s8wNJMnXB39BpCf35Lq4ipPTSXiD7dCpump",
        "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v", // USDC
    ];

    for (i, mint) in test_mints.iter().enumerate() {
        println!("Test {}: {}", i + 1, mint);
        let start = std::time::Instant::now();
        
        match SNIPER::socials::check_token_metadata(mint, &api_key).await {
            Ok((socials, metadata)) => {
                let elapsed = start.elapsed();
                println!("  ✅ {}ms - Socials: {}, Name: {}, Symbol: {}", 
                    elapsed.as_millis(),
                    socials.count(),
                    metadata.name,
                    metadata.symbol);
                if socials.has_any() {
                    if let Some(tw) = &socials.twitter { println!("     Twitter: {}", tw); }
                    if let Some(web) = &socials.website { println!("     Website: {}", web); }
                    if let Some(tg) = &socials.telegram { println!("     Telegram: {}", tg); }
                    if let Some(dc) = &socials.discord { println!("     Discord: {}", dc); }
                }
            }
            Err(e) => println!("  ❌ Error: {}", e),
        }
        println!();
    }
    
    Ok(())
}
