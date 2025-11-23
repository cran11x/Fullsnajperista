# build_release.ps1 - Build release executables for Windows

Write-Host "🔨 Building release version..." -ForegroundColor Cyan

# Find cargo
$cargoPaths = @(
    "$env:USERPROFILE\.cargo\bin\cargo.exe",
    "C:\Users\$env:USERNAME\.cargo\bin\cargo.exe",
    "cargo.exe"
)

$cargo = $null
foreach ($path in $cargoPaths) {
    if (Test-Path $path) {
        $cargo = $path
        break
    }
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
    Write-Host "❌ Cargo not found! Please install Rust." -ForegroundColor Red
    exit 1
}

Write-Host "✅ Using cargo: $cargo" -ForegroundColor Green
Write-Host ""

# Build release
Write-Host "📦 Building release executable..." -ForegroundColor Cyan
& $cargo build --release

if ($LASTEXITCODE -eq 0) {
    $exePath = "target\release\Fullsnajperista.exe"
    if (Test-Path $exePath) {
        $fileInfo = Get-Item $exePath
        $size = [math]::Round($fileInfo.Length / 1MB, 2)
        Write-Host ""
        Write-Host "✅ Build successful!" -ForegroundColor Green
        Write-Host "📦 Executable: $exePath" -ForegroundColor Green
        Write-Host "📊 Size: $size MB" -ForegroundColor Green
        Write-Host "📅 Build date: $($fileInfo.LastWriteTime)" -ForegroundColor Cyan
    } else {
        Write-Host "❌ Executable not found at: $exePath" -ForegroundColor Red
        exit 1
    }
} else {
    Write-Host "❌ Build failed!" -ForegroundColor Red
    exit 1
}

Write-Host ""
Write-Host "💡 Tip: Run .\package_release.ps1 to create a distributable package" -ForegroundColor Yellow

