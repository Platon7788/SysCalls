@echo off
:: clean.bat — `cargo clean` + wipe C example artifacts.
:: Does NOT touch:
::   * db/canonical.toml — checked in (Path A), regenerate with refresh.bat
::   * %APPDATA%\rsc\cache — the PDB cache

setlocal
cd /d "%~dp0.."

echo [*] cargo clean
cargo clean

if exist target\c_examples rmdir /s /q target\c_examples

echo [*] done
