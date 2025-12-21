# PowerShell skripta za development sa auto-rebuild i auto-run
# Usage: .\watch_dev.ps1
#
# Automatski rebuilda i pokreće program na svaku promenu .rs fajlova.
# Zahteva cargo-watch: cargo install cargo-watch

param(
    [string]$Profile = "debug"  # debug ili release
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

Write-Host "🔍 Tražim Cargo..." -ForegroundColor Cyan

# Pokušaj pronaći cargo
$cargo = "$env:USERPROFILE\.cargo\bin\cargo.exe"
if (-not (Test-Path $cargo)) {
    $found = Get-Command cargo -ErrorAction SilentlyContinue
    if ($found) {
        $cargo = "cargo"
    } else {
        Write-Host "❌ Cargo nije pronaden!" -ForegroundColor Red
        Write-Host "Molimo instalirajte Rust: https://rustup.rs/" -ForegroundColor Yellow
        exit 1
    }
}

Write-Host "✅ Cargo pronaden: $cargo" -ForegroundColor Green
Write-Host ""

# Proveri da li je cargo-watch instaliran
$watchCheck = & $cargo watch --version 2>&1
if ($LASTEXITCODE -ne 0) {
    Write-Host "⚠️  cargo-watch nije instaliran!" -ForegroundColor Yellow
    Write-Host "📦 Instaliram cargo-watch..." -ForegroundColor Cyan
    Write-Host "   Ovo može potrajati nekoliko minuta..." -ForegroundColor Gray
    & $cargo install cargo-watch
    if ($LASTEXITCODE -ne 0) {
        Write-Host "❌ Neuspješna instalacija cargo-watch-a!" -ForegroundColor Red
        Write-Host "Pokušajte ručno: cargo install cargo-watch" -ForegroundColor Yellow
        exit 1
    }
    Write-Host "✅ cargo-watch instaliran!" -ForegroundColor Green
    Write-Host ""
}

$exePath = if ($Profile -eq "release") {
    "target\release\Fullsnajperista.exe"
} else {
    "target\debug\Fullsnajperista.exe"
}

Write-Host "🚀 Development Watch Mode" -ForegroundColor Cyan
Write-Host "   Profile: $Profile" -ForegroundColor Gray
Write-Host "   Pritisnite Ctrl+C za zaustavljanje" -ForegroundColor Gray
Write-Host ""

# Funkcija za pokretanje programa
function Start-App {
    param([string]$ExePath)
    
    # Ugasi stare procese
    Get-Process -Name "Fullsnajperista" -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
    Start-Sleep -Milliseconds 500
    
    if (Test-Path $ExePath) {
        Write-Host "▶️  Pokrećem program..." -ForegroundColor Green
        Start-Process -FilePath $ExePath -WindowStyle Normal
    } else {
        Write-Host "⏳ Executable još ne postoji, čekam build..." -ForegroundColor Yellow
    }
}

# Prvo build i run
Write-Host "🔨 Prvi build..." -ForegroundColor Cyan
if ($Profile -eq "release") {
    & $cargo build --release
} else {
    & $cargo build
}

if ($LASTEXITCODE -eq 0) {
    $fullExePath = Join-Path $scriptDir $exePath
    Start-App -ExePath $fullExePath
} else {
    Write-Host "❌ Build neuspješan!" -ForegroundColor Red
    Pop-Location
    exit 1
}

Write-Host ""
Write-Host "👀 Pracenje promena fajlova..." -ForegroundColor Cyan
Write-Host ""

# Watch mode - rebuild i restart na promjene
if ($Profile -eq "release") {
    & $cargo watch -x "build --release" -x "run --release"
} else {
    & $cargo watch -x build -x run
}

# Restore original directory (only reached if watch is interrupted)
Pop-Location

