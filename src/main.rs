// main.rs - Fixed verzija - procesira JEDAN token po jedan

mod detection;
mod buy;

use anyhow::{anyhow, Result};
use solana_client::{
    nonblocking::rpc_client::RpcClient,
    rpc_config::RpcTransactionConfig,
};
use solana_sdk::{bs58, commitment_config::CommitmentConfig, compute_budget::ComputeBudgetInstruction, message::{v0, VersionedMessage}, pubkey::Pubkey, signature::{Keypair, Signature, Signer}, transaction::VersionedTransaction};
use solana_transaction_status::UiTransactionEncoding;
use spl_associated_token_account::{get_associated_token_address, instruction::create_associated_token_account};
use std::str::FromStr;
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message as WsMessage};
use futures_util::StreamExt;

use detection::PumpBuyAccounts;
use buy::build_buy_instruction;

const RPC_URL: &str = "https://lb.drpc.org/ogrpc?network=solana&dkey=AovWXi0VzUCig4R4vBTXu4nRoZLjr_kR8LqsQrxF2MGT";
const WSS_URL: &str = "wss://lb.drpc.org/ogws?network=solana&dkey=AovWXi0VzUCig4R4vBTXu4nRoZLjr_kR8LqsQrxF2MGT";
const PUMP_PROGRAM_ID: &str = "6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P";

// Konfiguracija bota sa TEST modom
struct BotConfig {
    wallet: Keypair,
    rpc: RpcClient,
    sol_amount: u64,
    use_jito: bool,
    jito_tip: u64,
    priority_fee: u64,
    compute_units: u32,

    // 🆕 TEST OPCIJE
    one_shot_mode: bool,    // Zaustavi se nakon prvog buy-a
    dry_run: bool,          // Samo detektuj, ne kupuj
    max_price_sol: f64,     // Max cena po buy-u
}

impl BotConfig {
    pub fn new(wallet: Keypair) -> Self {
        Self {
            rpc: RpcClient::new(RPC_URL.to_string()),
            wallet,
            sol_amount: 1_000_000,       // 0.001 SOL default (SIGURNO!)
            use_jito: false,
            jito_tip: 10_000,
            priority_fee: 100_000,
            compute_units: 300_000,
            one_shot_mode: false,
            dry_run: false,
            max_price_sol: 0.1,          // Max 0.1 SOL
        }
    }

    // 🆕 TEST MODE builder
    pub fn test_mode(mut self) -> Self {
        self.one_shot_mode = true;
        self.sol_amount = 1_000_000;     // 0.001 SOL
        self.priority_fee = 50_000;      // Manji fee za test
        println!("🧪 TEST MODE ENABLED:");
        println!("   - Will stop after first buy");
        println!("   - Buy amount: 0.001 SOL");
        println!("   - Priority fee: low");
        self
    }

    // 🆕 DRY RUN MODE (samo detektuj, ne kupuj)
    pub fn dry_run_mode(mut self) -> Self {
        self.dry_run = true;
        self.one_shot_mode = true;
        println!("🔍 DRY RUN MODE ENABLED:");
        println!("   - Will only detect, NOT buy");
        println!("   - Will stop after first detection");
        self
    }

    pub fn with_amount(mut self, lamports: u64) -> Self {
        if lamports > (self.max_price_sol * 1_000_000_000.0) as u64 {
            println!("⚠️  Amount exceeds max_price_sol, capping to {} SOL", self.max_price_sol);
            self.sol_amount = (self.max_price_sol * 1_000_000_000.0) as u64;
        } else {
            self.sol_amount = lamports;
        }
        self
    }

    pub fn with_jito(mut self, tip_lamports: u64) -> Self {
        self.use_jito = true;
        self.jito_tip = tip_lamports;
        self
    }

