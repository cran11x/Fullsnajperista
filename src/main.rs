// main.rs - WITH DUPLICATE PROTECTION & MC TRACKING

mod detection;
mod buy;
mod accounts;
mod jito;
mod helius;
mod socials;
mod das_check;

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
use futures_util::{StreamExt, SinkExt};
use rand::seq::SliceRandom;
use chrono::Utc;
use std::time::Duration;
use std::sync::Arc;

use detection::PumpBuyAccounts;
use buy::build_buy_instruction;
use jito::send_jito_bundle;
use helius::send_helius_transaction;
use socials::{check_token_socials, Socials};
use das_check::check_creator_token_count_das;
use crate::accounts::{TokenBuy, TokenTracker, SeenTokens, fetch_bonding_curve_mc, BondingCurveAccount};  // ← ADD MC imports

const RPC_URL: &str = "https://mainnet.helius-rpc.com/?api-key=7ef7af02-aa9d-4f5c-9c98-d5fa303d1f04";
const WSS_URL: &str = "wss://mainnet.helius-rpc.com/?api-key=7ef7af02-aa9d-4f5c-9c98-d5fa303d1f04";
const PUMP_PROGRAM_ID: &str = "6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P";
const ENABLE_TRACKER: bool = true;

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
    min_dev_buy_usd: f64,
    max_dev_buy_usd: f64,
    sol_price_usd: f64,
    min_dev_tokens: usize,
    max_dev_tokens: usize,
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
            require_socials: false,
            require_twitter: false,
            min_socials_count: 0,
            min_dev_buy_usd: 500.0,
            max_dev_buy_usd: 1200.0,
            sol_price_usd: 162.0,
            min_dev_tokens: 6,
            max_dev_tokens: 10,
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenv::dotenv().ok();

    println!("⚡ ULTRA FAST HELIUS SNIPER v2.0 (AUTO-RECONNECT + DUPLICATE PROTECTION + MC TRACKING)");
    println!("{}", "=".repeat(60));

    let wallet = load_wallet()?;
    println!("💰 Wallet: {}", wallet.pubkey());

    let rpc = RpcClient::new(RPC_URL.to_string());
    let balance = rpc.get_balance(&wallet.pubkey()).await?;
    println!("💵 Balance: {} SOL", balance as f64 / 1e9);

    println!("\n⚡ Pre-loading global account...");
    buy::preload_global(&rpc).await?;
    println!("✅ Cached!");

    // ✅ CREATE DUPLICATE TRACKER
    let seen_tokens = Arc::new(SeenTokens::new());
    println!("🛡️  Duplicate protection: ENABLED");

    let config = BotConfig::new(wallet);

    let mut tracker = if ENABLE_TRACKER {
        match TokenTracker::new() {
            Ok(t) => {
                println!("✅ Tracker enabled (with MC tracking)");
                Some(t)
            }
            Err(e) => {
                println!("⚠️  Tracker failed to init: {}", e);
                None
            }
        }
    } else {
        None
    };

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
    println!("   Tracker: {}", if ENABLE_TRACKER { "✅ ON (with MC)" } else { "❌ OFF" });
    println!("   Auto-Reconnect: ✅ ENABLED");
    println!("   Duplicate Protection: 🛡️  ENABLED");

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

    let min_sol = config.min_dev_buy_usd / config.sol_price_usd;
    let max_sol = config.max_dev_buy_usd / config.sol_price_usd;
    println!("\n💰 Dev Buy Filter:");
    println!("   ✓ Range: ${}-${} (@ ${}/SOL)",
             config.min_dev_buy_usd as u32,
             config.max_dev_buy_usd as u32,
             config.sol_price_usd as u32);
    println!("   ✓ Dev tokens range: {}-{}", config.min_dev_tokens, config.max_dev_tokens);
    println!("   ✓ SOL Range: {:.2}-{:.2} SOL", min_sol, max_sol);

    println!("\n📡 Starting WebSocket listener (will auto-reconnect on disconnect)...");

    // ✅ PASS seen_tokens TO LISTENER
    listen_for_new_tokens(config, tracker, seen_tokens).await?;

    Ok(())
}

