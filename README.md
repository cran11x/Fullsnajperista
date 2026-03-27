# SNIPER

Solana pump.fun token sniper bot with a native desktop GUI. Detects new token launches in real-time via WebSocket, applies 50+ configurable filters, executes buy transactions with MEV protection, and manages positions with automatic sell triggers.

## Architecture

```
WebSocket (Helius)          Solana RPC
      |                         |
      v                         v
  Detection ──> Filtering ──> Buy TX ──> Submit (Helius/Jito/RPC)
                                              |
                                              v
                                     Position Tracking
                                       |          |
                                       v          v
                                  Auto-Sell    GUI Display
                                (SL/TP/BE)    (7 tabs)
```

**Pipeline:**

1. **Detect** — WebSocket listener receives pump.fun `Create` instructions (new token launches)
2. **Filter** — 50+ criteria: dev buy range, socials, creator history, brand matching, ticker rules
3. **Buy** — Build pump.fun buy instruction with slippage + priority fee
4. **Submit** — Send via Helius Sender, Jito MEV bundles, or direct RPC (configurable)
5. **Track** — Monitor bonding curve for live PnL, peak MC, breakeven state
6. **Sell** — Auto-sell on stop loss, take profit, breakeven trigger, or dead coin timeout

## Prerequisites

- **Rust nightly** (managed via `rust-toolchain.toml` — installs automatically)
- **Solana wallet** — Base58-encoded 64-byte keypair
- **Helius API key** — for RPC + WebSocket access (a default key is included but rate-limited)

## Setup

Create a `.env` file in the project root:

```env
# Required
SOLANA_PRIVATE_KEY=<your-base58-keypair>

# Optional (defaults are provided)
HELIUS_API_KEY=<your-helius-key>
RPC_URL=https://mainnet.helius-rpc.com/?api-key=<key>
WSS_URL=wss://mainnet.helius-rpc.com/?api-key=<key>
BUY_AMOUNT_SOL=0.015
SLIPPAGE_PERCENT=200
PRIORITY_FEE=11000000
COMPUTE_UNITS=200000
JITO_TIP=100000
```

If no `.env` is present, the GUI still starts with defaults. You can configure everything from the Settings tab.

## Build & Run

```bash
# Release build (optimized, recommended for trading)
cargo run --release

# Debug build (faster compile, shows "DEBUG BUILD" in title)
cargo run
```

The GUI window opens at 1400x900 pixels.

## GUI Tabs

| Tab | Purpose |
|-----|---------|
| **Dashboard** | Aggregate metrics: tokens detected, filtered, bought, success rate, timing stats |
| **Feed** | Real-time stream of detected tokens with metadata and filter results |
| **Filtered** | Tokens that were rejected, with the specific filter reason for each |
| **Buys** | All positions (active + closed) with entry MC, current MC, PnL |
| **Positions** | Advanced position view: peak PnL tracking, breakeven state, manual sell controls |
| **Settings** | Live-editable configuration: all buy/sell/filter/network parameters |
| **Buy Sniper** | Manual buy/sell by mint address (for targeted trades) |

## How to Use

1. **Launch** — `cargo run --release` opens the SNIPER window
2. **Configure** — Go to the **Settings** tab:
   - Set your wallet private key (or use `.env`)
   - Set RPC/WSS URLs and Helius API key
   - Set buy amount (SOL), slippage, priority fee, Jito tip
   - Choose submission mode: Helius / Jito / RPC / All
   - Enable/disable filters (socials, dev buy range, brand matching, etc.)
   - Configure auto-sell: stop loss %, take profit MC, breakeven protection
3. **Start** — Click the **Start** button to begin listening for new tokens
4. **Monitor** — Watch the **Feed** tab for detected tokens flowing in
5. **Review** — Check **Filtered** tab to understand why tokens are rejected (tune filters)
6. **Track** — **Buys** and **Positions** tabs show live PnL for active positions
7. **Manual trade** — Use **Buy Sniper** tab to buy/sell a specific token by mint address
8. **Stop** — Click **Stop** to pause the bot (positions continue to be tracked)

## Configuration Reference

### Trading

| Parameter | Default | Description |
|-----------|---------|-------------|
| `buy_amount_sol` | 0.015 | SOL to spend per buy |
| `slippage_percent` | 200 | Slippage tolerance (200 = 100% slippage). Range: 100-500 |
| `priority_fee` | 11,000,000 | Lamports per compute unit for priority |
| `compute_units` | 200,000 | Compute budget per transaction |
| `jito_tip` | 100,000 | Lamports tip for Jito MEV bundles |
| `submission_mode` | All | Helius, Jito, RPC, or All |

