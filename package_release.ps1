# package_release.ps1 - Package release with .env template and documentation

param(
    [string]$Version = "0.3.0"
)

$packageName = "Fullsnajperista-v$Version"
$packageDir = "release-packages\$packageName"

Write-Host "📦 Creating release package..." -ForegroundColor Cyan
Write-Host "   Version: $Version" -ForegroundColor Cyan
Write-Host ""

# Check if release build exists
$exePath = "target\release\Fullsnajperista.exe"
if (-not (Test-Path $exePath)) {
    Write-Host "❌ Release executable not found. Run .\build_release.ps1 first" -ForegroundColor Red
    exit 1
}

# Create package directory
Write-Host "📁 Creating package directory..." -ForegroundColor Cyan
if (Test-Path $packageDir) {
    Remove-Item -Recurse -Force $packageDir
}
New-Item -ItemType Directory -Force -Path $packageDir | Out-Null

# Copy executable
Write-Host "📦 Copying executable..." -ForegroundColor Cyan
Copy-Item $exePath "$packageDir\Fullsnajperista.exe"

# Create .env template
Write-Host "📝 Creating .env.example..." -ForegroundColor Cyan
@"
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
"@ | Out-File "$packageDir\.env.example" -Encoding UTF8

# Create README
Write-Host "📝 Creating README..." -ForegroundColor Cyan
@"
# Pump.fun Sniper Bot v$Version

## Installation

1. Extract all files to a folder
2. Copy `.env.example` to `.env`
3. Open `.env` in a text editor and fill in:
   - SOLANA_PRIVATE_KEY: Your Solana wallet private key (base58)
   - HELIUS_API_KEY: Your Helius API key
4. Adjust other settings as needed
5. Run `Fullsnajperista.exe`

## Requirements

- Windows 10/11 (64-bit)
- Internet connection
- Valid Solana wallet with SOL balance
- Helius API key (get one at https://helius.dev)

## Configuration

All settings are in the `.env` file. Key settings:

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
"@ | Out-File "$packageDir\README.txt" -Encoding UTF8

# Create quick start guide
Write-Host "📝 Creating QUICKSTART.txt..." -ForegroundColor Cyan
@"
QUICK START GUIDE
=================

1. Copy .env.example to .env
2. Edit .env and add:
   - SOLANA_PRIVATE_KEY=your_key_here
   - HELIUS_API_KEY=your_key_here
3. Run Fullsnajperista.exe
4. In Settings tab, configure your preferences
5. Click Start button

IMPORTANT: Test with MOCK_BUY=true first!
"@ | Out-File "$packageDir\QUICKSTART.txt" -Encoding UTF8

# Get package info
$exeSize = [math]::Round((Get-Item "$packageDir\Fullsnajperista.exe").Length / 1MB, 2)
$totalSize = [math]::Round((Get-ChildItem $packageDir -Recurse | Measure-Object -Property Length -Sum).Sum / 1MB, 2)

Write-Host ""
Write-Host "✅ Package created successfully!" -ForegroundColor Green
Write-Host ""
Write-Host "📦 Package location: $packageDir" -ForegroundColor Cyan
Write-Host "📊 Package size: $totalSize MB" -ForegroundColor Cyan
Write-Host "📊 Executable size: $exeSize MB" -ForegroundColor Cyan
Write-Host ""
Write-Host "📋 Package contents:" -ForegroundColor Yellow
Get-ChildItem $packageDir | ForEach-Object { 
    $size = if ($_.PSIsContainer) { "<DIR>" } else { "$([math]::Round($_.Length / 1KB, 2)) KB" }
    Write-Host "   - $($_.Name) ($size)" -ForegroundColor Gray
}
Write-Host ""
Write-Host "💡 Next steps:" -ForegroundColor Yellow
Write-Host "   1. Test the package in a clean folder" -ForegroundColor Gray
Write-Host "   2. Create a ZIP archive for distribution" -ForegroundColor Gray
Write-Host "   3. Share with users (include README.txt)" -ForegroundColor Gray

