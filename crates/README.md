# crates/

The workspace is intentionally split into two "factors" by role. When
you consume SysCalls you only need the **Core** side; the **Tools** side
is a maintenance workflow that rebuilds the baked DB.

## Core — what consumers import

These three crates form the distributable runtime. Depend on just one
(`rsc-runtime`) and you get direct NT syscalls; depend on the bundle
through `rsc-c` if you're writing a C/C++ program. Neither touches the
network at build time; everything they need is pre-baked in
`db/canonical.toml`.

| Crate | Type | What consumer sees |
|---|---|---|
| `rsc-runtime` | `lib`, `#![no_std]` | `rsc_runtime::syscalls::NtXxx`, types, constants, `NtStatus`, resolver |
| `rsc-codegen` | `proc-macro` | Invisible — expanded at compile-time when `rsc-runtime` is built |
| `rsc-c` | `staticlib + cdylib` | `rsc.dll`, `rsc.lib`, `include/rsc.h` with `RscNt*` functions |

**Zero runtime dependencies** from `rsc-runtime`. `rsc-codegen` is a
proc-macro (build-time only). `rsc-c` adds nothing at runtime either.

## Tools — maintainer / CI workflow

You only need these if:
* Microsoft shipped a new Windows cumulative update
* You want first-class coverage of a different Windows version
* You updated the vendored phnt submodule
* You're auditing the baked DB

| Crate | Binary | What it produces |
|---|---|---|
| `rsc-collector` | `rsc-collector.exe` | `db/auto/<build>.toml` — SSNs + stubs from current host's `ntdll.pdb` |
| `rsc-types` | `rsc-types.exe` | `db/phnt/phnt.toml` — typed signatures from vendored phnt |
| `rsc-cli` | `rsc.exe` | `db/canonical.toml` via `rsc merge`; plus `verify / diff / stats` |

Trigger the full refresh with `scripts\refresh.bat` at repo root.
`git diff db/` after, commit if the delta looks right.

## Which `Cargo.toml` do I edit?

| I want to... | Edit this |
|---|---|
| Add a fresh NT syscall override | `db/overrides.toml` |
| Use a different phnt revision | `vendor/phnt` submodule + `rsc-types` re-run |
| Fix a type mapping (phnt → Rust) | `crates/rsc-types/src/normalizer.rs` |
| Change how stubs are emitted | `crates/rsc-codegen/src/lib.rs` |
| Change PEB walk / resolver | `crates/rsc-runtime/src/peb.rs` / `table.rs` |
| Change C header layout | `crates/rsc-c/build.rs` |
| Add a new `rsc` CLI subcommand | `crates/rsc-cli/src/commands/` |
