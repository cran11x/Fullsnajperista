#!/bin/bash

# Auto-Sell Test Mode with MOCK_BUY
# Testira auto-sell bez stvarnih transakcija

echo "╔════════════════════════════════════════════════════════════════╗"
echo "║     Auto-Sell Test Mode - MOCK (Safe Testing)                 ║"
echo "╚════════════════════════════════════════════════════════════════╝"
echo ""

# Export test environment variables with MOCK mode
export MOCK_BUY=true                   # Ne šalje stvarne transakcije
export MOCK_SELL=true                  # Ne šalje stvarne sell transakcije
export STOP_LOSS_PERCENT=5.0           # 5% stop loss
export TAKE_PROFIT_MC_SOL=5.0          # 5 SOL take profit
export ENABLE_AUTO_SELL=true
export MONITOR_INTERVAL_SEC=2          # Check every 2 seconds

echo "✅ Safe test configuration loaded:"
echo "   MOCK_BUY: $MOCK_BUY (no real transactions)"
echo "   MOCK_SELL: $MOCK_SELL (no real sell transactions)"
echo "   STOP_LOSS_PERCENT: $STOP_LOSS_PERCENT%"
echo "   TAKE_PROFIT_MC_SOL: $TAKE_PROFIT_MC_SOL SOL"
echo "   MONITOR_INTERVAL_SEC: $MONITOR_INTERVAL_SEC"
echo ""
echo "✅ Safe mode: Bot will simulate buys/sells without real transactions"
echo "   You can test auto-sell logic without risking SOL"
echo ""

# Run bot with test config
echo "🚀 Starting bot in MOCK mode..."
echo ""

cd /Users/daniscavkic/Desktop/fullsniper
./target/release/SNIPER

