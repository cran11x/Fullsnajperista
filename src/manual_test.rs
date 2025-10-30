// manual_test.rs - Testiranje sa postojećim tokenom (ne čeka WebSocket)

mod detection;
mod buy;

use anyhow::Result;
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::{
    commitment_config::CommitmentConfig,
    compute_budget::ComputeBudgetInstruction,
    message::{v0, VersionedMessage},
    pubkey::Pubkey,
    signature::{Keypair, Signer},
    transaction::VersionedTransaction,
};
use spl_associated_token_account::{get_associated_token_address, instruction::create_associated_token_account};
use std::str::FromStr;

use detection::PumpBuyAccounts;
use buy::build_buy_instruction;

const RPC_URL: &str = "https://lb.drpc.live/solana/AovWXi0VzUCig4R4vBTXu4nRoZLjr_kR8LqsQrxF2MGT";

#[tokio::main]
async fn main() -> Result<()> {
    println!("🧪 MANUAL TEST MODE");
    println!("=" .repeat(60));

    // 1. Load wallet
    let wallet = load_wallet()?;
    println!("💰 Wallet: {}", wallet.pubkey());

    let rpc = RpcClient::new(RPC_URL.to_string());
    let balance = rpc.get_balance(&wallet.pubkey()).await?;
    println!("💵 Balance: {} SOL\n", balance as f64 / 1_000_000_000.0);

    // 2. Unesi mint adresu postojećeg Pump.Fun tokena
    println!("📝 Enter Pump.Fun token mint address:");
    println!("   (You can find one on https://pump.fun)");
    print!("   Mint: ");

    use std::io::{self, Write};
    io::stdout().flush()?;

    let mut mint_str = String::new();
    io::stdin().read_line(&mut mint_str)?;
    let mint_str = mint_str.trim();

    // Za test, možeš hardcodovati neki aktivan token:
    // let mint_str = "PASTE_MINT_HERE";  // Npr. sa pump.fun

    let mint = Pubkey::from_str(mint_str)?;
    println!("\n🪙 Mint: {}", mint);
    println!("   Pump.Fun: https://pump.fun/{}", mint);

    // 3. Amount
    println!("\n💸 Enter SOL amount to buy (default 0.001):");
    print!("   SOL: ");
    io::stdout().flush()?;

    let mut amount_str = String::new();
    io::stdin().read_line(&mut amount_str)?;
    let amount_str = amount_str.trim();

    let sol_amount = if amount_str.is_empty() {
        1_000_000  // 0.001 SOL default
    } else {
        let sol: f64 = amount_str.parse()?;
        (sol * 1_000_000_000.0) as u64
    };

    println!("   Amount: {} SOL ({} lamports)",
             sol_amount as f64 / 1_000_000_000.0,
             sol_amount
    );

    // 4. Confirm
    println!("\n⚠️  CONFIRM:");
    println!("   Wallet: {}", wallet.pubkey());
    println!("   Mint: {}", mint);
    println!("   Amount: {} SOL", sol_amount as f64 / 1_000_000_000.0);
    println!("\n   Type 'yes' to proceed:");
    print!("   > ");
    io::stdout().flush()?;

    let mut confirm = String::new();
    io::stdin().read_line(&mut confirm)?;

    if confirm.trim().to_lowercase() != "yes" {
        println!("❌ Cancelled");
        return Ok(());
    }

    println!("\n🚀 Starting buy process...\n");
    println!("=" .repeat(60));

    // 5. Ekstraktuj account-e iz postojećeg Buy TX-a
    println!("🔍 Step 1/3: Finding existing Buy transaction...");
    let accounts = PumpBuyAccounts::from_existing_buy_tx(&rpc, &mint).await?;
    println!("✅ Accounts extracted");

    println!("\n📋 Account Summary:");
    println!("   Global: {}", accounts.global);
    println!("   Bonding Curve: {}", accounts.bonding_curve);
    println!("   Assoc BC: {}", accounts.associated_bonding_curve);
    println!("   Creator Vault: {}", accounts.creator_vault);
    println!("   Event Authority: {}", accounts.event_authority);

    // 6. Build transaction
    println!("\n🔍 Step 2/3: Building buy transaction...");

    let user_wallet = wallet.pubkey();
    let user_ata = get_associated_token_address(&user_wallet, &mint);
    println!("   User ATA: {}", user_ata);

    let mut instructions = vec![
        ComputeBudgetInstruction::set_compute_unit_limit(300_000),
        ComputeBudgetInstruction::set_compute_unit_price(100_000),
    ];

    // Check ATA
    print!("   Checking ATA... ");
    if rpc.get_account(&user_ata).await.is_err() {
        println!("not found, creating...");
        let create_ata = create_associated_token_account(
            &user_wallet,
            &user_wallet,
            &mint,
            &spl_token::id(),
        );
        instructions.push(create_ata);
    } else {
        println!("exists ✅");
    }

    // Build buy instruction
    let buy_ix = build_buy_instruction(
        &accounts,
        &user_wallet,
        &user_ata,
        sol_amount,
    )?;
    instructions.push(buy_ix);

    // Build transaction
    let recent_blockhash = rpc.get_latest_blockhash().await?;
    let msg = v0::Message::try_compile(
        &user_wallet,
        &instructions,
        &[],
        recent_blockhash,
    )?;

    let tx = VersionedTransaction::try_new(
        VersionedMessage::V0(msg),
        &[&wallet],
    )?;

    println!("✅ Transaction built");

    // 7. Send
    println!("\n🔍 Step 3/3: Sending transaction...");
    println!("   Via: Regular RPC");

    let signature = rpc.send_transaction(&tx).await?;

    println!("\n{'=':.>60}", "");
    println!("✅ TRANSACTION SENT!");
    println!("   Signature: {}", signature);
    println!("   Solscan: https://solscan.io/tx/{}", signature);
    println!("   Amount: {} SOL", sol_amount as f64 / 1_000_000_000.0);
    println!("{'=':.>60}", "");

    println!("\n⏳ Waiting for confirmation...");
    tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;

    // Check status
    match rpc.get_signature_status(&signature).await? {
        Some(status) => {
            if status.err.is_some() {
                println!("❌ Transaction failed: {:?}", status.err);
            } else {
                println!("✅ Transaction confirmed!");
            }
        }
        None => {
            println!("⏳ Transaction pending (check explorer)");
        }
    }

    // Check token balance
    println!("\n🔍 Checking token balance...");
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    match rpc.get_token_account_balance(&user_ata).await {
        Ok(balance) => {
            println!("💰 Token Balance: {} tokens", balance.ui_amount.unwrap_or(0.0));
        }
        Err(e) => {
            println!("⚠️  Could not fetch balance: {}", e);
        }
    }

    println!("\n✅ Test complete!");
    Ok(())
}

fn load_wallet() -> Result<Keypair> {
    use anyhow::anyhow;

    if let Ok(private_key) = std::env::var("SOLANA_PRIVATE_KEY") {
        let bytes: Vec<u8> = serde_json::from_str(&private_key)?;
        let keypair = Keypair::from_bytes(&bytes)?;
        println!("✅ Loaded from SOLANA_PRIVATE_KEY");
        return Ok(keypair);
    }

    if let Ok(wallet_json) = std::fs::read_to_string("wallet.json") {
        let bytes: Vec<u8> = serde_json::from_str(&wallet_json)?;
        let keypair = Keypair::from_bytes(&bytes)?;
        println!("✅ Loaded from wallet.json");
        return Ok(keypair);
    }

    Err(anyhow!("No wallet found. Set SOLANA_PRIVATE_KEY or create wallet.json"))
}