# PowerShell skripta za pokretanje programa
# Usage: .\run.ps1 [release|debug]

param(
    [string]$BuildProfile = "release"
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

$ErrorActionPreference = "SilentlyContinue"

# Setup cargo path
$cargo = "$env:USERPROFILE\.cargo\bin\cargo.exe"
if (-not (Test-Path $cargo)) {
    $cargo = "cargo"
}

Write-Host "Koristim cargo: $cargo" -ForegroundColor Cyan

# Kill existing instances
Write-Host "Gasim stare procese..." -ForegroundColor Gray
Stop-Process -Name "Fullsnajperista" -Force -ErrorAction SilentlyContinue
Stop-Process -Name "cargo" -Force -ErrorAction SilentlyContinue
Start-Sleep -Seconds 1

if ($BuildProfile -eq "release") {
    Write-Host "Pokretanje release verzije..." -ForegroundColor Cyan
    & $cargo run --release
} else {
    Write-Host "Pokretanje debug verzije..." -ForegroundColor Cyan
    & $cargo run
}

$exitCode = $LASTEXITCODE

# Restore original directory
Pop-Location

exit $exitCode
