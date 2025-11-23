#!/bin/bash
# build_macos.sh - Create macOS .app bundle

set -e

if [[ "$OSTYPE" != "darwin"* ]]; then
    echo "❌ This script is for macOS only!"
    exit 1
fi

echo "🍎 Creating macOS .app bundle..."

# Check if release build exists
EXE_PATH="target/release/Fullsnajperista"
if [ ! -f "$EXE_PATH" ]; then
    echo "❌ Release executable not found. Run ./build_release.sh first"
    exit 1
fi

# App bundle structure
APP_NAME="Fullsnajperista"
APP_DIR="target/release/${APP_NAME}.app"
CONTENTS_DIR="${APP_DIR}/Contents"
MACOS_DIR="${CONTENTS_DIR}/MacOS"
RESOURCES_DIR="${CONTENTS_DIR}/Resources"

# Clean up old bundle if exists
if [ -d "$APP_DIR" ]; then
    echo "🧹 Removing old .app bundle..."
    rm -rf "$APP_DIR"
fi

# Create directory structure
echo "📁 Creating .app structure..."
mkdir -p "${MACOS_DIR}"
mkdir -p "${RESOURCES_DIR}"

# Copy executable
echo "📦 Copying executable..."
cp "$EXE_PATH" "${MACOS_DIR}/${APP_NAME}"
chmod +x "${MACOS_DIR}/${APP_NAME}"

# Create Info.plist
echo "📝 Creating Info.plist..."
cat > "${CONTENTS_DIR}/Info.plist" << EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleExecutable</key>
    <string>${APP_NAME}</string>
    <key>CFBundleIdentifier</key>
    <string>com.fullsnajperista.sniper</string>
    <key>CFBundleName</key>
    <string>Pump.fun Sniper Bot</string>
    <key>CFBundleVersion</key>
    <string>0.3.0</string>
    <key>CFBundleShortVersionString</key>
    <string>0.3.0</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>LSMinimumSystemVersion</key>
    <string>10.13</string>
    <key>NSHighResolutionCapable</key>
    <true/>
    <key>NSHumanReadableCopyright</key>
    <string>Copyright © 2024</string>
    <key>CFBundleIconFile</key>
    <string>AppIcon</string>
</dict>
</plist>
EOF

# Create PkgInfo (optional but recommended)
echo "APPL????" > "${CONTENTS_DIR}/PkgInfo"

# Get app size
APP_SIZE=$(du -sh "$APP_DIR" | cut -f1)

echo ""
echo "✅ macOS .app bundle created successfully!"
echo "📦 Location: $APP_DIR"
echo "📊 Size: $APP_SIZE"
echo ""
echo "💡 You can now:"
echo "   - Double-click to run the app"
echo "   - Drag to Applications folder"
echo "   - Distribute as .app or create a .dmg"

