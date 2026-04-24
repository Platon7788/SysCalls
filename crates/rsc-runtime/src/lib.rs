#![no_std]
#![forbid(unsafe_op_in_unsafe_fn)]
#![allow(non_camel_case_types)] // Windows type aliases (HANDLE, PVOID, …)

//! # rsc-runtime
//!
//! Direct Windows NT syscall stubs with hash-obfuscated lookup and random
//! JUMPER dispatch. The consumer-facing crate of SysCalls — depend on it
//! and `rsc_runtime::syscalls::*` gives you ~500 NT functions.
//!
//! ## How to use
//!
//! ```ignore
//! use rsc_runtime::{syscalls::*, constants::*, types::*};
//!
//! let mut base: PVOID = core::ptr::null_mut();
//! let mut size: SIZE_T = 0x1000;
//! let status = unsafe {
//!     NtAllocateVirtualMemory(
//!         NT_CURRENT_PROCESS, &mut base, 0, &mut size,
//!         MEM_COMMIT | MEM_RESERVE, PAGE_READWRITE,
//!     )
//! };
//! ```
//!
//! ## Properties
//!
//! * `#![no_std]`, **zero runtime deps**. `rsc-codegen` is a proc-macro
//!   used only at build time.
//! * x64 + x86 / WoW64 both real (no stub placeholders).
//! * JUMPER by default: every syscall jumps through a **random**
//!   `syscall; ret` slide in ntdll so stack traces don't pattern.
//! * Baked canonical DB (`db/canonical.toml`) is checked into the repo —
//!   consumers don't need network, DbgHelp, or any tool run.
//!
//! ## Safety
//!
//! All syscall functions are `unsafe extern "system" fn` — caller
//! upholds NT API preconditions (valid handles, properly-aligned
//! buffers, etc.). Every internal `unsafe` block carries a `// SAFETY:`
//! comment.
//!
//! ## Source of truth
//!
//! `Docs/ARCHITECTURE.md` + `Docs/USAGE.md` at the repository root.

// Self-alias so code generated **inside this crate** (our own `build.rs`
// emitting `rsc_syscall!(...)` invocations against canonical.toml) can
// use the same absolute path consumers do — `::rsc_runtime::…`.
extern crate self as rsc_runtime;

// --- Internal modules ------------------------------------------------------

pub mod constants;
pub mod error;
mod hash;
mod jumper;
mod pe;
mod peb;
pub mod syscalls;
mod table;
pub mod types;

// --- Public API ------------------------------------------------------------

/// Hash computation — entry point for `rsc_syscall!` macro expansion and
/// manual calls to `resolve()` from consumer code.
pub use hash::{rsc_hash, RSC_SEED};

/// Table introspection. `count()` is useful for sanity checks ("did the
/// PEB walk work?"); `resolve()` lets consumers implement their own
/// dispatch or diagnose missing syscalls on a foreign Windows build.
pub use table::{count, resolve};

/// ABI wrappers called by `rsc_syscall!`-generated naked stubs. Split
/// into integer-returning functions so the MS x64 ABI doesn't route us
/// through a hidden-pointer return.
pub use table::{__rsc_random_slide, __rsc_resolve_ssn};

/// Status / result types.
pub use error::{NtStatus, NtStatusExt, RscResult};
