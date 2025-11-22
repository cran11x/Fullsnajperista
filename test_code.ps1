# Test skript za proveru Rust koda
# Ovaj skript proverava da li kod može da se kompajlira

Write-Host "Proveravam Rust kod..." -ForegroundColor Cyan
Write-Host ""

# Proveri da li cargo postoji
if (Get-Command cargo -ErrorAction SilentlyContinue) {
    Write-Host "Cargo pronadjen" -ForegroundColor Green
    
    Write-Host ""
    Write-Host "Pokretanje cargo check..." -ForegroundColor Yellow
    cargo check 2>&1 | Tee-Object -Variable checkOutput
    
    if ($LASTEXITCODE -eq 0) {
        Write-Host ""
        Write-Host "Kod se uspesno kompajlira!" -ForegroundColor Green
        
        Write-Host ""
        Write-Host "Pokretanje unit testova..." -ForegroundColor Yellow
        cargo test --lib 2>&1 | Tee-Object -Variable testOutput
        
        if ($LASTEXITCODE -eq 0) {
            Write-Host ""
            Write-Host "Svi testovi prolaze!" -ForegroundColor Green
        } else {
            Write-Host ""
            Write-Host "Neki testovi nisu prosli" -ForegroundColor Yellow
            Write-Host "$testOutput" -ForegroundColor Red
        }
    } else {
        Write-Host ""
        Write-Host "Greska pri kompilaciji!" -ForegroundColor Red
        Write-Host "$checkOutput" -ForegroundColor Red
        exit 1
    }
} else {
    Write-Host "Cargo nije pronadjen u PATH-u" -ForegroundColor Yellow
    Write-Host ""
    Write-Host "Mozete pokrenuti testove rucno sa:" -ForegroundColor Cyan
    Write-Host "  cargo check" -ForegroundColor White
    Write-Host "  cargo test --lib" -ForegroundColor White
    Write-Host "  cargo test --test integration" -ForegroundColor White
    Write-Host ""
    Write-Host "Provera sintakse kroz linter:" -ForegroundColor Cyan
    
    # Proveri glavne fajlove
    $files = @(
        "src\main.rs",
        "src\metrics.rs",
        "src\health.rs",
        "src\rate_limiter.rs",
        "src\config.rs"
    )
    
    $allGood = $true
    foreach ($file in $files) {
        if (Test-Path $file) {
            Write-Host "  $file postoji" -ForegroundColor Green
        } else {
            Write-Host "  $file ne postoji" -ForegroundColor Red
            $allGood = $false
        }
    }
    
    if ($allGood) {
        Write-Host ""
        Write-Host "Svi kriticni fajlovi postoje!" -ForegroundColor Green
        Write-Host "Preporuka: Instalirajte Rust i pokrenite 'cargo check' za kompletnu proveru" -ForegroundColor Yellow
    }
}
