// main.rs - ULTRA PARALLEL: Social check + TX prep + Multi-submission

mod detection;
mod buy;
mod accounts;
mod jito;
mod helius;
mod socials;

use anyhow::{anyhow, Result};
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::{
    bs58,
    compute_budget::ComputeBudgetInstruction,
    message::{v0, VersionedMessage},
    pubkey::Pubkey,
    signature::{Keypair, Signer},
    transaction::VersionedTransaction,
    system_instruction,
};
use spl_associated_token_account::{get_associated_token_address, instruction::create_associated_token_account};
use std::str::FromStr;
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message as WsMessage};
use futures_util::StreamExt;
use rand::seq::SliceRandom;

use detection::PumpBuyAccounts;
use buy::build_buy_instruction;
use jito::send_jito_bundle;
use helius::send_helius_transaction;
use socials::{check_token_socials, Socials};

const RPC_URL: &str = "https://mainnet.helius-rpc.com/?api-key=7ef7af02-aa9d-4f5c-9c98-d5fa303d1f04";
const WSS_URL: &str = "wss://mainnet.helius-rpc.com/?api-key=7ef7af02-aa9d-4f5c-9c98-d5fa303d1f04";
const PUMP_PROGRAM_ID: &str = "6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P";

// ✅ Helius tip accounts (from official docs)
const HELIUS_TIP_ACCOUNTS: [&str; 10] = [
    "4ACfpUFoaSD9bfPdeu6DBt89gB6ENTeHBXCAi87NhDEE",
    "D2L6yPZ2FmmmTKPgzaMKdhu6EWZcTpLy1Vhx8uvZe7NZ",
    "9bnz4RShgq1hAnLnZbP8kbgBg1kEmcJBYQq3gQbmnSta",
    "5VY91ws6B2hMmBFRsXkoAAdsPHBJwRfBht4DXox3xkwn",
    "2nyhqdwKcJZR2vcqCyrYsaPVdAnFoJjiksCXJ7hfEYgD",
    "2q5pghRs6arqVjRvT5gfgWfWcHWmw1ZuCzphgd5KfWGJ",
    "wyvPkWjVZz1M8fHQnMMCDTQDbkManefNNhweYk5WkcF",
    "3KCKozbAaF75qEU33jtzozcJ29yJuaLJTy2jFdzUY8bT",
    "4vieeGHPYPG2MmyPRcYjdiDmmhN3ww7hsFNap8pVN3Ey",
    "4TQLFNWK8AovT1gFvda5jfw2oJeRMKEmw7aH6MGBJ3or",
];

struct BotConfig {
    wallet: Keypair,
    rpc: RpcClient,
    sol_amount: u64,
    priority_fee: u64,
    compute_units: u32,
    one_shot_mode: bool,
    submission_mode: SubmissionMode,
    jito_tip: u64,
    require_socials: bool,
    require_twitter: bool,
    min_socials_count: usize,
    // 🔥 DEV BUY FILTERS
    min_dev_buy_usd: f64,  // Minimum dev buy in USD
    max_dev_buy_usd: f64,  // Maximum dev buy in USD
    sol_price_usd: f64,    // Current SOL price for conversion
}

#[derive(Debug, Clone, Copy)]
enum SubmissionMode {
    Helius,
    Jito,
    Rpc,
}

