# PowerShell skripta za linting i preporuke
# Usage: .\clippy.ps1 [-AllTargets] [-Fix]
#
# cargo clippy proverava kod i daje preporuke za poboljsanja.
# Koristite ovo pre commit-a za cistiji kod.

param(
    [switch]$AllTargets,
    [switch]$Fix
)

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

# Proveri da li je clippy instaliran
$clippyCheck = & $cargo clippy --version 2>&1
if ($LASTEXITCODE -ne 0) {
    Write-Host "Clippy nije instaliran!" -ForegroundColor Yellow
    Write-Host "Instaliram clippy..." -ForegroundColor Cyan
    rustup component add clippy
    if ($LASTEXITCODE -ne 0) {
        Write-Host "Neuspjesna instalacija clippy-a!" -ForegroundColor Red
        exit 1
    }
}

Write-Host "Pokretanje clippy linting-a..." -ForegroundColor Cyan
Write-Host ""

$clippyArgs = @()
if ($AllTargets) {
    $clippyArgs += "--all-targets"
    Write-Host "Proveravam sve targete (lib, bins, tests)..." -ForegroundColor Cyan
}
if ($Fix) {
    $clippyArgs += "--fix"
    Write-Host "Pokusavam automatski popraviti probleme..." -ForegroundColor Cyan
}

& $cargo clippy $clippyArgs

if ($LASTEXITCODE -eq 0) {
    Write-Host ""
    Write-Host "Clippy provera uspjesna! Kod je cist." -ForegroundColor Green
} else {
    Write-Host ""
    Write-Host "Clippy je nasao probleme. Proverite iznad." -ForegroundColor Yellow
    if (-not $Fix) {
        Write-Host "Pokusajte sa -Fix flagom: .\clippy.ps1 -Fix" -ForegroundColor Gray
    }
    exit 1
}
