# PowerShell skripta za pokretanje programa
# Usage: .\run.ps1 [release|debug]

param(
    [string]$BuildProfile = "release"
)

# Ensure we're in the script's directory (project root)
$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
Push-Location $scriptDir

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
