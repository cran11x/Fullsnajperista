#!/bin/bash

# Auto-Sell Test Configuration
# Postavlja niske threshold-e za testiranje auto-sell funkcionalnosti

echo "╔════════════════════════════════════════════════════════════════╗"
echo "║        Auto-Sell Test Mode - Low Thresholds                    ║"
echo "╚════════════════════════════════════════════════════════════════╝"
echo ""

# Export test environment variables
export STOP_LOSS_PERCENT=5.0          # 5% stop loss (umesto 30%)
export TAKE_PROFIT_MC_SOL=5.0         # 5 SOL take profit (umesto 175 SOL)
export ENABLE_AUTO_SELL=true
export MONITOR_INTERVAL_SEC=2         # Check every 2 seconds (umesto 5)

echo "✅ Test configuration loaded:"
echo "   STOP_LOSS_PERCENT: $STOP_LOSS_PERCENT%"
echo "   TAKE_PROFIT_MC_SOL: $TAKE_PROFIT_MC_SOL SOL"
echo "   MONITOR_INTERVAL_SEC: $MONITOR_INTERVAL_SEC"
echo ""
echo "⚠️  WARNING: These are TEST values - tokens will sell quickly!"
echo "   Use MOCK_BUY=true for safe testing without real transactions"
echo ""

# Check if MOCK_BUY is set
if [ -z "$MOCK_BUY" ]; then
    echo "💡 Tip: Set MOCK_BUY=true to test without real transactions"
    echo ""
fi

# Run bot with test config
echo "🚀 Starting bot with test configuration..."
echo ""

cd /Users/daniscavkic/Desktop/fullsniper
./target/release/SNIPER

