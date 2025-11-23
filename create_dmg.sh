#!/bin/bash
# create_dmg.sh - Create macOS .dmg disk image for distribution

set -e

if [[ "$OSTYPE" != "darwin"* ]]; then
    echo "❌ This script is for macOS only!"
    exit 1
fi

echo "💿 Creating macOS .dmg disk image..."

# Check if .app bundle exists
APP_NAME="Fullsnajperista"
APP_DIR="target/release/${APP_NAME}.app"
DMG_NAME="${APP_NAME}-v0.3.0"
DMG_PATH="target/release/${DMG_NAME}.dmg"
TEMP_DMG="target/release/${DMG_NAME}-temp.dmg"
VOLUME_NAME="Pump.fun Sniper Bot"

if [ ! -d "$APP_DIR" ]; then
    echo "❌ .app bundle not found. Run ./build_macos.sh first"
    exit 1
fi

# Clean up old DMG if exists
if [ -f "$DMG_PATH" ]; then
    echo "🧹 Removing old .dmg..."
    rm -f "$DMG_PATH"
fi
if [ -f "$TEMP_DMG" ]; then
    rm -f "$TEMP_DMG"
fi

# Create temporary directory for DMG contents
TEMP_DIR=$(mktemp -d)
echo "📁 Creating temporary directory: $TEMP_DIR"

# Copy .app to temp directory
echo "📦 Copying .app bundle..."
cp -R "$APP_DIR" "$TEMP_DIR/"

# Create Applications symlink (for drag & drop)
echo "🔗 Creating Applications symlink..."
ln -s /Applications "$TEMP_DIR/Applications"

# Optional: Create README
cat > "$TEMP_DIR/README.txt" << EOF
Pump.fun Sniper Bot v0.3.0

INSTALLATION:
1. Drag ${APP_NAME}.app to the Applications folder
2. Open Applications folder
3. Double-click ${APP_NAME}.app to launch

Note: If macOS blocks the app, go to:
System Settings > Privacy & Security > Allow apps downloaded from: App Store and identified developers

For more information, visit the project repository.
EOF

# Calculate size needed (app size + 50MB overhead)
APP_SIZE=$(du -sk "$APP_DIR" | cut -f1)
DMG_SIZE=$((APP_SIZE + 51200))  # Add 50MB overhead

echo "📊 App size: $(du -sh "$APP_DIR" | cut -f1)"
echo "💾 Creating DMG (size: ${DMG_SIZE}KB)..."

# Create DMG using hdiutil
hdiutil create -srcfolder "$TEMP_DIR" \
    -volname "$VOLUME_NAME" \
    -fs HFS+ \
    -fsargs "-c c=64,a=16,e=16" \
    -format UDRW \
    -size ${DMG_SIZE}k \
    "$TEMP_DMG" || {
    echo "❌ Failed to create DMG"
    rm -rf "$TEMP_DIR"
    exit 1
}

# Mount the DMG
echo "🔌 Mounting DMG..."
MOUNT_DIR=$(hdiutil attach -readwrite -noverify -noautoopen "$TEMP_DMG" | \
    egrep '^/dev/' | sed 1q | awk '{print $3}')

# Wait a moment for mount
sleep 2

# Set volume icon and background (optional - requires icon file)
# If you have an icon, uncomment these:
# cp "icon.icns" "$MOUNT_DIR/.VolumeIcon.icns"
# SetFile -a C "$MOUNT_DIR"

# Set window view options
echo "🎨 Configuring DMG window..."
echo '
   tell application "Finder"
     tell disk "'"$VOLUME_NAME"'"
           open
           set current view of container window to icon view
           set toolbar visible of container window to false
           set statusbar visible of container window to false
           set the bounds of container window to {400, 100, 920, 420}
           set viewOptions to the icon view options of container window
           set arrangement of viewOptions to not arranged
           set icon size of viewOptions to 72
           delay 1
           set position of item "'"$APP_NAME.app"'" of container window to {160, 205}
           set position of item "Applications" of container window to {360, 205}
           set position of item "README.txt" of container window to {260, 100}
           close
           open
           update without registering applications
           delay 2
     end tell
   end tell
' | osascript || echo "⚠️  Could not configure DMG window (continuing anyway)"

# Unmount
echo "🔌 Unmounting DMG..."
hdiutil detach "$MOUNT_DIR"

# Convert to read-only compressed DMG
echo "🗜️  Compressing DMG..."
hdiutil convert "$TEMP_DMG" \
    -format UDZO \
    -imagekey zlib-level=9 \
    -o "$DMG_PATH" || {
    echo "❌ Failed to compress DMG"
    rm -f "$TEMP_DMG"
    rm -rf "$TEMP_DIR"
    exit 1
}

# Clean up
rm -f "$TEMP_DMG"
rm -rf "$TEMP_DIR"

# Get final DMG size
DMG_SIZE_MB=$(du -sh "$DMG_PATH" | cut -f1)

echo ""
echo "✅ DMG created successfully!"
echo "📦 Location: $DMG_PATH"
echo "📊 Size: $DMG_SIZE_MB"
echo ""
echo "💡 You can now:"
echo "   - Distribute the .dmg file"
echo "   - Users can double-click to mount and drag app to Applications"
echo "   - Upload to GitHub Releases or distribute via other means"

