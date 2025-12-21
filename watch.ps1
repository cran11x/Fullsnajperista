# PowerShell skripta za auto-check na promene fajlova
# Usage: .\watch.ps1 [check|clippy|test]
#
# Automatski pokrece cargo check/clippy/test na svaku promenu .rs fajlova.
# Zahteva cargo-watch: cargo install cargo-watch

param(
    [string]$Mode = "check"
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

Write-Host "Tražim Cargo..." -ForegroundColor Cyan

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

# Proveri da li je cargo-watch instaliran
$watchCheck = & $cargo watch --version 2>&1
if ($LASTEXITCODE -ne 0) {
    Write-Host "cargo-watch nije instaliran!" -ForegroundColor Yellow
    Write-Host "Instaliram cargo-watch..." -ForegroundColor Cyan
    Write-Host "   Ovo moze potrajati nekoliko minuta..." -ForegroundColor Gray
    & $cargo install cargo-watch
    if ($LASTEXITCODE -ne 0) {
        Write-Host "Neuspjesna instalacija cargo-watch-a!" -ForegroundColor Red
        Write-Host "Pokusajte rucno: cargo install cargo-watch" -ForegroundColor Yellow
        exit 1
    }
    Write-Host "cargo-watch instaliran!" -ForegroundColor Green
    Write-Host ""
}

Write-Host "Pracenje promena fajlova..." -ForegroundColor Cyan
Write-Host "   Režim: $Mode" -ForegroundColor Gray
Write-Host "   Pritisnite Ctrl+C za zaustavljanje" -ForegroundColor Gray
Write-Host ""

switch ($Mode.ToLower()) {
    "check" {
        Write-Host "Auto-check na promene (cargo check)..." -ForegroundColor Cyan
        & $cargo watch -x check
    }
    "clippy" {
        Write-Host "Auto-clippy na promene..." -ForegroundColor Cyan
        & $cargo watch -x clippy
    }
    "test" {
        Write-Host "Auto-test na promene..." -ForegroundColor Cyan
        & $cargo watch -x test
    }
    default {
        Write-Host "Nepoznat rezim: $Mode" -ForegroundColor Red
        Write-Host "Dostupni rezimi: check, clippy, test" -ForegroundColor Yellow
        Pop-Location
        exit 1
    }
}

# Restore original directory (only reached if watch is interrupted)
Pop-Location
