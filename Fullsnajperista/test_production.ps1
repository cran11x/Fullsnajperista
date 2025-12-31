# test_production.ps1 - Kompletan test suite za produkciju
# Usage: .\test_production.ps1 [-SkipBuild] [-Verbose] [-SkipClippy]

param(
    [switch]$SkipBuild,
    [switch]$Verbose,
    [switch]$SkipClippy
)

$ErrorActionPreference = "Stop"
$failed = $false
$startTime = Get-Date

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

Write-Host ""
Write-Host "🧪 PRODUCTION TEST SUITE" -ForegroundColor Cyan
Write-Host ("=" * 60) -ForegroundColor Gray
Write-Host ""

# Find Cargo
$cargo = "$env:USERPROFILE\.cargo\bin\cargo.exe"
if (-not (Test-Path $cargo)) {
    $found = Get-Command cargo -ErrorAction SilentlyContinue
    if ($found) {
        $cargo = "cargo"
    } else {
        Write-Host "❌ Cargo nije pronaden!" -ForegroundColor Red
        Write-Host "Molimo instalirajte Rust: https://rustup.rs/" -ForegroundColor Yellow
        Pop-Location
        exit 1
    }
}

Write-Host "Cargo: $cargo" -ForegroundColor Gray
Write-Host ""

# 1. Syntax Check
Write-Host "[1/7] Provera sintakse (cargo check)..." -ForegroundColor Yellow
try {
    if ($Verbose) {
        & $cargo check --all-targets 2>&1 | Out-Host
    } else {
        & $cargo check --all-targets 2>&1 | Out-Null
    }
    if ($LASTEXITCODE -ne 0) {
        Write-Host "   ❌ Sintaksna greška!" -ForegroundColor Red
        $failed = $true
    } else {
        Write-Host "   ✅ Sintaksa OK" -ForegroundColor Green
    }
} catch {
    Write-Host "   ❌ Greška pri proveri sintakse: $_" -ForegroundColor Red
    $failed = $true
}

# 2. Clippy Linting
if (-not $SkipClippy) {
    Write-Host ""
    Write-Host "[2/7] Clippy linting..." -ForegroundColor Yellow
    try {
        if ($Verbose) {
            & $cargo clippy --all-targets 2>&1 | Out-Host
        } else {
            & $cargo clippy --all-targets 2>&1 | Out-Null
        }
        if ($LASTEXITCODE -ne 0) {
            Write-Host "   ⚠️  Clippy našao probleme - proverite preporuke" -ForegroundColor Yellow
            # Don't fail on clippy warnings, just warn
        } else {
            Write-Host "   ✅ Clippy OK" -ForegroundColor Green
        }
    } catch {
        Write-Host "   ⚠️  Clippy greška (može se ignorisati): $_" -ForegroundColor Yellow
    }
} else {
    Write-Host ""
    Write-Host "[2/7] Clippy preskočen (--SkipClippy)" -ForegroundColor Gray
}

# 3. PDA Derivation Tests (CRITICAL)
Write-Host ""
Write-Host "[3/7] PDA Derivation testovi (KRITIČNO)..." -ForegroundColor Yellow
try {
    if ($Verbose) {
        & $cargo test --lib pda_derivation::tests -- --nocapture 2>&1 | Out-Host
    } else {
        & $cargo test --lib pda_derivation::tests --quiet 2>&1 | Out-Null
    }
    if ($LASTEXITCODE -ne 0) {
        Write-Host "   ❌ PDA testovi neuspešni!" -ForegroundColor Red
        $failed = $true
    } else {
        Write-Host "   ✅ PDA testovi OK" -ForegroundColor Green
    }
} catch {
    Write-Host "   ❌ Greška pri PDA testovima: $_" -ForegroundColor Red
    $failed = $true
}

# 4. Critical Module Tests
Write-Host ""
Write-Host "[4/7] Kritični moduli (Buy/Sell/Validation)..." -ForegroundColor Yellow
$criticalModules = @(
    "buy::tests",
    "sell::tests", 
    "validation::tests"
)

