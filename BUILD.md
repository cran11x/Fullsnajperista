# Build Instructions

This document explains how to build and package the Pump.fun Sniper Bot for distribution.

## Quick Start

### Windows
```powershell
# Build release executable
.\build_release.ps1

# Create distributable package
.\package_release.ps1
```

### macOS/Linux
```bash
# Make scripts executable
chmod +x build_release.sh build_macos.sh package_release.sh

# Build release executable
./build_release.sh

# Create macOS .app bundle (macOS only)
./build_macos.sh

# Create distributable package
./package_release.sh
```

## Build Process

### 1. Release Build

The release build creates an optimized executable:

**Windows:**
- Output: `target/release/Fullsnajperista.exe`
- Size: ~10-20 MB (depending on dependencies)

**macOS/Linux:**
- Output: `target/release/Fullsnajperista`
- Size: ~10-20 MB

### 2. macOS .app Bundle

On macOS, you can create a proper `.app` bundle:

```bash
./build_macos.sh
```

This creates:
- `target/release/Fullsnajperista.app`

The `.app` bundle includes:
- Executable in `Contents/MacOS/`
- `Info.plist` with app metadata
- Proper structure for macOS distribution

### 3. Package Creation

The package script creates a distributable folder with:
- Executable (or .app on macOS)
- `.env.example` template
- `README.txt` documentation
- `QUICKSTART.txt` quick start guide

**Windows:**
```powershell
.\package_release.ps1
# Creates: release-packages/Fullsnajperista-v0.3.0/
```

**macOS/Linux:**
```bash
./package_release.sh
# Creates: release-packages/Fullsnajperista-v0.3.0/
```

## Manual Build

If you prefer to build manually:

### Windows
```powershell
cargo build --release
# Executable: target\release\Fullsnajperista.exe
```

### macOS/Linux
```bash
cargo build --release
# Executable: target/release/Fullsnajperista
```

## Cross-Compilation

### Building for Windows from macOS/Linux

```bash
# Install Windows target
rustup target add x86_64-pc-windows-gnu

# Build
cargo build --release --target x86_64-pc-windows-gnu
# Output: target/x86_64-pc-windows-gnu/release/Fullsnajperista.exe
```

**Note:** You'll need a cross-compilation toolchain. For Windows, you may need `mingw-w64`.

### Building for macOS from Linux

```bash
# Install macOS targets
rustup target add x86_64-apple-darwin
rustup target add aarch64-apple-darwin

# Build
cargo build --release --target x86_64-apple-darwin
# Output: target/x86_64-apple-darwin/release/Fullsnajperista
```

**Note:** macOS cross-compilation requires additional setup and may not work on all systems. It's recommended to build on macOS.

## Distribution

### Creating ZIP Archives

**Windows:**
```powershell
Compress-Archive -Path "release-packages\Fullsnajperista-v0.3.0\*" -DestinationPath "Fullsnajperista-v0.3.0-Windows.zip"
```

**macOS/Linux:**
```bash
cd release-packages
zip -r Fullsnajperista-v0.3.0-macOS.zip Fullsnajperista-v0.3.0/
# or
tar -czf Fullsnajperista-v0.3.0-Linux.tar.gz Fullsnajperista-v0.3.0/
```

### Creating macOS DMG (Optional)

For macOS, you can create a `.dmg` disk image file for easy distribution:

```bash
# Make script executable
chmod +x create_dmg.sh

# Create DMG (requires .app bundle first)
./build_macos.sh
./create_dmg.sh
```

This creates: `target/release/Fullsnajperista-v0.3.0.dmg`

The DMG includes:
- The `.app` bundle
- Applications folder symlink (for drag & drop)
- README.txt with installation instructions
- Properly configured window layout

**Alternative using create-dmg tool:**

If you prefer using the `create-dmg` tool:

```bash
# Install create-dmg if needed
brew install create-dmg

# Create DMG
create-dmg \
  --volname "Pump.fun Sniper Bot" \
  --window-pos 200 120 \
  --window-size 600 400 \
  --icon-size 100 \
  --icon "Fullsnajperista.app" 175 190 \
  --hide-extension "Fullsnajperista.app" \
  --app-drop-link 425 190 \
  "Fullsnajperista-v0.3.0.dmg" \
  "target/release/Fullsnajperista.app"
```

## Code Signing (Optional)

### macOS Code Signing

For distribution outside the App Store, you may want to code sign:

```bash
# Sign the app
codesign --deep --force --verify --verbose --sign "Developer ID Application: Your Name" Fullsnajperista.app

# Notarize (requires Apple Developer account)
xcrun notarytool submit Fullsnajperista.app --keychain-profile "AC_PASSWORD" --wait
```

### Windows Code Signing

For Windows, you can sign with a code signing certificate:

```powershell
signtool sign /f certificate.pfx /p password /t http://timestamp.digicert.com Fullsnajperista.exe
```

## Troubleshooting

### Build Fails

1. **Missing dependencies:** Make sure all Rust dependencies are installed
2. **Out of memory:** Release builds can use significant memory
3. **Linker errors:** Check that you have the correct toolchain installed

### macOS .app Won't Run

1. **Gatekeeper:** Right-click → Open (first time)
2. **Permissions:** Check System Preferences → Security & Privacy
3. **Code signing:** Unsigned apps may not run on some macOS versions

### Executable Too Large

The release build includes all dependencies. To reduce size:
- Use `strip` on the binary (already enabled in release profile)
- Consider UPX compression (may trigger antivirus warnings)
- Remove debug symbols (already done with `strip = true`)

## Release Profile

The `Cargo.toml` includes an optimized release profile:

```toml
[profile.release]
opt-level = 3      # Maximum optimization
lto = true        # Link-time optimization
codegen-units = 1 # Better optimization
strip = true      # Remove debug symbols
```

This creates the smallest, fastest executable possible.

## Version Management

To build with a specific version:

**Windows:**
```powershell
.\package_release.ps1 -Version "0.4.0"
```

**macOS/Linux:**
```bash
./package_release.sh 0.4.0
```

## CI/CD Integration

You can integrate these scripts into CI/CD pipelines:

**GitHub Actions Example:**
```yaml
- name: Build Release
  run: cargo build --release

- name: Package Release
  run: ./package_release.sh
```

**GitLab CI Example:**
```yaml
build:
  script:
    - cargo build --release
    - ./package_release.sh
  artifacts:
    paths:
      - release-packages/
```

