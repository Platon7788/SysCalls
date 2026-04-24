//! Auto-generated syscall stubs — the content is assembled by
//! `build.rs` from `db/canonical.toml` and dropped into `$OUT_DIR`.
//!
//! When `canonical.toml` is absent, the include file is a tiny stub so
//! this module still compiles. Running `cargo run -p rsc -- merge` (after
//! `rsc-collector` and `rsc-types`) populates the file.

#![allow(clippy::missing_safety_doc)]
#![allow(non_snake_case)]

include!(concat!(env!("OUT_DIR"), "/syscalls_generated.rs"));