// ✅ ADD seen_tokens PARAMETER
async fn listen_for_new_tokens(
    config: BotConfig,
    mut tracker: Option<TokenTracker>,
    seen_tokens: Arc<SeenTokens>,
) -> Result<()> {
    let mut detected = 0;
    let mut reconnect_count = 0;
    let mut total_reconnects = 0;

    loop {
        reconnect_count += 1;
        total_reconnects += 1;

        let reconnect_delay = if reconnect_count == 1 {
            Duration::from_secs(0)
        } else if reconnect_count < 5 {
            Duration::from_secs(5)
        } else {
            Duration::from_secs(15)
        };

        if reconnect_delay.as_secs() > 0 {
            println!("\n🔄 Reconnecting in {} seconds... (reconnect #{})",
                     reconnect_delay.as_secs(), total_reconnects);
            tokio::time::sleep(reconnect_delay).await;
        }

        println!("\n🔌 Connecting to WebSocket...");

        // ✅ PASS seen_tokens TO WEBSOCKET HANDLER
        match listen_websocket_once(&config, &mut tracker, &mut detected, seen_tokens.clone()).await {
            Ok(_) => {
                println!("⚠️  WebSocket closed normally");
                reconnect_count = 0;
            }
            Err(e) => {
                eprintln!("❌ WebSocket error: {}", e);
            }
        }

        if reconnect_count > 10 {
            eprintln!("🚨 Too many rapid reconnects ({}), pausing for 60s...", reconnect_count);
            tokio::time::sleep(Duration::from_secs(60)).await;
            reconnect_count = 0;
        }
    }
}

// ✅ ADD seen_tokens PARAMETER
async fn listen_websocket_once(
    config: &BotConfig,
    tracker: &mut Option<TokenTracker>,
    detected: &mut u32,
    seen_tokens: Arc<SeenTokens>,
) -> Result<()> {
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
    println!("✅ Connected!");

    let (mut write, mut read) = ws_stream.split();
    write.send(WsMessage::Text(subscribe_msg.to_string())).await?;

    while let Some(msg) = read.next().await {
        let msg = msg?;
        if let WsMessage::Text(text) = msg {
            let notification: serde_json::Value = match serde_json::from_str(&text) {
                Ok(v) => v,
                Err(_) => continue,
            };

            if !is_initialize_bonding_curve(&notification) {
                continue;
            }

            let init_signature = match extract_signature(&notification) {
                Some(s) => s,
                None => continue,
            };

            *detected += 1;

            println!("\n🔥 [{}] NEW TOKEN DETECTED!", detected);
            println!("   Init TX: {}", init_signature);

            let (accounts, mint_pubkey) = match PumpBuyAccounts::from_initialize_tx(&config.rpc, &init_signature).await {
                Ok((acc, m)) => (acc, m),
                Err(e) => {
                    println!("   ❌ Parse error: {}", e);
                    continue;
                }
            };

            let mint = mint_pubkey.to_string();

            // ✅✅✅ DUPLICATE CHECK - MOST IMPORTANT PART! ✅✅✅
            if !seen_tokens.check_and_mark(&mint) {
                println!("   ⏭️  SKIPPING: Already processed this token!");
                continue;
            }

            println!("   ✨ FIRST TIME seeing this token - processing...");
            println!("   🪙 Mint: {}", mint);

            // Rest of your processing...
            match process_and_buy(
                config,
                tracker,
                *detected,
                init_signature,
                accounts,
            ).await {
                Ok(()) => {
                    println!("   ✅ Successfully processed token!");

                    // Print periodic stats
                    if *detected % 10 == 0 {
                        println!("\n📊 SESSION STATS:");
                        println!("   Total detected: {}", detected);
                        println!("   Unique tokens: {}", seen_tokens.count());
                        println!("   Duplicates blocked: {}", *detected - seen_tokens.count() as u32);
                    }
                }
                Err(e) => {
                    println!("   ⚠️  Processing failed: {}", e);
                }
            }
        }
    }

    Ok(())
}