impl BotConfig {
    pub fn new(wallet: Keypair) -> Self {
        Self {
            rpc: RpcClient::new(RPC_URL.to_string()),
            wallet,
            sol_amount: 15_000_000,
            priority_fee: 11_000_000,
            compute_units: 200_000,
            one_shot_mode: true,
            submission_mode: SubmissionMode::Helius,
            jito_tip: 1_500_000,
            require_socials: true,        // 🔥 DISABLED for speed
            require_twitter: false,
            min_socials_count: 0,
            // 🔥 DEV BUY FILTERS ($600-$1200 range)
            min_dev_buy_usd: 600.0,
            max_dev_buy_usd: 1200.0,
            sol_price_usd: 122.0,  // 🔥 UPDATED: Current SOL price
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenv::dotenv().ok();

    println!("⚡ ULTRA FAST HELIUS SNIPER");
    println!("{}", "=".repeat(50));

    let wallet = load_wallet()?;
    println!("💰 Wallet: {}", wallet.pubkey());

    let rpc = RpcClient::new(RPC_URL.to_string());
    let balance = rpc.get_balance(&wallet.pubkey()).await?;
    println!("💵 Balance: {} SOL", balance as f64 / 1e9);

    println!("\n⚡ Pre-loading global account...");
    buy::preload_global(&rpc).await?;
    println!("✅ Cached!");

    let config = BotConfig::new(wallet);

    println!("\n🎯 Config:");
    println!("   Buy: {} SOL", config.sol_amount as f64 / 1e9);
    println!("   Priority: {} micro-lamports", config.priority_fee);
    let mode_str = match config.submission_mode {
        SubmissionMode::Helius => "⚡ HELIUS SENDER (Dual Routing)",
        SubmissionMode::Jito => "🚀 JITO BUNDLE",
        SubmissionMode::Rpc => "📡 Regular RPC",
    };
    println!("   Mode: {}", mode_str);
    println!("   Tip: {} SOL", config.jito_tip as f64 / 1e9);

    // 🛑 ONE-SHOT MODE INFO
    if config.one_shot_mode {
        println!("\n🛑 ONE-SHOT MODE: Bot will stop after first successful buy!");
    }

    if config.require_socials || config.require_twitter || config.min_socials_count > 0 {
        println!("\n🔥 Social Filters:");
        if config.require_socials {
            println!("   ✓ Skip tokens without socials");
        }
        if config.require_twitter {
            println!("   ✓ Require Twitter/X");
        }
        if config.min_socials_count > 0 {
            println!("   ✓ Min {} social links", config.min_socials_count);
        }
    }

    // 🔥 DEV BUY FILTER INFO
    let min_sol = config.min_dev_buy_usd / config.sol_price_usd;
    let max_sol = config.max_dev_buy_usd / config.sol_price_usd;
    println!("\n💰 Dev Buy Filter:");
    println!("   ✓ Range: ${}-${} (@ ${}/SOL)",
             config.min_dev_buy_usd as u32,
             config.max_dev_buy_usd as u32,
             config.sol_price_usd as u32);
    println!("   ✓ SOL Range: {:.2}-{:.2} SOL", min_sol, max_sol);

    println!("\n📡 Connecting...");
    listen_for_new_tokens(config).await?;

    Ok(())
}

async fn listen_for_new_tokens(config: BotConfig) -> Result<()> {
    let pump_program = Pubkey::from_str(PUMP_PROGRAM_ID)?;

    let subscribe_msg = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "logsSubscribe",
        "params": [
            {
                "mentions": [pump_program.to_string()]
            },
            {
                "commitment": "processed"
            }
        ]
    });

    let (ws_stream, _) = connect_async(WSS_URL).await?;
    let (mut write, mut read) = ws_stream.split();

    use futures_util::SinkExt;
    write.send(WsMessage::Text(subscribe_msg.to_string())).await?;

    println!("✅ Listening...\n");

    let mut detected = 0;

