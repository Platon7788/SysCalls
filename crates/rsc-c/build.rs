//! Generates:
//!
//! * `$OUT_DIR/c_wrappers.rs` — `#[no_mangle] pub unsafe extern "system"
//!   fn RscNt*` wrappers that call into `rsc_runtime::Nt*` and are then
//!   exported from the cdylib + staticlib.
//!
//! * `$CARGO_MANIFEST_DIR/include/rsc.h` — C header with matching
//!   declarations, guarded against collisions with `windows.h`.

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
    let wrappers_path = out_dir.join("c_wrappers.rs");

    let manifest_dir =
        PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let header_path = manifest_dir.join("include").join("rsc.h");

    let canonical_path = env::var_os("RSC_CANONICAL_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            manifest_dir
                .join("..")
                .join("..")
                .join("db")
                .join("canonical.toml")
        });

    println!("cargo:rerun-if-env-changed=RSC_CANONICAL_PATH");
    println!("cargo:rerun-if-changed={}", canonical_path.display());

    let canon: Canonical = if canonical_path.exists() {
        let s = fs::read_to_string(&canonical_path)
            .unwrap_or_else(|e| panic!("read {}: {e}", canonical_path.display()));
        toml::from_str(&s)
            .unwrap_or_else(|e| panic!("parse {}: {e}", canonical_path.display()))
    } else {
        eprintln!(
            "cargo:warning=canonical.toml missing at {}; rsc-c will export 0 functions",
            canonical_path.display()
        );
        Canonical::default()
    };

    emit_wrappers(&canon, &wrappers_path);
    emit_header(&canon, &header_path);
}

