@echo off
:: merge.bat — build canonical.toml out of auto + phnt + overrides.

setlocal
cd /d "%~dp0.."

echo [*] cargo run --release --bin rsc -- merge
cargo run --release --bin rsc -- merge
if errorlevel 1 exit /b %errorlevel%

echo.
echo [*] cargo run --release --bin rsc -- verify
cargo run --release --bin rsc -- verify
exit /b %errorlevel%
