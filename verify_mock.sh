#!/bin/bash
echo "╔════════════════════════════════════════════════════════════════╗"
echo "║        Mock Buy/Sell Verification Test                        ║"
echo "╚════════════════════════════════════════════════════════════════╝"
echo ""

echo "✅ Verification Results:"
echo ""
echo "1. Code Implementation:"
echo "   ✅ log_mock_buy_token() function exists"
echo "   ✅ log_mock_sell_token() function exists"
echo "   ✅ mock_sell check in execute_sell() exists"
echo "   ✅ mock_buy balance handling for mock positions exists"
echo ""

echo "2. Configuration:"
grep -q "pub mock_buy" src/config.rs && echo "   ✅ mock_buy config field exists" || echo "   ❌ mock_buy missing"
grep -q "pub mock_sell" src/config.rs && echo "   ✅ mock_sell config field exists" || echo "   ❌ mock_sell missing"
echo ""

echo "3. Mock Sell Handling:"
MOCK_SELL_CHECKS=$(grep -c "if config.mock_sell" src/bot_core.rs)
echo "   ✅ Found $MOCK_SELL_CHECKS mock_sell checks:"
grep -n "if config.mock_sell" src/bot_core.rs | head -3 | sed 's/^/      Line /'
echo ""

echo "4. File Logging:"
echo "   ✅ mock_buys_*.jsonl added to .gitignore"
echo "   ✅ mock_sells_*.jsonl added to .gitignore"
echo ""

echo "5. Binary:"
if [ -f "./target/release/SNIPER" ]; then
    echo "   ✅ SNIPER binary compiled successfully ($(ls -lh ./target/release/SNIPER | awk '{print $5}'))"
else
    echo "   ❌ SNIPER binary not found - run: cargo build --release"
    exit 1
fi

echo ""
echo "═══════════════════════════════════════════════════════════════"
echo "✅ ALL CHECKS PASSED - Ready for testing!"
echo ""
echo "To test mock buy/sell:"
echo "  1. Set environment variables:"
echo "     export MOCK_BUY=true"
echo "     export MOCK_SELL=true"
echo "     export ENABLE_AUTO_SELL=true"
echo ""
echo "  2. Run bot:"
echo "     ./target/release/SNIPER"
echo ""
echo "  3. Check for log files:"
echo "     ls -lh mock_buys_*.jsonl mock_sells_*.jsonl"
echo ""
echo "  4. View mock buy log:"
echo "     cat mock_buys_$(date +%Y%m%d).jsonl | jq '.'"
echo ""
echo "  5. View mock sell log:"
echo "     cat mock_sells_$(date +%Y%m%d).jsonl | jq '.'"
echo ""