    while let Some(msg) = read.next().await {
        if let Ok(WsMessage::Text(text)) = msg {
            if let Ok(notification) = serde_json::from_str::<serde_json::Value>(&text) {
                if is_initialize_bonding_curve(&notification) {
                    if let Some(signature) = extract_signature(&notification) {
                        detected += 1;

                        println!("\n🔔 TOKEN #{}", detected);
                        println!("   Sig: {}", signature);

                        match ultra_fast_buy(&config, &signature).await {
                            Ok(_) => {
                                println!("✅ DONE!");
                                if config.one_shot_mode {
                                    println!("\n🛑 ONE-SHOT MODE: Stopping bot after successful buy!");
                                    println!("🔴 Bot will now exit...\n");
                                    break;
                                }
                            }
                            Err(e) => {
                                eprintln!("❌ Failed: {}", e);
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(())
}

async fn ultra_fast_buy(config: &BotConfig, init_signature: &str) -> Result<()> {
    let (accounts, mint) = PumpBuyAccounts::from_initialize_tx(&config.rpc, init_signature).await?;
    println!("   🪙 {}", mint);

    // 🔥 DEV BUY FILTER CHECK
    let dev_buy_sol = accounts.dev_buy_sol as f64 / 1e9;
    let dev_buy_usd = dev_buy_sol * config.sol_price_usd;

    println!("   💰 Dev buy: {:.3} SOL (${:.0})", dev_buy_sol, dev_buy_usd);

    if dev_buy_usd < config.min_dev_buy_usd || dev_buy_usd > config.max_dev_buy_usd {
        println!("   ⛔ SKIP: Dev buy ${:.0} outside range ${}-${}",
                 dev_buy_usd,
                 config.min_dev_buy_usd,
                 config.max_dev_buy_usd);
        return Err(anyhow!("Token skipped - dev buy outside range"));
    }
    println!("   ✅ Dev buy in range!");

    // 🚀 PARALLEL: Start social check in background while preparing TX!
    let social_check_enabled = config.require_socials || config.require_twitter || config.min_socials_count > 0;

    let social_task = if social_check_enabled {
        println!("   🔍 Checking socials (parallel with retry)...");
        let mint_str = mint.to_string();
        let require_socials = config.require_socials;
        let require_twitter = config.require_twitter;
        let min_socials = config.min_socials_count;

        Some(tokio::spawn(async move {
            // Retry logic for metadata indexing
            let mut attempts = 0;
            let max_attempts = 3;

            loop {
                attempts += 1;

                match check_token_socials(&mint_str).await {
                    Ok(socials) => {
                        println!("   📱 {} social(s) found (attempt {})", socials.count(), attempts);
                        if socials.count() > 0 {
                            socials.display();
                        }

                        // Check filters
                        if require_socials && !socials.has_any() {
                            return Err(anyhow!("SKIP: No socials"));
                        }
                        if require_twitter && !socials.has_twitter() {
                            return Err(anyhow!("SKIP: No Twitter/X"));
                        }
                        if socials.count() < min_socials {
                            return Err(anyhow!("SKIP: Need {} socials", min_socials));
                        }

                        return Ok(socials);
                    }
                    Err(e) if attempts < max_attempts => {
                        println!("   ⏳ Metadata not ready (attempt {}/{}), retrying in 2s...", attempts, max_attempts);
                        tokio::time::sleep(tokio::time::Duration::from_millis(150)).await;
                    }
                    Err(e) => {
                        println!("   ⚠️  Social check failed after {} attempts", max_attempts);
                        if require_socials {
                            return Err(anyhow!("SKIP: Could not verify socials"));
                        }
                        // If not required, proceed anyway
                        return Ok(Socials {
                            twitter: None,
                            website: None,
                            telegram: None,
                            discord: None,
                        });
                    }
                }
            }
        }))
    } else {
        None
    };

    // ⚡ While socials are being checked, prepare the transaction!
    println!("   ⚡ Preparing TX...");
    let user_wallet = config.wallet.pubkey();
    let user_ata = get_associated_token_address(&user_wallet, &accounts.mint);

    let buy_ix = build_buy_instruction(
        &config.rpc,
        &accounts,
        &user_wallet,
        &user_ata,
        config.sol_amount,
    ).await?;

    let mut rng = rand::thread_rng();
    let tip_account = Pubkey::from_str(
        HELIUS_TIP_ACCOUNTS.choose(&mut rng).unwrap()
    )?;

    let instructions = vec![
        ComputeBudgetInstruction::set_compute_unit_limit(config.compute_units),
        ComputeBudgetInstruction::set_compute_unit_price(config.priority_fee),
        create_associated_token_account(
            &user_wallet,
            &user_wallet,
            &accounts.mint,
            &spl_token::id(),
        ),
        buy_ix,
        system_instruction::transfer(
            &user_wallet,
            &tip_account,
            config.jito_tip,
        ),
    ];

    let recent_blockhash = config.rpc.get_latest_blockhash().await?;

    let msg = v0::Message::try_compile(
        &user_wallet,
        &instructions,
        &[],
        recent_blockhash,
    )?;

    let tx = VersionedTransaction::try_new(
        VersionedMessage::V0(msg),
        &[&config.wallet],
    )?;

    let tx_sig = tx.signatures[0];

    // 🎯 Wait for social check to complete (if enabled)
    if let Some(task) = social_task {
        match task.await {
            Ok(Ok(_)) => {
                println!("   ✅ Social check passed!");
            }
            Ok(Err(e)) => {
                println!("   ⏭️  {}", e);
                return Err(anyhow!("Token skipped - social check failed")); // Skip this token
            }
            Err(e) => {
                println!("   ⚠️  Social task panicked: {}", e);
                if config.require_socials {
                    return Err(anyhow!("Token skipped - social check required"));
                }
            }
        }
    }

    println!("   🔖 TX Sig: {}", tx_sig);
    println!("   🚀 Multi-submission mode...");

    // 🔥 MULTI-SUBMISSION
    let tx_helius = tx.clone();
    let tx_jito = tx.clone();
    let tx_rpc = tx.clone();

    let wallet_bytes = config.wallet.to_bytes();
    let wallet_clone = Keypair::from_bytes(&wallet_bytes)?;
    let jito_tip = config.jito_tip;
    let rpc_url = RPC_URL.to_string();

    let helius_task = tokio::spawn(async move {
        match send_helius_transaction(tx_helius).await {
            Ok(sig) => Ok(format!("Helius: {}", sig)),
            Err(e) => Err(e),
        }
    });

    let jito_task = tokio::spawn(async move {
        match send_jito_bundle(tx_jito, &wallet_clone, recent_blockhash, jito_tip).await {
            Ok(bundle_id) => Ok(format!("Jito: {}", bundle_id)),
            Err(e) => Err(e),
        }
    });

    let rpc_task = tokio::spawn(async move {
        let rpc = RpcClient::new(rpc_url);
        match rpc.send_transaction(&tx_rpc).await {
            Ok(sig) => Ok(format!("RPC: {}", sig)),
            Err(e) => Err(anyhow::anyhow!("RPC error: {}", e)),
        }
    });

    let result = tokio::select! {
        res = helius_task => {
            match res {
                Ok(Ok(msg)) => {
                    println!("   ✅ {} 🏆", msg);
                    println!("   ⚡ Dual routed to validators + Jito");
                    Ok(())
                }
                Ok(Err(e)) => Err(e),
                Err(e) => Err(anyhow::anyhow!("Helius task panic: {}", e))
            }
        }
        res = jito_task => {
            match res {
                Ok(Ok(msg)) => {
                    println!("   ✅ {} 🏆", msg);
                    println!("   ⚡ Bundle submitted to validators");
                    Ok(())
                }
                Ok(Err(e)) => Err(e),
                Err(e) => Err(anyhow::anyhow!("Jito task panic: {}", e))
            }
        }
        res = rpc_task => {
            match res {
                Ok(Ok(msg)) => {
                    println!("   ✅ {} 🏆", msg);
                    Ok(())
                }
                Ok(Err(e)) => Err(e),
                Err(e) => Err(anyhow::anyhow!("RPC task panic: {}", e))
            }
        }
    };

    println!("   🔗 Track TX: https://solscan.io/tx/{}", tx_sig);
    println!("   ⏳ Wait ~3-5 seconds for confirmation...");

    result
}

fn load_wallet() -> Result<Keypair> {
    dotenv::dotenv().ok();

    if let Ok(private_key_base58) = std::env::var("SOLANA_PRIVATE_KEY") {
        let bytes = bs58::decode(private_key_base58.trim())
            .into_vec()
            .map_err(|e| anyhow!("Invalid Base58: {}", e))?;

        let keypair = Keypair::from_bytes(&bytes)
            .map_err(|e| anyhow!("Invalid keypair: {}", e))?;

        return Ok(keypair);
    }

    Err(anyhow!("Set SOLANA_PRIVATE_KEY in .env file"))
}

fn is_initialize_bonding_curve(notification: &serde_json::Value) -> bool {
    if let Some(logs) = notification["params"]["result"]["value"]["logs"].as_array() {
        let has_pump = logs.iter().any(|l|
            l.as_str().map_or(false, |s|
                s.contains("Program 6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P invoke [1]")
            )
        );

        let has_create = logs.iter().any(|l|
            l.as_str().map_or(false, |s|
                s.contains("Program log: Instruction: Create")
            )
        );

        return has_pump && has_create;
    }
    false
}

fn extract_signature(notification: &serde_json::Value) -> Option<String> {
    notification["params"]["result"]["value"]["signature"]
        .as_str()
        .map(|s| s.to_string())
}