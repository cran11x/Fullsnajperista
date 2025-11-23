#!/bin/bash
# package_release.sh - Package release for macOS/Linux

set -e

VERSION="${1:-0.3.0}"
PACKAGE_NAME="Fullsnajperista-v${VERSION}"
PACKAGE_DIR="release-packages/${PACKAGE_NAME}"

echo "📦 Creating release package..."
echo "   Version: $VERSION"
echo ""

# Check if release build exists
EXE_PATH="target/release/Fullsnajperista"
if [ ! -f "$EXE_PATH" ]; then
    echo "❌ Release executable not found. Run ./build_release.sh first"
    exit 1
fi

# Create package directory
echo "📁 Creating package directory..."
if [ -d "$PACKAGE_DIR" ]; then
    rm -rf "$PACKAGE_DIR"
fi
mkdir -p "$PACKAGE_DIR"

# Copy executable
echo "📦 Copying executable..."
cp "$EXE_PATH" "$PACKAGE_DIR/Fullsnajperista"
chmod +x "$PACKAGE_DIR/Fullsnajperista"

# On macOS, also copy .app if it exists
if [[ "$OSTYPE" == "darwin"* ]]; then
    APP_PATH="target/release/Fullsnajperista.app"
    if [ -d "$APP_PATH" ]; then
        echo "🍎 Copying .app bundle..."
        cp -R "$APP_PATH" "$PACKAGE_DIR/"
    fi
fi

# Create .env template
echo "📝 Creating .env.example..."
cat > "$PACKAGE_DIR/.env.example" << 'EOF'
# Pump.fun Sniper Bot Configuration
# Copy this file to .env and fill in your values

# REQUIRED: Your Solana wallet private key (base58 encoded)
SOLANA_PRIVATE_KEY=your_private_key_here

# REQUIRED: Your Helius API key
HELIUS_API_KEY=your_helius_api_key_here

# RPC Configuration (optional, defaults will be used if not set)
RPC_URL=https://mainnet.helius-rpc.com/?api-key=
WSS_URL=wss://mainnet.helius-rpc.com/?api-key=

# Trading Configuration
SOL_PRICE_USD=162.0
BUY_AMOUNT_SOL=0.015
PRIORITY_FEE=11000000
COMPUTE_UNITS=200000
SUBMISSION_MODE=helius
JITO_TIP_SOL=0.0015

# Filter Configuration
REQUIRE_SOCIALS=false
REQUIRE_TWITTER=false
MIN_SOCIALS_COUNT=0
MIN_DEV_BUY_USD=500.0
MAX_DEV_BUY_USD=1200.0
MIN_DEV_TOKENS=6
MAX_DEV_TOKENS=10

# Other Settings
ENABLE_TRACKER=true
MOCK_BUY=false
ONE_SHOT_MODE=true
EOF

# Create README
echo "📝 Creating README..."
cat > "$PACKAGE_DIR/README.txt" << EOF
# Pump.fun Sniper Bot v${VERSION}

## Installation

1. Extract all files to a folder
2. Copy .env.example to .env
3. Open .env in a text editor and fill in:
   - SOLANA_PRIVATE_KEY: Your Solana wallet private key (base58)
   - HELIUS_API_KEY: Your Helius API key
4. Adjust other settings as needed
5. Run the executable

## Requirements

- macOS 10.13+ or Linux (64-bit)
- Internet connection
- Valid Solana wallet with SOL balance
- Helius API key (get one at https://helius.dev)

## Configuration

All settings are in the .env file. Key settings:

- BUY_AMOUNT_SOL: Amount of SOL to spend per buy (default: 0.015)
- PRIORITY_FEE: Priority fee in lamports (default: 11000000)
- SUBMISSION_MODE: Transaction submission method (helius/jito/rpc/all)
- MIN_DEV_BUY_USD: Minimum dev buy in USD (default: 500)
- MAX_DEV_BUY_USD: Maximum dev buy in USD (default: 1200)

## Usage

1. Make sure your wallet has enough SOL for buys + fees
2. Run the executable
3. The GUI will open - configure settings in the Settings tab
4. Click Start to begin monitoring

## Safety

- Always test with MOCK_BUY=true first
- Start with small BUY_AMOUNT_SOL values
- Monitor your wallet balance
- Never share your SOLANA_PRIVATE_KEY

## Support

For issues and questions, please check the main repository.

## License

See the main repository for license information.
EOF

# Create quick start guide
echo "📝 Creating QUICKSTART.txt..."
cat > "$PACKAGE_DIR/QUICKSTART.txt" << 'EOF'
QUICK START GUIDE
=================

1. Copy .env.example to .env
2. Edit .env and add:
   - SOLANA_PRIVATE_KEY=your_key_here
   - HELIUS_API_KEY=your_key_here
3. Run the executable (or .app on macOS)
4. In Settings tab, configure your preferences
5. Click Start button

IMPORTANT: Test with MOCK_BUY=true first!
EOF

# Get package info
EXE_SIZE=$(du -h "$PACKAGE_DIR/Fullsnajperista" | cut -f1)
TOTAL_SIZE=$(du -sh "$PACKAGE_DIR" | cut -f1)

echo ""
echo "✅ Package created successfully!"
echo ""
echo "📦 Package location: $PACKAGE_DIR"
echo "📊 Package size: $TOTAL_SIZE"
echo "📊 Executable size: $EXE_SIZE"
echo ""
echo "📋 Package contents:"
ls -lh "$PACKAGE_DIR" | tail -n +2 | awk '{print "   - " $9 " (" $5 ")"}'
echo ""
echo "💡 Next steps:"
echo "   1. Test the package in a clean folder"
echo "   2. Create a ZIP/TAR archive for distribution"
echo "   3. Share with users (include README.txt)"

