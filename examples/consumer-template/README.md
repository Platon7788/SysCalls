# consumer-template

Minimal external consumer of `rsc-runtime`. Two files. Run it to see
`NtAllocateVirtualMemory` / `NtQueryVirtualMemory` / `NtFreeVirtualMemory`
/ `NtClose` executing through the random-JUMPER path on the current
Windows build.

## Run in place

```
cd examples/consumer-template
cargo run --release
```

Expected output (truncated):

```
[*] rsc-runtime resolved 473 syscalls from this process's ntdll
[+] allocated 4096 bytes at 0x…
[+] wrote + read back sentinel 0xDEAD_BEEF_CAFE_BABE
[+] NtQueryVirtualMemory: Protect=0x4, State=0x1000, RegionSize=0x1000
[+] freed
[*] NtClose(0xDEADBEEF) -> 0xc0000008 (expected STATUS_INVALID_HANDLE = 0xC0000008)
[*] all template checks passed
```

## Use as a starting point for your own tool

1. Copy the whole `consumer-template/` directory anywhere outside the
   SysCalls tree.
2. Open `Cargo.toml` and fix the path dep:

   ```toml
   [dependencies]
   rsc-runtime = { path = "<absolute-or-relative path>/SysCalls/crates/rsc-runtime" }
   ```

   (If you prefer `git = "…"`, see
   [`Docs/USAGE.md § 6 Using as a library`](../../Docs/USAGE.md) for the
   `RSC_CANONICAL_PATH` workflow.)

3. Rename `rsc-consumer-template` in `Cargo.toml` to whatever you want
   your tool called.

4. Edit `src/main.rs`; you now have all of `rsc_runtime::syscalls::*`
   available, plus `rsc_runtime::constants::*`, `rsc_runtime::types::*`,
   `rsc_runtime::error::{NtStatus, RscResult}`, etc.

## Prerequisites on this machine

**Nothing special.** `db/canonical.toml` is checked into the SysCalls
repo (the "Path A" baked-distribution pattern), so a fresh clone + a
path / git dep in your `Cargo.toml` is all it takes. No PDB downloads,
no phnt parsing, no network at consumer-build time.

Run `scripts\refresh.bat` only when Microsoft ships a new Windows
build you want first-class coverage on — that's a **repo-maintainer**
task, not something every consumer needs to do.

## Anti-stealth knobs (optional)

Add a `features = […]` clause on the `rsc-runtime` dep to toggle:

| Feature | Effect |
|---|---|
| `status-names-full` | Full 260-status name table in `NtStatus.name()` |
| `debug-breakpoints` | `int3` before each syscall (for WinDbg attach) |
| `random-seed` | Use `RSC_SEED_OVERRIDE` env var instead of default seed |
| `no-jumper` | Direct `syscall` from stub (louder, less stealthy) |

Example: `rsc-runtime = { path = "…", features = ["status-names-full"] }`

## Why `[workspace]` block in Cargo.toml?

Keeps this template *standalone* — it has its own `Cargo.lock` and
`target/` and doesn't get sucked into a parent workspace if you drop
it into another repo. If you want to fold it into your own existing
workspace, delete the empty `[workspace]` section.
