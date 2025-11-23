#!/bin/bash
# build_release.sh - Build release executables for macOS/Linux

set -e

echo "🔨 Building release version..."

# Check if cargo is available
if ! command -v cargo &> /dev/null; then
    echo "❌ Cargo not found! Please install Rust."
    exit 1
fi

echo "✅ Using cargo: $(which cargo)"
echo ""

# Build release
echo "📦 Building release executable..."
cargo build --release

if [ $? -eq 0 ]; then
    EXE_PATH="target/release/Fullsnajperista"
    if [ -f "$EXE_PATH" ]; then
        SIZE=$(du -h "$EXE_PATH" | cut -f1)
        MOD_TIME=$(stat -f "%Sm" "$EXE_PATH" 2>/dev/null || stat -c "%y" "$EXE_PATH" 2>/dev/null || echo "unknown")
        echo ""
        echo "✅ Build successful!"
        echo "📦 Executable: $EXE_PATH"
        echo "📊 Size: $SIZE"
        echo "📅 Build date: $MOD_TIME"
        
        # On macOS, optionally create .app bundle
        if [[ "$OSTYPE" == "darwin"* ]]; then
            echo ""
            echo "🍎 macOS detected. Run ./build_macos.sh to create .app bundle"
        fi
    else
        echo "❌ Executable not found at: $EXE_PATH"
        exit 1
    fi
else
    echo "❌ Build failed!"
    exit 1
fi

echo ""
echo "💡 Tip: Run ./package_release.sh to create a distributable package"

