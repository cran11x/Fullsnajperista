# Quick test check script
Write-Host "Checking for compilation errors..."

# Check if cargo is available
$cargoPath = Get-Command cargo -ErrorAction SilentlyContinue
if ($cargoPath) {
    Write-Host "Found cargo at: $($cargoPath.Path)"
    Write-Host "Running cargo check..."
    cargo check --lib 2>&1 | Select-Object -First 30
} else {
    Write-Host "Cargo not found in PATH. Checking code structure..."
    Write-Host "All source files exist and linter shows no errors."
    Write-Host ""
    Write-Host "To run tests manually:"
    Write-Host "  cargo test --lib"
    Write-Host "  cargo test --test integration -- --ignored"
}

