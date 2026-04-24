@echo off
:: refresh.bat — update the baked canonical.toml for the current host.
::
:: Use this when:
::   * Microsoft ships a new Windows cumulative update (new UBR)
::   * You move to a different Windows major version
::   * `db/phnt/` submodule was bumped
::
:: Steps:
::   1. `rsc-collector --force`  →  fresh db/auto/<build>.toml
::   2. `rsc-types`              →  fresh db/phnt/phnt.toml
::   3. `rsc merge`              →  rebuild db/canonical.toml (unions
::                                  every *.toml under db/auto)
::   4. `rsc verify`             →  sanity checks
::   5. `rsc stats`              →  show what we ended up with
::
:: After a successful run, `git diff db/` shows what changed — commit if
:: the delta looks right.

setlocal
cd /d "%~dp0.."

echo === [1/5] collect current-Windows snapshot
cargo run --release -p rsc-collector -- --force %* || exit /b 1

echo.
echo === [2/5] re-parse phnt headers
cargo run --release -p rsc-types || exit /b 1

echo.
echo === [3/5] merge (union every db/auto/*.toml into canonical)
cargo run --release --bin rsc -- merge || exit /b 1

echo.
echo === [4/5] verify canonical integrity
cargo run --release --bin rsc -- verify || exit /b 1

echo.
echo === [5/5] dashboard
cargo run --release --bin rsc -- stats

echo.
echo [*] refresh done. Review `git diff db/` and commit if the delta is expected.