### Auto-Sell Triggers

| Parameter | Default | Description |
|-----------|---------|-------------|
| `enable_auto_sell` | true | Master switch for auto-selling |
| `stop_loss_percent` | configurable | Sell if PnL drops below -X% |
| `take_profit_mc_sol` | configurable | Sell if MC reaches X SOL |
| `enable_breakeven` | true | Breakeven protection system |
| `breakeven_arm_mc_usd` | configurable | MC (USD) at which breakeven arms |
| `breakeven_buffer_percent` | configurable | Buffer above entry before triggering sell |
| `enable_dead_coin_sell` | true | Sell if no activity for 60+ seconds |

### Filters

| Category | Examples |
|----------|----------|
| **Dev Buy** | min/max SOL the creator bought |
| **Socials** | Require Twitter, website, Telegram, Discord |
| **Twitter Type** | Account vs community, username length |
| **Brand Matching** | Name matches website, symbol matches Twitter, brand score thresholds |
| **Creator History** | Min/max tokens previously launched (DAS API) |
| **Ticker Rules** | Length, case (uppercase/lowercase), name length |
| **Blacklist/Whitelist** | Block specific tokens/creators, or only allow specific ones |

### Mock Mode

Enable `mock_buy` and `mock_sell` in Settings to simulate trades without spending SOL. Useful for testing filter configurations and understanding the token flow.

## CLI Commands

```bash
# Analyze a buy transaction
cargo run -- debug <transaction-signature>

# Analyze a sell transaction
cargo run -- debug sell <transaction-signature>

# Compare two transactions (successful vs failed)
cargo run -- debug <successful-sig> <failed-sig>

# Test SOL price fetching
cargo run -- test-sol-price
```

## Companion Service: pump-token-count-api

A separate Axum HTTP API in `pump-token-count-api/` that caches creator token counts:

- Listens to Helius WebSocket for new token events
- Caches creator -> token count in Redis
- Endpoint: `GET /api/creator/{pubkey}`
- Background refresh from Apify dataset

## Tools

- **`tools/analyze_snapshots.py`** — Parse position snapshot JSONL files into reports:
  ```bash
  python tools/analyze_snapshots.py snapshots_2026-01-17.jsonl
  python tools/analyze_snapshots.py snapshots_2026-01-17.jsonl --mint <address>
  ```

## Project Structure

```
src/
  main.rs                  Entry point, CLI args, .env loading, GUI init
  bot_core.rs              Main bot loop: detect -> filter -> buy -> track -> sell
  config.rs                100+ configurable parameters with env var loading
  constants.rs             Pump.fun program IDs, Jito/Helius endpoints
  buy.rs                   Build pump.fun buy instruction
  sell.rs                  Build pump.fun sell instruction
  detection.rs             Extract mint/bonding curve from creation TX
  filters.rs               50+ buy/sell decision filters
  validation.rs            Pre-flight checks (balance, TX size, fees)
  websocket.rs             WebSocket protocol parsing
  jito.rs                  Jito MEV bundle submission (4 endpoints)
  helius.rs                Helius Sender fast TX submission
  socials.rs               Token metadata + social link fetching (DAS/IPFS)
  das_check.rs             Creator token count via DAS API
  metrics.rs               Performance tracking + timing histograms
  errors.rs                Error types + categorization
  health.rs                Bot health checks
  rate_limiter.rs          RPC rate limiting
  pda_derivation.rs        Derive pump.fun PDAs
  token_logger.rs          JSONL buy event logging
  tracking_logger.rs       Position snapshot logging
  utils.rs                 Shared HTTP client, SOL price cache
  wallet.rs                Load keypair from .env
  debug.rs                 Transaction analysis CLI tools
  account_subscription.rs  Subscribe to bonding curve account updates
  accounts/
    bonding_curve.rs       MC calculation, token pricing from AMM reserves
    tracker.rs             Position tracking: PnL, peak, breakeven, sell reason
    global.rs              Global account parsing
    history_tracker.rs     MC/price history for charts
    seen_tokens.rs         Duplicate detection cache
  gui/
    mod.rs                 GuiApp struct, shared state, event loop
    components.rs          Reusable UI components
    events.rs              TokenEvent, BotControl message types
    tabs/
      dashboard.rs         Metrics display
      feed.rs              Real-time token detection feed
      filtered.rs          Rejected tokens + filter reasons
      buys.rs              Position list
      positions.rs         Advanced PnL tracking + manual controls
      settings.rs          Live config editor (2,200 lines)
      buy_sniper.rs        Manual buy/sell by mint address
```
