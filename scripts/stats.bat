@echo off
:: stats.bat — print a dashboard on db/canonical.toml.

setlocal
cd /d "%~dp0.."

cargo run --release --bin rsc -- stats
exit /b %errorlevel%
