# PowerShell script za pokretanje testova
# Usage: .\run_tests.ps1 [test_name]

param(
    [string]$TestName = ""
)

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

# Provjeri postoji li .env fajl
$envFile = ".env"
if (Test-Path $envFile) {
    Write-Host "✅ .env fajl pronađen" -ForegroundColor Green
    # Ucitaj .env varijable
    Get-Content $envFile | ForEach-Object {
        $line = $_.Trim()
        if ($line -and -not $line.StartsWith("#") -and $line.Contains("=")) {
            $parts = $line.Split("=", 2)
            if ($parts.Length -eq 2) {
                $key = $parts[0].Trim()
                $value = $parts[1].Trim().Trim('"').Trim("'")
                if ($key -and $value) {
                    [Environment]::SetEnvironmentVariable($key, $value, "Process")
                }
            }
        }
    }
} else {
    Write-Host "⚠️  .env fajl nije pronađen" -ForegroundColor Yellow
}

Write-Host ""
Write-Host "🚀 Pokretanje testova..." -ForegroundColor Cyan
Write-Host ""

if ($TestName -eq "") {
    # Pokreni sve testove
    Write-Host "📋 Pokretanje svih unit testova..." -ForegroundColor Cyan
    & $cargo test --lib 2>&1
    
    Write-Host ""
    Write-Host "📋 Pokretanje websocket server testova..." -ForegroundColor Cyan
    & $cargo test --test websocket_server_test 2>&1
    
    Write-Host ""
    Write-Host "📋 Pokretanje mock buy testova..." -ForegroundColor Cyan
    & $cargo test --test mock_buy_test 2>&1
} else {
    # Pokreni specifičan test
    Write-Host "📋 Pokretanje testa: $TestName" -ForegroundColor Cyan
    & $cargo test --test websocket_server_test $TestName -- --nocapture 2>&1
}

Write-Host ""
Write-Host "Testovi zavrseni!" -ForegroundColor Green

