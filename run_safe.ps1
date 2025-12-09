# PowerShell skripta za sigurno pokretanje programa bez crash-a Cursora
# Output se preusmjerava u fajl da izbjegnemo serialization error

param(
    [string]$BuildProfile = "release"
)

# Ensure we're in the script's directory (project root)
$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
Push-Location $scriptDir

$ErrorActionPreference = "Continue"

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

# Create output file with timestamp
$timestamp = Get-Date -Format "yyyyMMdd_HHmmss"
$outputFile = "cargo_output_$timestamp.log"

Write-Host "Output se cuva u: $outputFile" -ForegroundColor Yellow
Write-Host "Pokretanje $BuildProfile verzije..." -ForegroundColor Cyan

# Run cargo and redirect all output to file AND console
if ($BuildProfile -eq "release") {
    & $cargo run --release 2>&1 | Tee-Object -FilePath $outputFile
} else {
    & $cargo run 2>&1 | Tee-Object -FilePath $outputFile
}

$exitCode = $LASTEXITCODE

Write-Host "`nExit code: $exitCode" -ForegroundColor $(if ($exitCode -eq 0) { "Green" } else { "Red" })
Write-Host "Puni output je u: $outputFile" -ForegroundColor Gray

# Restore original directory
Pop-Location

exit $exitCode

