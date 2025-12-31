# Test script for optimization features
$cargo = if (Test-Path "$env:USERPROFILE\.cargo\bin\cargo.exe") { 
    "$env:USERPROFILE\.cargo\bin\cargo.exe" 
} else { 
    "cargo" 
}

Write-Host "Running bonding curve cache tests..." -ForegroundColor Cyan
& $cargo test --lib bonding_curve::tests -- --nocapture

Write-Host "`nRunning batch operation tests..." -ForegroundColor Cyan
& $cargo test --lib bot_core::tests::test_batch -- --nocapture

Write-Host "`nAll optimization tests completed!" -ForegroundColor Green

