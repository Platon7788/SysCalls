# SysCalls (RSC — RustSysCall)

[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](./LICENSE)
[![Rust 1.88+](https://img.shields.io/badge/rust-1.88%2B-orange.svg)](https://www.rust-lang.org)
[![Windows](https://img.shields.io/badge/platform-Windows%20x64%20%7C%20x86%2FWoW64-informational.svg)](#)

Self-contained direct-NT-syscall library for Windows, written in Rust. Ground truth is pulled from `ntdll.pdb` (Microsoft Symbol Server), typed signatures come from [`phnt`](https://github.com/winsiderss/phnt), and the runtime is a `#![no_std]` crate with **zero runtime dependencies**, driven by a `proc-macro` expander.

> **Status — v1.0**. 509 NT functions (Win10 19045 + Win11 26200 unioned),
> random-JUMPER dispatch through `ntdll`, x64 native + x86/WoW64 both real.
> See [`Docs/USAGE.md`](./Docs/USAGE.md) to get started in 30 seconds.

## TL;DR — use it from a Rust project

`db/canonical.toml` is **checked into the repo** (Path A — baked distribution) — no network or extra steps on consumer build.

```toml
# your Cargo.toml
[dependencies]
rsc-runtime = { git = "https://github.com/Platon7788/SysCalls", package = "rsc-runtime" }
```

```rust
use rsc_runtime::{syscalls::*, constants::*, types::*};

unsafe {
    let mut base: *mut core::ffi::c_void = core::ptr::null_mut();
    let mut size: usize = 0x1000;
    NtAllocateVirtualMemory(
        NT_CURRENT_PROCESS,
        &mut base,
        0,
        &mut size,
        MEM_COMMIT | MEM_RESERVE,
        PAGE_READWRITE,
    );
}
```

**509 NT functions** available. Release binary ~ 280 KB. See [`examples/consumer-template/`](./examples/consumer-template/) for a copy-paste starter.

## Key properties

- `#![no_std]` runtime, **ZERO** runtime deps (hard invariant)
- JUMPER mode always on — `syscall` instruction is executed *from inside ntdll*, hiding the return address from stack-trace heuristics
- WoW64 runtime-detected via `fs:[0xC0]` (x86 gate)
- Hash obfuscation: ROR8 + `RSC_SEED = 0x52534300`
- MSRV: Rust 1.88 (stable `#[unsafe(naked)]` + `core::arch::naked_asm!`)
- Reproducible: `phnt` as pinned submodule
- Multi-Windows coverage via union merge — canonical.toml is the *superset* of every harvested build

## Crate layout

The workspace is split into two factors (see [`crates/README.md`](./crates/README.md)):

### Core — what a consumer imports

| Crate | Purpose |
|---|---|
| [`rsc-runtime`](./crates/rsc-runtime) | `#![no_std]` lib: PEB walking, hash table, naked syscall stubs |
| [`rsc-codegen`](./crates/rsc-codegen) | proc-macro `rsc_syscall!` — expands one naked stub per NT fn |
| [`rsc-c`](./crates/rsc-c)             | staticlib + cdylib + auto-generated `rsc.h` for C/C++ consumers |

### Tools — maintainer / CI only

| Crate | Purpose |
|---|---|
| [`rsc-collector`](./crates/rsc-collector) | CLI: downloads `ntdll.pdb`, disassembles stubs → `db/auto/<build>.toml` |
| [`rsc-types`](./crates/rsc-types)         | CLI: parses phnt headers → `db/phnt/phnt.toml` |
| [`rsc-cli`](./crates/rsc-cli)             | Unified `rsc` binary: `merge / verify / diff / stats` |

## Build

```bash
# Core (consumer build — no tools needed)
cargo build --release -p rsc-runtime
cargo build --release -p rsc-c        # C bindings: rsc.dll + rsc.lib + rsc.h

# Everything
cargo build --release --workspace
cargo test  --release --workspace

# Refresh DB on the maintainer box (only when supported Windows set changes)
scripts/refresh.bat
```

Release profile uses `opt-level = "z"`, LTO, `panic = "abort"`, `strip = true` — tuned for injection-friendly binaries.

## Covered Windows builds

| Family | Build | Source |
|---|---|---|
| Windows 10 | 19045.6466 | `db/auto/10_19045_6466.toml` |
| Windows 11 | 26200.8246 | `db/auto/11_26200_8246.toml` |

17 functions are exclusive to Win11 (`IoRing*` family, `CpuPartition*`, `ThreadStateChange`, `NtSetEventEx`, `NtReadVirtualMemoryEx`, `NtAlertMultipleThreadByThreadId`, …). The runtime resolves SSNs from the live `ntdll` at first call, so the compile-time superset works on both.

## Documentation (in `Docs/`)

- **[USAGE.md](./Docs/USAGE.md)** — start here — how to actually use it
- [PROJECT_OVERVIEW.md](./Docs/PROJECT_OVERVIEW.md) — what & why
- [ARCHITECTURE.md](./Docs/ARCHITECTURE.md) — ASCII flow diagrams
- [API.md](./Docs/API.md) — Rust / C / CLI public API reference
- [DATABASE.md](./Docs/DATABASE.md) — 3-layer TOML DB schema
- [DECISIONS.md](./Docs/DECISIONS.md) — ADRs (D-01 … D-24)
- [ROADMAP.md](./Docs/ROADMAP.md) — phases + checkable items
- [CURRENT_STATE.md](./Docs/CURRENT_STATE.md) — progress snapshot
- [`modules/`](./Docs/modules/) — per-crate internals

## License

MIT — see [`LICENSE`](./LICENSE).

Intended for authorized security research, red-team engagements, and penetration testing. Follow the law where you are, don't be a jerk.
