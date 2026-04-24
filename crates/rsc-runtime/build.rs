//! Build-time bridge: reads `db/canonical.toml` and emits one
//! `rsc_syscall! { fn … }` invocation per non-excluded function into
//! `$OUT_DIR/syscalls_generated.rs`. The lib's `syscalls` module then
//! `include!`s that file.
//!
//! If `db/canonical.toml` isn't present (fresh clone, merge not yet run),
//! we emit an empty stub so the crate still compiles —
//! `cargo run -p rsc-collector`, `cargo run -p rsc-types`, then
//! `cargo run --bin rsc -- merge` populates it.
//!
//! Override the path with `RSC_CANONICAL_PATH` — handy for CI where the
//! canonical file may live outside the workspace.

use std::env;
use std::fs;
use std::path::PathBuf;

use serde::Deserialize;

#[derive(Deserialize, Default)]
struct Canonical {
    #[serde(rename = "syscall", default)]
    syscalls: Vec<Syscall>,
}

#[derive(Deserialize)]
struct Syscall {
    name: String,
    return_type: String,
    #[serde(default)]
    params: Vec<Param>,
    #[serde(default)]
    excluded: bool,
}

#[derive(Deserialize)]
struct Param {
    name: String,
    r#type: String,
}

fn main() {
    let out_dir = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR"));
    let out_path = out_dir.join("syscalls_generated.rs");

    let manifest_dir =
        PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));

    let canonical_path = env::var_os("RSC_CANONICAL_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            // Default: SysCalls/db/canonical.toml (two levels up from crate).
            manifest_dir.join("..").join("..").join("db").join("canonical.toml")
        });

    println!("cargo:rerun-if-env-changed=RSC_CANONICAL_PATH");
    println!("cargo:rerun-if-changed={}", canonical_path.display());

    // Hard-fail if canonical.toml is missing: it's checked in (Path A)
    // and a missing copy means the developer deleted it by hand, or the
    // path-env override points to nothing. Emitting an empty syscalls
    // module (old "soft fallback") just hides the problem until a
    // consumer hits a missing function at runtime.
    if !canonical_path.exists() {
        panic!(
            "canonical.toml not found at {}.\n\
             \n\
             Expected a committed DB at <SysCalls>/db/canonical.toml.\n\
             If you're the maintainer: run `scripts/refresh.bat`.\n\
             If you're a consumer: restore the file from git — SysCalls\n\
             ships it via Path A (baked distribution).",
            canonical_path.display()
        );
    }
    let s = fs::read_to_string(&canonical_path)
        .unwrap_or_else(|e| panic!("read {}: {e}", canonical_path.display()));
    let canon: Canonical = toml::from_str(&s)
        .unwrap_or_else(|e| panic!("parse {}: {e}", canonical_path.display()));

    let mut out = String::with_capacity(256 * 1024);
    out.push_str("// @generated from canonical.toml — DO NOT EDIT\n\n");
    out.push_str("#[allow(unused_imports)]\n");
    out.push_str("use crate::types::*;\n");
    out.push_str("#[allow(unused_imports)]\n");
    out.push_str("use core::ffi::c_void;\n\n");

    let mut emitted = 0_usize;
    let mut skipped = 0_usize;

    for sys in &canon.syscalls {
        if sys.excluded {
            skipped += 1;
            continue;
        }
        out.push_str("::rsc_codegen::rsc_syscall! {\n    fn ");
        out.push_str(&sys.name);
        out.push('(');
        for (i, p) in sys.params.iter().enumerate() {
            if i > 0 {
                out.push_str(", ");
            }
            out.push_str(&p.name);
            out.push_str(": ");
            out.push_str(&sanitize_type(&p.r#type));
        }
        out.push_str(") -> ");
        out.push_str(&sanitize_type(&sys.return_type));
        out.push_str(";\n}\n");
        emitted += 1;
    }

    fs::write(&out_path, &out)
        .unwrap_or_else(|e| panic!("write {}: {e}", out_path.display()));

    println!("cargo:warning=rsc-runtime emitted {emitted} syscalls (skipped {skipped})");
}

/// Defensive layer: even if an older canonical.toml leaked through with
/// leftover C keywords that aren't valid Rust tokens, scrub them here so
/// the crate always builds. Normalizer in `rsc-types` handles the common
/// cases upstream; this is belt-and-braces.
fn sanitize_type(raw: &str) -> String {
    raw.replace("volatile ", "")
        .replace(" volatile", "")
        .replace("volatile", "")
        .replace("__forceinline ", "")
        .replace("register ", "")
        .trim()
        .to_string()
}
