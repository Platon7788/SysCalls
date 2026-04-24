@echo off
:: build.bat — full workspace release build.
::
:: Uses profile settings from Cargo.toml (opt-level = z, LTO, strip).
:: Prints the produced binaries on success.

setlocal
cd /d "%~dp0.."

echo [*] cargo build --workspace --release
cargo build --workspace --release
if errorlevel 1 goto :fail

echo.
echo [*] artifacts:
dir /b target\release\*.exe target\release\rsc*.dll target\release\rsc*.lib 2>nul
goto :eof

:fail
echo [!] build failed
exit /b 1
