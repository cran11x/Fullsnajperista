# PowerShell script za pokretanje DAS check testova
# Usage: .\test_das_check.ps1

Write-Host "🔍 Tražim Rust/Cargo..." -ForegroundColor Cyan

# Pokušaj pronaći cargo u uobičajenim lokacijama
$cargoPaths = @(
    "$env:USERPROFILE\.cargo\bin\cargo.exe",
    "C:\Users\$env:USERNAME\.cargo\bin\cargo.exe",
    "$env:ProgramFiles\Rust stable MSVC 1.78\bin\cargo.exe",
    "cargo.exe"
)

$cargo = $null
foreach ($path in $cargoPaths) {
    if (Test-Path $path) {
        $cargo = $path
        break
    }
    # Pokušaj pronaći u PATH-u
    try {
        $found = Get-Command "cargo" -ErrorAction SilentlyContinue
        if ($found) {
            $cargo = "cargo"
            break
        }
    } catch {
        continue
    }
}

if (-not $cargo) {
    Write-Host "❌ Cargo nije pronađen!" -ForegroundColor Red
    Write-Host "Molimo instalirajte Rust: https://rustup.rs/" -ForegroundColor Yellow
    Write-Host "Ili dodajte Cargo u PATH." -ForegroundColor Yellow
    exit 1
}

Write-Host "✅ Cargo pronađen: $cargo" -ForegroundColor Green
Write-Host ""

Write-Host "🚀 Pokretanje DAS check testova..." -ForegroundColor Cyan
Write-Host ""

# Pokreni unit testove iz das_check.rs modula
Write-Host "📋 Pokretanje unit testova iz src/das_check.rs..." -ForegroundColor Cyan
& $cargo test --lib das_check 2>&1

Write-Host ""
Write-Host "📋 Pokretanje integration testova iz tests/das_check_test.rs..." -ForegroundColor Cyan
& $cargo test --test das_check_test 2>&1

Write-Host ""
Write-Host "Testovi zavrseni!" -ForegroundColor Green

