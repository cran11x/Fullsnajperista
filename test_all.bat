@echo off
REM Batch script za pokretanje testova na Windows
echo Pokretanje testova...

REM Pokušaj pronaći cargo
where cargo >nul 2>&1
if %ERRORLEVEL% NEQ 0 (
    echo Cargo nije u PATH-u. Pokušavam pronaći Rust instalaciju...
    set CARGO_PATH=%USERPROFILE%\.cargo\bin\cargo.exe
    if exist "%CARGO_PATH%" (
        set CARGO=%CARGO_PATH%
    ) else (
        echo ERROR: Cargo nije pronađen!
        echo Molimo instalirajte Rust: https://rustup.rs/
        pause
        exit /b 1
    )
) else (
    set CARGO=cargo
)

echo Pokretanje unit testova...
%CARGO% test --lib

echo.
echo Pokretanje websocket server testova...
%CARGO% test --test websocket_server_test

echo.
echo Pokretanje mock buy testova...
%CARGO% test --test mock_buy_test

echo.
echo Testovi završeni!
pause

