@echo off
:: test.bat — run the full workspace test suite.

setlocal
cd /d "%~dp0.."

echo [*] cargo test --workspace
cargo test --workspace
exit /b %errorlevel%