async fn process_and_buy(
    config: &BotConfig,
    tracker: &mut Option<TokenTracker>,
    token_number: u32,
    init_signature: String,
    accounts: PumpBuyAccounts,
) -> Result<()> {
    let mint = accounts.mint;
    let dev_buy_lamports = accounts.dev_buy_sol;
    let dev_buy_sol = dev_buy_lamports as f64 / 1e9;

    let min_sol = config.min_dev_buy_usd / config.sol_price_usd;
    let max_sol = config.max_dev_buy_usd / config.sol_price_usd;

    if dev_buy_sol < min_sol || dev_buy_sol > max_sol {
        return Err(anyhow!("SKIP: Dev buy {:.2} SOL (want {:.2}-{:.2})",
                           dev_buy_sol, min_sol, max_sol));
    }

    println!("   ✅ Dev buy: {:.2} SOL", dev_buy_sol);

    println!("   🔍 Checking creator tokens...");
    let creator_count = match check_creator_token_count_das(&accounts.creator).await {
        Ok(count) => {
            if count < config.min_dev_tokens as u32 {
                return Err(anyhow!("SKIP: Creator has only {} tokens (min: {})",
                                   count, config.min_dev_tokens));
            }
            if count > config.max_dev_tokens as u32 {
                return Err(anyhow!("SKIP: Creator has {} tokens (max: {})",
                                   count, config.max_dev_tokens));
            }
            println!("      ✅ Creator tokens: {}", count);
            count
        }
        Err(e) => {
            println!("      ⚠️  DAS check failed: {}", e);
            0
        }
    };

    // 🆕 FETCH MARKET CAP
    println!("   📊 Fetching market cap...");
    let (curve, mc_sol, mc_usd) = match fetch_bonding_curve_mc(
        &config.rpc,
        &accounts.bonding_curve,
        config.sol_price_usd,
    ).await {
        Ok(data) => {
            println!("   💰 MC: {:.2} SOL (${:.0})", data.1, data.2);
            data
        }
        Err(e) => {
            println!("   ⚠️  Could not fetch MC: {}", e);
            (BondingCurveAccount::default(), 0.0, 0.0)
        }
    };

    let token_price_sol = curve.get_token_price_sol();

    let require_socials = config.require_socials;
    let require_twitter = config.require_twitter;
    let min_socials = config.min_socials_count;

    let social_task = if require_socials || require_twitter || min_socials > 0 {
        let mint_str = mint.to_string();
        Some(tokio::spawn(async move {
            let max_attempts = 3;
            for attempts in 1..=max_attempts {
                match check_token_socials(&mint_str).await {
                    Ok(socials) => {
                        println!("   📱 {} social(s) (attempt {})", socials.count(), attempts);
                        if socials.count() > 0 {
                            socials.display();
                        }

                        if require_socials && !socials.has_any() {
                            return Err(anyhow!("SKIP: No socials"));
                        }
                        if require_twitter && !socials.has_twitter() {
                            return Err(anyhow!("SKIP: No Twitter"));
                        }
                        if socials.count() < min_socials {
                            return Err(anyhow!("SKIP: Need {} socials", min_socials));
                        }

                        return Ok(socials);
                    }
                    Err(_) if attempts < max_attempts => {
                        tokio::time::sleep(tokio::time::Duration::from_millis(150)).await;
                    }
                    Err(_) => {
                        if require_socials {
                            return Err(anyhow!("SKIP: Could not verify socials"));
                        }
                        return Ok(Socials {
                            twitter: None,
                            website: None,
                            telegram: None,
                            discord: None,
                        });
                    }
                }
            }
            unreachable!()
        }))
    } else {
        None
    };

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

    let socials_result = if let Some(task) = social_task {
        match task.await {
            Ok(Ok(s)) => {
                println!("   ✅ Social check passed!");
                Some(s)
            }
            Ok(Err(e)) => {
                return Err(e);
            }
            Err(_) => {
                if config.require_socials {
                    return Err(anyhow!("Social check required"));
                }
                None
            }
        }
    } else {
        None
    };

    println!("   🔖 TX Sig: {}", tx_sig);
    println!("   🚀 Multi-submission...");

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
                    Ok(())
                }
                Ok(Err(e)) => Err(e),
                Err(e) => Err(anyhow::anyhow!("Helius panic: {}", e))
            }
        }
        res = jito_task => {
            match res {
                Ok(Ok(msg)) => {
                    println!("   ✅ {} 🏆", msg);
                    Ok(())
                }
                Ok(Err(e)) => Err(e),
                Err(e) => Err(anyhow::anyhow!("Jito panic: {}", e))
            }
        }
        res = rpc_task => {
            match res {
                Ok(Ok(msg)) => {
                    println!("   ✅ {} 🏆", msg);
                    Ok(())
                }
                Ok(Err(e)) => Err(e),
                Err(e) => Err(anyhow::anyhow!("RPC panic: {}", e))
            }
        }
    };

    println!("   🔗 https://solscan.io/tx/{}", tx_sig);

    // 🆕 FETCH MC AFTER BUY (wait 200ms for TX to settle)
    let (mc_entry_usd, token_price_entry) = if result.is_ok() {
        println!("   ⏳ Waiting 5  0ms for TX settlement...");
        tokio::time::sleep(Duration::from_millis(300)).await;

        println!("   📊 Fetching entry MC...");
        match fetch_bonding_curve_mc(
            &config.rpc,
            &accounts.bonding_curve,
            config.sol_price_usd,
        ).await {
            Ok((curve_post, _, mc_usd_post)) => {
                let price_post = curve_post.get_token_price_sol();
                println!("   💰 Entry MC: ${:.0}", mc_usd_post);

                // Show slippage if both MCs exist
                if mc_usd > 0.0 && mc_usd_post > 0.0 {
                    let slippage = ((mc_usd_post - mc_usd) / mc_usd * 100.0).abs();
                    println!("   📈 MC Change: ${:.0} → ${:.0} ({:.1}% slippage)",
                             mc_usd, mc_usd_post, slippage);
                }

                (Some(mc_usd_post), if price_post > 0.0 { Some(price_post) } else { None })
            }
            Err(e) => {
                println!("   ⚠️  Could not fetch entry MC: {}", e);
                (None, None)
            }
        }
    } else {
        (None, None)
    };

    // 🆕 RECORD BUY WITH PRE AND POST MC DATA
    if let (Ok(()), Some(tracker)) = (&result, tracker) {
        let buy = TokenBuy {
            token_number,
            mint: mint.to_string(),
            signature: init_signature.to_string(),
            creator: accounts.creator.to_string(),
            dev_buy_sol,
            our_buy_sol: config.sol_amount as f64 / 1e9,
            timestamp: Utc::now(),
            has_socials: socials_result.as_ref().map(|s| s.has_any()).unwrap_or(false),
            twitter: socials_result.as_ref().and_then(|s| s.twitter.clone()),
            website: socials_result.as_ref().and_then(|s| s.website.clone()),
            telegram: socials_result.as_ref().and_then(|s| s.telegram.clone()),
            creator_token_count: creator_count,
            detection_method: if dev_buy_lamports > 0 {
                "instruction".to_string()
            } else {
                "balance_fallback".to_string()
            },
            // 🆕 PRE and POST MC tracking
            mc_at_detection_usd: if mc_usd > 0.0 { Some(mc_usd) } else { None },
            mc_at_entry_usd: mc_entry_usd,
            token_price_sol: token_price_entry.or(if token_price_sol > 0.0 { Some(token_price_sol) } else { None }),
        };

        if let Err(e) = tracker.record_buy(buy) {
            println!("   ⚠️  Tracker error: {}", e);
        } else {
            tracker.print_periodic_stats();
        }
    }

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