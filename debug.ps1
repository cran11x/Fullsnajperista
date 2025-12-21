# PowerShell skripta za debug analizu transakcija
# Usage: 
#   .\debug.ps1 <tx_signature>                    - Analizira jednu transakciju
#   .\debug.ps1 <successful_tx> <failed_tx>       - Uspoređuje dvije transakcije

param(
    [Parameter(Mandatory=$true, Position=0)]
    [string]$TxSig1,
    
    [Parameter(Mandatory=$false, Position=1)]
    [string]$TxSig2
)

# Ensure we're in the script's directory (project root)
# Handles paths with spaces correctly and prevents Cursor crashes
$scriptDir = if ($PSScriptRoot) {
    $PSScriptRoot
} elseif ($MyInvocation.MyCommand.Path) {
    Split-Path -Parent $MyInvocation.MyCommand.Path
} else {
    # Fallback: use current directory if script path is not available
    (Get-Location).Path
}

# Use -LiteralPath to handle paths with spaces correctly
if ($scriptDir) {
    Push-Location -LiteralPath $scriptDir
} else {
    Write-Host "⚠️  Warning: Could not determine script directory, using current location" -ForegroundColor Yellow
}

# Pokušaj pronaći cargo
$cargo = "$env:USERPROFILE\.cargo\bin\cargo.exe"
if (-not (Test-Path $cargo)) {
    $found = Get-Command cargo -ErrorAction SilentlyContinue
    if ($found) {
        $cargo = "cargo"
    } else {
        Write-Host "Cargo nije pronaden!" -ForegroundColor Red
        Write-Host "Molimo instalirajte Rust: https://rustup.rs/" -ForegroundColor Yellow
        exit 1
    }
}

Write-Host "Cargo pronaden: $cargo" -ForegroundColor Green
Write-Host ""

if ($TxSig2) {
    # Compare mode
    Write-Host "Debug Mode: Uspoređivanje transakcija" -ForegroundColor Cyan
    Write-Host "  Successful: $TxSig1" -ForegroundColor Green
    Write-Host "  Failed:     $TxSig2" -ForegroundColor Red
    Write-Host ""
    Write-Host "Pokretanje debug usporedbe..." -ForegroundColor Cyan
    & $cargo run --release -- debug $TxSig1 $TxSig2
} else {
    # Single transaction analysis mode
    Write-Host "Debug Mode: Analiza transakcije" -ForegroundColor Cyan
    Write-Host "  Transaction: $TxSig1" -ForegroundColor Yellow
    Write-Host ""
    Write-Host "Pokretanje analize..." -ForegroundColor Cyan
    & $cargo run --release -- debug $TxSig1
}

$exitCode = $LASTEXITCODE

# Restore original directory
Pop-Location

if ($exitCode -ne 0) {
    Write-Host ""
    Write-Host "Debug neuspješan!" -ForegroundColor Red
    exit 1
}

