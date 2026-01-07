#!/bin/bash

# Auto-Sell Verification Script
# Proverava da li auto-sell fixovi rade ispravno

echo "╔════════════════════════════════════════════════════════════════╗"
echo "║           Auto-Sell Verification Check                         ║"
echo "╚════════════════════════════════════════════════════════════════╝"
echo ""

# Colors
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Find tracker.json file (check multiple locations)
TRACKER_FILE=""
if [ -f "tracker.json" ]; then
    TRACKER_FILE="tracker.json"
elif [ -n "$(ls -t target/debug/deps/tracker_*.json target/release/tracker_*.json 2>/dev/null | head -1)" ]; then
    TRACKER_FILE=$(ls -t target/debug/deps/tracker_*.json target/release/tracker_*.json 2>/dev/null | head -1)
    echo "Using tracker file: $TRACKER_FILE"
fi

if [ -z "$TRACKER_FILE" ]; then
    echo -e "${YELLOW}⚠️  tracker.json not found${NC}"
    echo "   This is normal if bot hasn't been run yet or no buys occurred"
    echo "   Run the bot first to generate tracker.json"
    echo ""
    echo "   Continuing with other checks..."
    TRACKER_FILE=""
fi

echo "1. Checking for positions marked as sold without signature..."
if [ -n "$TRACKER_FILE" ]; then
    SOLD_WITHOUT_SIG=$(cat "$TRACKER_FILE" 2>/dev/null | jq -r '.buys[]? | select(.sold == true and (.sell_signature == null or .sell_signature == "")) | .mint' 2>/dev/null)
else
    SOLD_WITHOUT_SIG=""
fi

if [ -z "$SOLD_WITHOUT_SIG" ]; then
    echo -e "${GREEN}✅ All sold positions have signatures${NC}"
else
    echo -e "${RED}❌ Found positions marked as sold without signature:${NC}"
    echo "$SOLD_WITHOUT_SIG"
    echo ""
    echo "This indicates a problem - positions should only be marked as sold"
    echo "after transaction confirmation."
fi

echo ""
echo "2. Checking for detailed sell logs in recent output..."
if [ -f "output.log" ] || [ -f "logs.txt" ]; then
    LOG_FILE=""
    if [ -f "output.log" ]; then
        LOG_FILE="output.log"
    elif [ -f "logs.txt" ]; then
        LOG_FILE="logs.txt"
    fi
    
    if [ -n "$LOG_FILE" ]; then
        SELL_EXECUTED=$(grep -c "🎯 SELL EXECUTED" "$LOG_FILE" 2>/dev/null || echo "0")
        TX_CONFIRMED=$(grep -c "✅ TRANSACTION CONFIRMED" "$LOG_FILE" 2>/dev/null || echo "0")
        AUTO_SKIPPED=$(grep -c "AUTO-SELL SKIPPED" "$LOG_FILE" 2>/dev/null || echo "0")
        TX_NOT_CONFIRMED=$(grep -c "Transaction NOT confirmed" "$LOG_FILE" 2>/dev/null || echo "0")
        
        echo "   Detailed sell logs found: $SELL_EXECUTED"
        echo "   Transactions confirmed: $TX_CONFIRMED"
        echo "   Auto-sells skipped (validation): $AUTO_SKIPPED"
        echo "   Transactions not confirmed: $TX_NOT_CONFIRMED"
        
        if [ "$SELL_EXECUTED" -gt 0 ]; then
            echo -e "${GREEN}✅ Detailed logging is working${NC}"
        else
            echo -e "${YELLOW}⚠️  No detailed sell logs found (may be normal if no sells occurred)${NC}"
        fi
        
        if [ "$AUTO_SKIPPED" -gt 0 ]; then
            echo -e "${GREEN}✅ Data validation is working (skipped invalid data)${NC}"
        fi
    fi
else
    echo -e "${YELLOW}⚠️  No log file found (output.log or logs.txt)${NC}"
    echo "   Run the bot and check logs manually"
fi

echo ""
echo "3. Checking tracker.json structure..."
if [ -n "$TRACKER_FILE" ]; then
    TRACKER_VALID=$(cat "$TRACKER_FILE" 2>/dev/null | jq -e '.buys' > /dev/null 2>&1 && echo "yes" || echo "no")
else
    TRACKER_VALID="no"
fi

if [ "$TRACKER_VALID" = "yes" ]; then
    TOTAL_BUYS=$(cat "$TRACKER_FILE" 2>/dev/null | jq '.buys | length' 2>/dev/null || echo "0")
    SOLD_COUNT=$(cat "$TRACKER_FILE" 2>/dev/null | jq '[.buys[]? | select(.sold == true)] | length' 2>/dev/null || echo "0")
    SOLD_WITH_SIG=$(cat "$TRACKER_FILE" 2>/dev/null | jq '[.buys[]? | select(.sold == true and .sell_signature != null and .sell_signature != "")] | length' 2>/dev/null || echo "0")
    
    echo "   Total buys: $TOTAL_BUYS"
    echo "   Sold positions: $SOLD_COUNT"
    echo "   Sold with signature: $SOLD_WITH_SIG"
    
    if [ "$SOLD_COUNT" -gt 0 ] && [ "$SOLD_WITH_SIG" -eq "$SOLD_COUNT" ]; then
        echo -e "${GREEN}✅ All sold positions have confirmed signatures${NC}"
    elif [ "$SOLD_COUNT" -eq 0 ]; then
        echo -e "${YELLOW}⚠️  No sold positions yet (normal if no sells occurred)${NC}"
    else
        MISSING=$((SOLD_COUNT - SOLD_WITH_SIG))
        echo -e "${RED}❌ $MISSING sold position(s) missing signature${NC}"
    fi
else
    echo -e "${RED}❌ tracker.json is invalid or empty${NC}"
fi

echo ""
echo "╔════════════════════════════════════════════════════════════════╗"
echo "║                    Verification Complete                        ║"
echo "╚════════════════════════════════════════════════════════════════╝"
echo ""
echo "For detailed verification guide, see: AUTOSELL_VERIFICATION.md"