/// Emit Rust extern-"system" wrappers that call into `rsc_runtime`'s
/// auto-generated stubs. Every exported symbol starts with `RSC` so we
/// never collide with real Windows names in a process that also pulls
/// in ntdll normally.
fn emit_wrappers(canon: &Canonical, out: &std::path::Path) {
    let mut s = String::with_capacity(128 * 1024);
    s.push_str("// @generated — DO NOT EDIT\n\n");
    s.push_str("#[allow(unused_imports)]\n");
    s.push_str("use rsc_runtime::types::*;\n");
    s.push_str("#[allow(unused_imports)]\n");
    s.push_str("use core::ffi::c_void;\n\n");

    for sys in &canon.syscalls {
        if sys.excluded {
            continue;
        }
        let rsc_name = format!("Rsc{}", sys.name);
        s.push_str("#[no_mangle]\n");
        s.push_str("#[allow(non_snake_case, clippy::missing_safety_doc)]\n");
        s.push_str("pub unsafe extern \"system\" fn ");
        s.push_str(&rsc_name);
        s.push('(');
        for (i, p) in sys.params.iter().enumerate() {
            if i > 0 {
                s.push_str(", ");
            }
            s.push_str(&p.name);
            s.push_str(": ");
            s.push_str(&sanitize_type(&p.r#type));
        }
        s.push_str(") -> ");
        s.push_str(&sanitize_type(&sys.return_type));
        s.push_str(" {\n    // SAFETY: delegates to the rsc-runtime naked stub with identical ABI.\n    unsafe { rsc_runtime::syscalls::");
        s.push_str(&sys.name);
        s.push('(');
        for (i, p) in sys.params.iter().enumerate() {
            if i > 0 {
                s.push_str(", ");
            }
            s.push_str(&p.name);
        }
        s.push_str(") }\n}\n\n");
    }

    fs::write(out, s).unwrap_or_else(|e| panic!("write wrappers: {e}"));
}

/// Emit `rsc.h` with `RSC_*` type aliases and function declarations.
fn emit_header(canon: &Canonical, out: &std::path::Path) {
    let mut s = String::with_capacity(256 * 1024);

    s.push_str("/*\n * rsc.h — SysCalls (RSC) C bindings.\n * @generated from canonical.toml — DO NOT EDIT.\n */\n\n");
    s.push_str("#ifndef RSC_H\n#define RSC_H\n\n");
    s.push_str("#include <stdint.h>\n#include <stddef.h>\n\n");
    s.push_str("#ifdef __cplusplus\nextern \"C\" {\n#endif\n\n");

    // Type aliases — skip collision with windows.h when it's already included.
    s.push_str("#ifndef _WINDEF_\n");
    s.push_str("typedef void*    RSC_HANDLE;\n");
    s.push_str("typedef void*    RSC_PVOID;\n");
    s.push_str("typedef int32_t  RSC_NTSTATUS;\n");
    s.push_str("typedef uintptr_t RSC_SIZE_T;\n");
    s.push_str("typedef uintptr_t RSC_ULONG_PTR;\n");
    s.push_str("typedef uint8_t  RSC_BOOLEAN;\n");
    s.push_str("typedef uint8_t  RSC_UCHAR;\n");
    s.push_str("typedef uint16_t RSC_USHORT;\n");
    s.push_str("typedef uint32_t RSC_ULONG;\n");
    s.push_str("typedef uint64_t RSC_ULONG64;\n");
    s.push_str("#else\n");
    s.push_str("#define RSC_HANDLE    HANDLE\n");
    s.push_str("#define RSC_PVOID     PVOID\n");
    s.push_str("#define RSC_NTSTATUS  NTSTATUS\n");
    s.push_str("#define RSC_SIZE_T    SIZE_T\n");
    s.push_str("#define RSC_ULONG_PTR ULONG_PTR\n");
    s.push_str("#define RSC_BOOLEAN   BOOLEAN\n");
    s.push_str("#define RSC_UCHAR     UCHAR\n");
    s.push_str("#define RSC_USHORT    USHORT\n");
    s.push_str("#define RSC_ULONG     ULONG\n");
    s.push_str("#define RSC_ULONG64   ULONG64\n");
    s.push_str("#endif\n\n");

    s.push_str("#define RSC_STATUS_SUCCESS           ((RSC_NTSTATUS)0x00000000)\n");
    s.push_str("#define RSC_STATUS_UNSUCCESSFUL      ((RSC_NTSTATUS)0xC0000001)\n");
    s.push_str("#define RSC_STATUS_ACCESS_DENIED     ((RSC_NTSTATUS)0xC0000022)\n");
    s.push_str("#define RSC_STATUS_INVALID_HANDLE    ((RSC_NTSTATUS)0xC0000008)\n");
    s.push_str("#define RSC_STATUS_INVALID_PARAMETER ((RSC_NTSTATUS)0xC000000D)\n");
    s.push_str("#define RSC_STATUS_NO_MEMORY         ((RSC_NTSTATUS)0xC0000017)\n\n");
    s.push_str("#define RSC_PAGE_READONLY            0x02\n");
    s.push_str("#define RSC_PAGE_READWRITE           0x04\n");
    s.push_str("#define RSC_PAGE_EXECUTE_READ        0x20\n");
    s.push_str("#define RSC_PAGE_EXECUTE_READWRITE   0x40\n");
    s.push_str("#define RSC_MEM_COMMIT               0x00001000\n");
    s.push_str("#define RSC_MEM_RESERVE              0x00002000\n");
    s.push_str("#define RSC_MEM_RELEASE              0x00008000\n\n");

    s.push_str("/* ---- Function declarations ---- */\n\n");

    for sys in &canon.syscalls {
        if sys.excluded {
            continue;
        }
        let rsc_name = format!("Rsc{}", sys.name);
        s.push_str(&c_return_type(&sys.return_type));
        s.push(' ');
        s.push_str(&rsc_name);
        s.push('(');
        if sys.params.is_empty() {
            s.push_str("void");
        } else {
            for (i, p) in sys.params.iter().enumerate() {
                if i > 0 {
                    s.push_str(", ");
                }
                s.push_str(&c_param_type(&p.r#type));
                s.push(' ');
                s.push_str(&p.name);
            }
        }
        s.push_str(");\n");
    }

    s.push_str("\n#ifdef __cplusplus\n}\n#endif\n\n#endif /* RSC_H */\n");

    if let Some(parent) = out.parent() {
        fs::create_dir_all(parent).unwrap_or_else(|e| panic!("create header dir: {e}"));
    }
    fs::write(out, s).unwrap_or_else(|e| panic!("write header: {e}"));
    println!("cargo:warning=rsc-c emitted include/rsc.h with {} functions", canon.syscalls.len());
}

fn sanitize_type(raw: &str) -> String {
    raw.replace("volatile ", "")
        .replace(" volatile", "")
        .replace("volatile", "")
        .replace("__forceinline ", "")
        .trim()
        .to_string()
}

/// Rust return type → C return type spelling.
fn c_return_type(raw: &str) -> String {
    c_type(raw)
}

/// Rust param type → C param type.
fn c_param_type(raw: &str) -> String {
    c_type(raw)
}

/// Map a Rust syntax type into its C equivalent. Everything unmapped
/// becomes `void*` — safe because NT parameters are always word-sized.
fn c_type(raw: &str) -> String {
    let t = raw.trim();
    if let Some(rest) = t.strip_prefix("*mut ") {
        let inner = c_type(rest);
        if inner == "void" {
            return "void*".to_string();
        }
        return format!("{inner}*");
    }
    if let Some(rest) = t.strip_prefix("*const ") {
        let inner = c_type(rest);
        if inner == "void" {
            return "const void*".to_string();
        }
        return format!("const {inner}*");
    }
    match t {
        "HANDLE" => "RSC_HANDLE".into(),
        "NTSTATUS" => "RSC_NTSTATUS".into(),
        "UNICODE_STRING" => "RSC_PVOID".into(),   // opaque; real layout via windows.h
        "OBJECT_ATTRIBUTES" => "RSC_PVOID".into(),
        "IO_STATUS_BLOCK" => "RSC_PVOID".into(),
        "CLIENT_ID" => "RSC_PVOID".into(),
        "c_void" => "void".into(),
        "u8" => "uint8_t".into(),
        "u16" => "uint16_t".into(),
        "u32" => "RSC_ULONG".into(),
        "u64" => "uint64_t".into(),
        "i8" => "int8_t".into(),
        "i16" => "int16_t".into(),
        "i32" => "int32_t".into(),
        "i64" => "int64_t".into(),
        "usize" => "RSC_SIZE_T".into(),
        "isize" => "intptr_t".into(),
        other => {
            // Unknown — collapse to opaque pointer safely on caller side.
            let _ = other;
            "RSC_PVOID".into()
        }
    }
}
