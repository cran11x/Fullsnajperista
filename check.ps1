# PowerShell skripta za brzu proveru sintakse (bez build-a)
# Usage: .\check.ps1 [-AllTargets]
# 
# cargo check je 10-50x brze od cargo build jer samo proverava sintaksu
# i tipove bez generisanja executable-a. Koristite ovo tokom developmenta.

param(
    [switch]$AllTargets
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
Write-Host "Brza provera sintakse (cargo check)..." -ForegroundColor Cyan
Write-Host "   Ovo je mnogo brze od punog build-a!" -ForegroundColor Gray
Write-Host ""

if ($AllTargets) {
    Write-Host "Proveravam sve targete (lib, bins, tests)..." -ForegroundColor Cyan
    & $cargo check --all-targets
} else {
    & $cargo check
}

if ($LASTEXITCODE -eq 0) {
    Write-Host ""
    Write-Host "Provera uspjesna! Nema gresaka." -ForegroundColor Green
    Write-Host "Za punu kompilaciju koristite: .\build.ps1" -ForegroundColor Gray
} else {
    Write-Host ""
    Write-Host "Pronadene greske!" -ForegroundColor Red
    exit 1
}
