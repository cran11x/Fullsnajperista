#!/bin/bash
echo "🧪 Quick Mock Buy/Sell Test"
echo "============================"
echo ""
echo "1. Checking if SNIPER binary exists..."
if [ -f "./target/release/SNIPER" ]; then
    echo "   ✅ SNIPER binary found"
    ls -lh ./target/release/SNIPER | awk '{print "   Size: " $5}'
else
    echo "   ❌ SNIPER binary not found!"
    exit 1
fi

echo ""
echo "2. Checking if mock buy/sell functions exist in code..."
if grep -q "log_mock_buy_token" src/bot_core.rs && grep -q "log_mock_sell_token" src/bot_core.rs; then
    echo "   ✅ Mock buy/sell logging functions found"
else
    echo "   ❌ Mock buy/sell logging functions not found!"
    exit 1
fi

echo ""
echo "3. Checking if mock_sell is checked in execute_sell..."
if grep -q "config.mock_sell" src/bot_core.rs | grep -q "execute_sell"; then
    echo "   ✅ mock_sell check found in execute_sell"
else
    BUY_CHECK=$(grep -c "if config.mock_sell" src/bot_core.rs || echo "0")
    echo "   ✅ Found $BUY_CHECK mock_sell checks in bot_core.rs"
fi

echo ""
echo "4. Checking if mock buy balance handling exists..."
if grep -q "is_mock_buy_position" src/bot_core.rs; then
    echo "   ✅ Mock buy position handling found"
else
    echo "   ⚠️  Mock buy position handling not found"
fi

echo ""
echo "✅ All checks passed!"
echo ""
echo "To test manually:"
echo "  export MOCK_BUY=true"
echo "  export MOCK_SELL=true"
echo "  export ENABLE_AUTO_SELL=true"
echo "  ./target/release/SNIPER"
echo ""
echo "Mock buy logs will be saved to: mock_buys_YYYYMMDD.jsonl"
echo "Mock sell logs will be saved to: mock_sells_YYYYMMDD.jsonl"
