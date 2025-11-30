# PowerShell skripta za build projekta
# Usage: .\build.ps1 [release|debug]

param(
    [string]$Profile = "release"
)

Write-Host "🔍 Tražim Cargo..." -ForegroundColor Cyan

# Pokušaj pronaći cargo
$cargo = "$env:USERPROFILE\.cargo\bin\cargo.exe"
if (-not (Test-Path $cargo)) {
    $found = Get-Command cargo -ErrorAction SilentlyContinue
    if ($found) {
        $cargo = "cargo"
    } else {
        Write-Host "❌ Cargo nije pronađen!" -ForegroundColor Red
        Write-Host "Molimo instalirajte Rust: https://rustup.rs/" -ForegroundColor Yellow
        exit 1
    }
}

Write-Host "✅ Cargo pronađen: $cargo" -ForegroundColor Green
Write-Host ""

if ($Profile -eq "release") {
    Write-Host "🚀 Building release verziju..." -ForegroundColor Cyan
    & $cargo build --release
} else {
    Write-Host "🚀 Building debug verziju..." -ForegroundColor Cyan
    & $cargo build
}

if ($LASTEXITCODE -eq 0) {
    Write-Host ""
    Write-Host "✅ Build uspješan!" -ForegroundColor Green
} else {
    Write-Host ""
    Write-Host "❌ Build neuspješan!" -ForegroundColor Red
    exit 1
}