    pub fn with_priority_fee(mut self, fee: u64) -> Self {
        self.priority_fee = fee;
        self
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // Učitaj .env fajl
    dotenv::dotenv().ok();

    println!("🚀 Pump.Fun Sniper Bot Starting...");
    println!("{}", "=".repeat(50));

    // Load wallet
    let wallet = load_wallet()?;
    println!("💰 Wallet: {}", wallet.pubkey());

    // Check balance
    let rpc = RpcClient::new(RPC_URL.to_string());
    let balance = rpc.get_balance(&wallet.pubkey()).await?;
    println!("💵 Balance: {} SOL", balance as f64 / 1_000_000_000.0);

    if balance < 10_000_000 {
        println!("⚠️  WARNING: Low balance! Need at least 0.01 SOL for testing.");
    }

    println!("{}", "=".repeat(50));

    // 🧪 IZABERI MOD:

    // OPCIJA 1: DRY RUN (samo detektuj, ne kupuj)
    // let config = BotConfig::new(wallet).dry_run_mode();

    // OPCIJA 2: TEST MODE (detektuj i kupi JEDAN token sa 0.001 SOL)
    let config = BotConfig::new(wallet)
        .test_mode()
        .with_amount(2_000_000);  // 0.002 SOL za test

    // OPCIJA 3: Custom config
    // let config = BotConfig::new(wallet)
    //     .with_amount(5_000_000)      // 0.005 SOL
    //     .with_priority_fee(100_000);
    // config.one_shot_mode = true;

    println!("\n🎯 Bot Configuration:");
    println!("   Buy Amount: {} SOL", config.sol_amount as f64 / 1_000_000_000.0);
    println!("   Priority Fee: {} micro-lamports/CU", config.priority_fee);
    println!("   One-Shot Mode: {}", config.one_shot_mode);
    println!("   Dry Run: {}", config.dry_run);
    println!();

    // Start listening
    println!("🔌 Connecting to WebSocket...");
    listen_for_new_tokens(config).await?;

    Ok(())
}

/// ✅ FIXED: WebSocket listener - procesira JEDAN token po jedan
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
                "commitment": "confirmed"
            }
        ]
    });

    let (ws_stream, _) = connect_async(WSS_URL).await?;
    let (mut write, mut read) = ws_stream.split();

    use futures_util::SinkExt;
    write.send(WsMessage::Text(subscribe_msg.to_string())).await?;

    println!("✅ Subscribed to Pump.Fun program logs");
    println!("👀 Watching for new tokens...");
    println!("   (Press Ctrl+C to stop)\n");

    let mut detected_count = 0;
    let mut successful_buys = 0;

    while let Some(msg) = read.next().await {
        if let Ok(WsMessage::Text(text)) = msg {
            if let Ok(notification) = serde_json::from_str::<serde_json::Value>(&text) {

                if is_initialize_bonding_curve(&notification) {
                    if let Some(signature) = extract_signature(&notification) {
                        detected_count += 1;

                        println!("\n{}", "=".repeat(50));
                        println!("🔔 NEW TOKEN DETECTED! (#{})", detected_count);
                        println!("   Signature: {}", signature);
                        println!("   Time: {}", chrono::Utc::now().format("%H:%M:%S"));
                        println!("{}", "=".repeat(50));

                        // ✅ ČEKA da se završi procesiranje PRE nego što pređe na sledeći!
                        match process_new_token(&config, signature).await {
                            Ok(_) => {
                                successful_buys += 1;

                                // 🛑 ONE-SHOT MODE: zaustavi se nakon prvog uspešnog buy-a
                                if config.one_shot_mode {
                                    println!("\n🛑 ONE-SHOT MODE: Stopping after successful buy");
                                    break;
                                }
                            }
                            Err(e) => {
                                eprintln!("\n❌ Error processing token: {}", e);

                                // Nastavi dalje sa sledećim tokenom
                                if config.one_shot_mode {
                                    println!("   (One-shot mode: will try next token)");
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    println!("\n{}", "=".repeat(50));
    println!("👋 Bot stopped.");
    println!("   Total tokens detected: {}", detected_count);
    println!("   Successful buys: {}", successful_buys);
    println!("{}", "=".repeat(50));

    Ok(())
}

/// Procesira novi token
async fn process_new_token(config: &BotConfig, signature: String) -> Result<()> {
    println!("🔍 Step 1/4: Fetching transaction details...");

  

    let sig = Signature::from_str(&signature)?;
    let tx = config.rpc.get_transaction_with_config(
        &sig,
        RpcTransactionConfig {
            encoding: Some(UiTransactionEncoding::JsonParsed),
            max_supported_transaction_version: Some(0),
            commitment: Some(CommitmentConfig::confirmed()),
        }
    ).await?;

    let mint = extract_mint_from_tx(&tx)?;
    println!("🪙 Mint: {}", mint);
    println!("   Explorer: https://pump.fun/{}", mint);

    println!("\n🔍 Step 2/4: Extracting accounts...");
    let accounts = match PumpBuyAccounts::from_initialize_tx(&config.rpc, &mint, &signature).await {
        Ok(acc) => {
            println!("✅ Extracted from InitializeBondingCurve");
            acc
        },
        Err(e) => {
            eprintln!("⚠️  Failed to extract from init TX: {}", e);
            println!("🔄 Trying to find existing Buy TX...");

            tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
            let acc = PumpBuyAccounts::from_existing_buy_tx(&config.rpc, &mint).await?;
            println!("✅ Extracted from existing Buy TX");
            acc
        }
    };

    // 🆕 DRY RUN MODE: samo prikaži info, ne kupuj
    if config.dry_run {
        println!("\n🔍 DRY RUN MODE - Would buy:");
        println!("   Amount: {} SOL", config.sol_amount as f64 / 1_000_000_000.0);
        println!("   Mint: {}", mint);
        println!("   Bonding Curve: {}", accounts.bonding_curve);
        println!("\n✅ Dry run complete (no transaction sent)");
        return Ok(());
    }

    println!("\n🔍 Step 3/4: Building buy transaction...");
    println!("   Amount: {} SOL", config.sol_amount as f64 / 1_000_000_000.0);

    execute_buy(config, &accounts).await?;

    println!("\n✅ Step 4/4: COMPLETE!");
    Ok(())
}

/// Izvršava buy transakciju
async fn execute_buy(config: &BotConfig, accounts: &PumpBuyAccounts) -> Result<()> {
    let user_wallet = config.wallet.pubkey();
    let user_ata = get_associated_token_address(&user_wallet, &accounts.mint);

    println!("   User ATA: {}", user_ata);

    let mut instructions = vec![
        ComputeBudgetInstruction::set_compute_unit_limit(config.compute_units),
        ComputeBudgetInstruction::set_compute_unit_price(config.priority_fee),
    ];

    // Check ATA
    print!("   Checking ATA... ");
    if config.rpc.get_account(&user_ata).await.is_err() {
        println!("not found, will create");
        let create_ata = create_associated_token_account(
            &user_wallet,
            &user_wallet,
            &accounts.mint,
            &spl_token::id(),
        );
        instructions.push(create_ata);
    } else {
        println!("exists");
    }

    // Build buy instruction
    let buy_ix = build_buy_instruction(
        accounts,
        &user_wallet,
        &user_ata,
        config.sol_amount,
    )?;
    instructions.push(buy_ix);

    // Build transaction
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

    // Send
    println!("\n🚀 Sending transaction...");
    let signature = if config.use_jito {
        println!("   Via: Jito bundle");
        send_jito_bundle(&tx, config.jito_tip).await?
    } else {
        println!("   Via: Regular RPC");
        config.rpc.send_transaction(&tx).await?
    };

    println!("\n{}", "=".repeat(50));
    println!("✅ BUY TRANSACTION SENT!");
    println!("   Signature: {}", signature);
    println!("   Explorer: https://solscan.io/tx/{}", signature);
    println!("   Amount: {} SOL", config.sol_amount as f64 / 1_000_000_000.0);
    println!("{}", "=".repeat(50));

    Ok(())
}

async fn send_jito_bundle(_tx: &VersionedTransaction, _tip_lamports: u64) -> Result<Signature> {
    println!("⚠️  Jito bundle not implemented, using regular send");
    let rpc = RpcClient::new(RPC_URL.to_string());
    Ok(rpc.send_transaction(_tx).await?)
}

// Helper funkcije

fn load_wallet() -> Result<Keypair> {
    // Učitaj .env
    dotenv::dotenv().ok();

    // Učitaj Base58 private key iz .env
    if let Ok(private_key_base58) = std::env::var("SOLANA_PRIVATE_KEY") {
        let bytes = bs58::decode(private_key_base58.trim())
            .into_vec()
            .map_err(|e| anyhow!("Invalid Base58 key: {}", e))?;

        let keypair = Keypair::from_bytes(&bytes)
            .map_err(|e| anyhow!("Invalid keypair: {}", e))?;

        println!("✅ Loaded wallet from .env");
        return Ok(keypair);
    }

    Err(anyhow!("No wallet configured. Set SOLANA_PRIVATE_KEY in .env"))
}

fn is_initialize_bonding_curve(notification: &serde_json::Value) -> bool {
    if let Some(logs) = notification["params"]["result"]["value"]["logs"].as_array() {
        let has_pump_invoke = logs.iter().any(|l|
            l.as_str().map_or(false, |s|
                s.contains("Program 6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P invoke [1]")
            )
        );

        let has_create_instruction = logs.iter().any(|l|
            l.as_str().map_or(false, |s|
                s.contains("Program log: Instruction: Create")
            )
        );

        return has_pump_invoke && has_create_instruction;
    }
    false
}

fn extract_signature(notification: &serde_json::Value) -> Option<String> {
    notification["params"]["result"]["value"]["signature"]
        .as_str()
        .map(|s| s.to_string())
}

fn extract_mint_from_tx(
    tx: &solana_transaction_status::EncodedConfirmedTransactionWithStatusMeta
) -> Result<Pubkey> {
    if let solana_transaction_status::EncodedTransaction::Json(ui_tx) = &tx.transaction.transaction {
        if let solana_transaction_status::UiMessage::Parsed(parsed) = &ui_tx.message {
            let accounts: Vec<Pubkey> = parsed.account_keys
                .iter()
                .filter_map(|key| Pubkey::from_str(&key.pubkey).ok())
                .collect();

            if accounts.len() > 1 {
                return Ok(accounts[1]);
            }
        }
    }
    Err(anyhow!("Could not extract mint from transaction"))
}