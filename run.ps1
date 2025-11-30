# PowerShell skripta za pokretanje programa
# Usage: .\run.ps1 [release|debug]

param(
    [string]$BuildProfile = "release"
)

Write-Host "Trazim Cargo..." -ForegroundColor Cyan

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

if ($BuildProfile -eq "release") {
    Write-Host "Pokretanje release verzije..." -ForegroundColor Cyan
    & $cargo run --release
} else {
    Write-Host "Pokretanje debug verzije..." -ForegroundColor Cyan
    & $cargo run
}

