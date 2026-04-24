# scripts/

Convenience batch files wrapping the common developer workflows. Run from
any working directory — each script `cd`-s to the repo root before doing
anything.

## Consumer workflow

| Script | What it does |
|---|---|
| `build.bat` | `cargo build --workspace --release` + lists produced `target/release/*.{exe,dll,lib}` |
| `test.bat`  | `cargo test --workspace` |
| `clean.bat` | `cargo clean` + wipe `target/c_examples/`. **Does NOT** delete `db/canonical.toml` (checked in, Path A) nor `%APPDATA%\rsc\cache` (PDB cache) |

## Maintainer DB workflow

| Script | What it does |
|---|---|
| `refresh.bat` | Full baked-DB refresh: `rsc-collector --force` → `rsc-types` → `rsc merge` → `rsc verify` → `rsc stats`. Run after a Windows update or phnt submodule bump. Review `git diff db/` + commit if the delta is expected. |
| `merge.bat`   | Re-emit `db/canonical.toml` (union of every `db/auto/*.toml` + phnt + overrides), then `rsc verify`. Use when only `overrides.toml` or `phnt.toml` changed — skips the expensive collect step. |
| `stats.bat`   | Dashboard on `db/canonical.toml` — counts per category / source layer / Windows build. |

## C/C++ sanity

| Script | What it does |
|---|---|
| `build_c_example.bat` | Build `examples/c/basic.c` against release `rsc.lib` via `cl.exe`, then run. Invoke from "x64 Native Tools Command Prompt for VS" (cl.exe in PATH). Auto-runs `cargo build --workspace --release` if `rsc.lib` is missing. |

## One-liners that don't need a wrapper

If you just need one tool invocation, call cargo directly:

```cmd
cargo run --release -p rsc-collector -- --force       :: single PDB snapshot
cargo run --release -p rsc-types                       :: re-parse phnt
cargo run --release --bin rsc -- diff <from> <to>      :: diff two Windows builds
cargo run --release --bin rsc -- verify                :: standalone verify
```
