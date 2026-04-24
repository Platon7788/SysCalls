@echo off
:: build_c_example.bat — builds examples/c/basic.c against the release
:: rsc.lib + rsc.h, then runs it. Requires MSVC `cl.exe` (vcvarsall x64).

setlocal
cd /d "%~dp0.."

where cl >nul 2>nul
if errorlevel 1 (
    echo [!] cl.exe not in PATH. Run from a "x64 Native Tools Command Prompt for VS".
    exit /b 1
)

if not exist target\release\rsc.lib (
    echo [*] rsc.lib missing; running `cargo build --workspace --release` first.
    cargo build --workspace --release || exit /b 1
)

set OUT=target\c_examples
if not exist %OUT% mkdir %OUT%

echo [*] compiling examples\c\basic.c
cl /nologo /W3 /O2 examples\c\basic.c ^
    /I crates\rsc-c\include ^
    /Fo:%OUT%\basic.obj ^
    /Fe:%OUT%\basic.exe ^
    /link target\release\rsc.lib ntdll.lib kernel32.lib advapi32.lib
if errorlevel 1 exit /b 1

echo.
echo [*] running %OUT%\basic.exe
%OUT%\basic.exe
exit /b %errorlevel%
