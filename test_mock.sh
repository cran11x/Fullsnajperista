#!/bin/bash
export MOCK_BUY=true
export MOCK_SELL=true
export ONE_SHOT_MODE=false
export ENABLE_AUTO_SELL=true
export STOP_LOSS_PERCENT=5.0
export TAKE_PROFIT_MC_SOL=5.0
export MONITOR_INTERVAL_SEC=2

echo "🧪 Testing mock buy/sell functionality..."
echo "   MOCK_BUY: $MOCK_BUY"
echo "   MOCK_SELL: $MOCK_SELL"
echo "   ENABLE_AUTO_SELL: $ENABLE_AUTO_SELL"
echo ""

./target/release/SNIPER 2>&1 | tee test_output.log &
BOT_PID=$!

echo "Bot started with PID: $BOT_PID"
echo "Waiting 15 seconds to see if it detects tokens..."
sleep 15

echo ""
echo "Checking if mock_buys file was created..."
ls -lh mock_buys_*.jsonl 2>/dev/null | tail -1

echo ""
echo "Stopping bot..."
kill $BOT_PID 2>/dev/null
wait $BOT_PID 2>/dev/null

echo ""
echo "✅ Test completed"
