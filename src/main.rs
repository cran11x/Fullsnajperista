// main.rs - BACK TO BASICS - PURE SPEED

mod detection;
mod buy;
mod accounts;

use anyhow::{anyhow, Result};
use solana_client::{
    nonblocking::rpc_client::RpcClient,
    rpc_config::RpcSendTransactionConfig,
};
use solana_sdk::{
    bs58,
    compute_budget::ComputeBudgetInstruction,
    message::{v0, VersionedMessage},
    pubkey::Pubkey,
    signature::{Keypair, Signer},
    transaction::VersionedTransaction,
};
use spl_associated_token_account::{get_associated_token_address, instruction::create_associated_token_account};
use std::str::FromStr;
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message as WsMessage};
use futures_util::StreamExt;

use detection::PumpBuyAccounts;
use buy::build_buy_instruction;

const RPC_URL: &str = "https://lb.drpc.org/ogrpc?network=solana&dkey=AovWXi0VzUCig4R4vBTXu4nRoZLjr_kR8LqsQrxF2MGT";
const WSS_URL: &str = "wss://lb.drpc.org/ogws?network=solana&dkey=AovWXi0VzUCig4R4vBTXu4nRoZLjr_kR8LqsQrxF2MGT";
const PUMP_PROGRAM_ID: &str = "6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P";

struct BotConfig {
    wallet: Keypair,
    rpc: RpcClient,
    sol_amount: u64,
    priority_fee: u64,
    compute_units: u32,
    one_shot_mode: bool,
}

impl BotConfig {
    pub fn new(wallet: Keypair) -> Self {
        Self {
            rpc: RpcClient::new(RPC_URL.to_string()),
            wallet,
            sol_amount: 2_000_000,
            priority_fee: 2_000_000,
            compute_units: 300_000,
            one_shot_mode: true,
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenv::dotenv().ok();

    println!("⚡ SIMPLE FAST Sniper");
    println!("{}", "=".repeat(50));

    let wallet = load_wallet()?;
    println!("💰 Wallet: {}", wallet.pubkey());

    let rpc = RpcClient::new(RPC_URL.to_string());
    let balance = rpc.get_balance(&wallet.pubkey()).await?;
    println!("💵 Balance: {} SOL", balance as f64 / 1e9);

    let config = BotConfig::new(wallet);

    println!("\n🎯 Buy: {} SOL | Fee: {}",
             config.sol_amount as f64 / 1e9,
             config.priority_fee);

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
    // STEP 1: Get accounts
    let (accounts, mint) = PumpBuyAccounts::from_initialize_tx(&config.rpc, init_signature).await?;
    println!("   🪙 {}", mint);

    let user_wallet = config.wallet.pubkey();
    let user_ata = get_associated_token_address(&user_wallet, &accounts.mint);

    // STEP 2: Build instruction
    let buy_ix = build_buy_instruction(
        &config.rpc,
        &accounts,
        &user_wallet,
        &user_ata,
        config.sol_amount,
    ).await?;

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
    ];

    // STEP 3: Get blockhash & build TX
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

    // STEP 4: Send
    println!("   🚀 Sending...");
    let signature = config.rpc.send_transaction_with_config(
        &tx,
        RpcSendTransactionConfig {
            skip_preflight: true,
            max_retries: Some(0),
            ..Default::default()
        }
    ).await?;

    println!("   ✅ {}", signature);
    println!("   🔗 https://solscan.io/tx/{}", signature);

    Ok(())
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