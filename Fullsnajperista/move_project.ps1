# PowerShell skripta za premještanje projekta na putanju bez razmaka
# Pokreni kao Administrator: powershell -ExecutionPolicy Bypass -File move_project.ps1

Write-Host "Premještanje projekta na putanju bez razmaka..." -ForegroundColor Cyan
Write-Host ""

$sourcePath1 = "C:\Users\danis\Desktop\novi stari snajper\Fullsnajperista"
$destPath1 = "C:\Users\danis\Desktop\Fullsnajperista"

$sourcePath2 = "C:\Users\danis\Desktop\novi stari snajper\pump-token-count-api"
$destPath2 = "C:\Users\danis\Desktop\pump-token-count-api"

$oldFolder = "C:\Users\danis\Desktop\novi stari snajper"

# Provjeri da li source postoji
if (-not (Test-Path $sourcePath1)) {
    Write-Host "❌ Source folder ne postoji: $sourcePath1" -ForegroundColor Red
    exit 1
}

# Provjeri da li destination već postoji
if (Test-Path $destPath1) {
    Write-Host "⚠️  Destination folder već postoji: $destPath1" -ForegroundColor Yellow
    $response = Read-Host "Želiš li prepisati? (y/n)"
    if ($response -ne "y") {
        Write-Host "Prekidam..." -ForegroundColor Yellow
        exit 0
    }
    Remove-Item -Path $destPath1 -Force -Recurse
}

# Premjesti Fullsnajperista
Write-Host "Premještam Fullsnajperista..." -ForegroundColor Yellow
try {
    Move-Item -Path $sourcePath1 -Destination $destPath1 -Force
    Write-Host "✅ Fullsnajperista premješten" -ForegroundColor Green
} catch {
    Write-Host "❌ Greška pri premještanju Fullsnajperista: $_" -ForegroundColor Red
    exit 1
}

# Premjesti pump-token-count-api ako postoji
if (Test-Path $sourcePath2) {
    Write-Host "Premještam pump-token-count-api..." -ForegroundColor Yellow
    try {
        if (Test-Path $destPath2) {
            Remove-Item -Path $destPath2 -Force -Recurse
        }
        Move-Item -Path $sourcePath2 -Destination $destPath2 -Force
        Write-Host "✅ pump-token-count-api premješten" -ForegroundColor Green
    } catch {
        Write-Host "⚠️  Greška pri premještanju pump-token-count-api: $_" -ForegroundColor Yellow
    }
}

# Obriši stari folder ako je prazan
if (Test-Path $oldFolder) {
    try {
        $items = Get-ChildItem -Path $oldFolder -Force
        if ($items.Count -eq 0) {
            Remove-Item -Path $oldFolder -Force -Recurse
            Write-Host "✅ Stari folder obrisan" -ForegroundColor Green
        } else {
            Write-Host "⚠️  Stari folder nije prazan, ostavljam ga" -ForegroundColor Yellow
        }
    } catch {
        Write-Host "⚠️  Ne mogu obrisati stari folder: $_" -ForegroundColor Yellow
    }
}

Write-Host ""
Write-Host "✅ Premještanje završeno!" -ForegroundColor Green
Write-Host "Nova lokacija: $destPath1" -ForegroundColor Cyan
Write-Host ""
Write-Host "Sada otvori Cursor i otvori folder: $destPath1" -ForegroundColor Yellow

