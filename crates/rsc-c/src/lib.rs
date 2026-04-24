//! # rsc-c
//!
//! Thin shim that re-exports `rsc-runtime`'s naked syscall stubs with an
//! `RscNt*` prefix, producing both `rsc.dll` (cdylib) and `rsc.lib`
//! (staticlib). A matching `include/rsc.h` is written by `build.rs`
//! on every compile.

// We re-export the runtime under its own path so the generated wrappers
// can resolve `rsc_runtime::syscalls::Nt*` etc.
pub use rsc_runtime;

// Generated wrapper functions: one `#[no_mangle] pub unsafe extern "system"`
// for every non-excluded syscall in canonical.toml.
include!(concat!(env!("OUT_DIR"), "/c_wrappers.rs"));