$criticalFailed = $false
foreach ($module in $criticalModules) {
    Write-Host "   Testing $module..." -ForegroundColor Gray -NoNewline
    try {
        & $cargo test --lib $module --quiet 2>&1 | Out-Null
        if ($LASTEXITCODE -ne 0) {
            Write-Host " ❌" -ForegroundColor Red
            $criticalFailed = $true
            $failed = $true
        } else {
            Write-Host " ✅" -ForegroundColor Green
        }
    } catch {
        Write-Host " ❌" -ForegroundColor Red
        $criticalFailed = $true
        $failed = $true
    }
}

if ($criticalFailed) {
    Write-Host "   ❌ Neki kritični testovi neuspešni!" -ForegroundColor Red
} else {
    Write-Host "   ✅ Svi kritični testovi OK" -ForegroundColor Green
}

# 5. All Unit Tests
Write-Host ""
Write-Host "[5/7] Svi unit testovi..." -ForegroundColor Yellow
try {
    if ($Verbose) {
        & $cargo test --lib -- --nocapture 2>&1 | Out-Host
    } else {
        & $cargo test --lib --quiet 2>&1 | Out-Null
    }
    if ($LASTEXITCODE -ne 0) {
        Write-Host "   ❌ Neki unit testovi neuspešni!" -ForegroundColor Red
        $failed = $true
    } else {
        Write-Host "   ✅ Svi unit testovi OK" -ForegroundColor Green
    }
} catch {
    Write-Host "   ❌ Greška pri unit testovima: $_" -ForegroundColor Red
    $failed = $true
}

# 6. Integration Tests
Write-Host ""
Write-Host "[6/7] Integration testovi..." -ForegroundColor Yellow
try {
    if ($Verbose) {
        & $cargo test --test optimization_tests -- --nocapture 2>&1 | Out-Host
    } else {
        & $cargo test --test optimization_tests --quiet 2>&1 | Out-Null
    }
    if ($LASTEXITCODE -ne 0) {
        Write-Host "   ❌ Integration testovi neuspešni!" -ForegroundColor Red
        $failed = $true
    } else {
        Write-Host "   ✅ Integration testovi OK" -ForegroundColor Green
    }
} catch {
    Write-Host "   ❌ Greška pri integration testovima: $_" -ForegroundColor Red
    $failed = $true
}

# 7. Release Build
if (-not $SkipBuild) {
    Write-Host ""
    Write-Host "[7/7] Release build..." -ForegroundColor Yellow
    try {
        if ($Verbose) {
            & $cargo build --release 2>&1 | Out-Host
        } else {
            & $cargo build --release 2>&1 | Out-Null
        }
        if ($LASTEXITCODE -ne 0) {
            Write-Host "   ❌ Release build neuspešan!" -ForegroundColor Red
            $failed = $true
        } else {
            Write-Host "   ✅ Release build OK" -ForegroundColor Green
        }
    } catch {
        Write-Host "   ❌ Greška pri release build-u: $_" -ForegroundColor Red
        $failed = $true
    }
} else {
    Write-Host ""
    Write-Host "[7/7] Release build preskočen (--SkipBuild)" -ForegroundColor Gray
}

# Final Report
$endTime = Get-Date
$duration = $endTime - $startTime

Write-Host ""
Write-Host ("=" * 60) -ForegroundColor Gray
Write-Host ""

if ($failed) {
    Write-Host "❌ TEST SUITE FAILED - Nije spremno za produkciju!" -ForegroundColor Red
    Write-Host ""
    Write-Host "Vreme izvršavanja: $($duration.TotalSeconds.ToString('F2'))s" -ForegroundColor Gray
    Pop-Location
    exit 1
} else {
    Write-Host "✅ SVI TESTOVI PROŠLI - Spremno za produkciju!" -ForegroundColor Green
    Write-Host ""
    Write-Host "Vreme izvršavanja: $($duration.TotalSeconds.ToString('F2'))s" -ForegroundColor Gray
    Write-Host ""
    Write-Host "Preporuka: Testiraj na testnet-u pre mainnet-a!" -ForegroundColor Yellow
    Pop-Location
    exit 0
}

