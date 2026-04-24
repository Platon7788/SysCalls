//! Translates phnt type spellings into the canonical RSC / Rust form
//! that the `rsc_syscall!` macro and downstream TOML consumers expect.
//!
//! Strategy: hardcoded mapping table of the types that actually appear
//! in phnt's NT syscall declarations. Everything unknown falls through
//! as an opaque pointer so the function stays callable (with
//! type-unsafe but behaviorally-correct arguments).

/// Maps a phnt type spelling to the canonical Rust-style name. Star
/// suffixes (e.g. `PVOID*`) are handled by unwrapping one level at a
/// time, keeping the translation monotone.
pub fn normalize(raw: &str) -> String {
    // Drop C qualifiers that have no Rust equivalent — at any position.
    // Rust doesn't model `volatile` / `register` / `__forceinline` etc.;
    // we rely on the compiler's own volatility guarantees through
    // `core::ptr::read_volatile`/`write_volatile` at call sites.
    let cleaned = raw
        .replace("volatile ", "")
        .replace(" volatile", "")
        .replace("volatile", "")
        .replace("__forceinline ", "")
        .replace("register ", "");
    let trimmed = cleaned.trim();
    if let Some(inner) = trimmed.strip_suffix('*') {
        return format!("*mut {}", normalize(inner));
    }
    if let Some(inner) = trimmed.strip_prefix("const ") {
        return format!("*const {}", normalize(inner));
    }
    match trimmed {
        // Handles / pointers
        "HANDLE" | "PHANDLE" => "HANDLE".into(),
        "PVOID" | "LPVOID" | "LPCVOID" => "*mut c_void".into(),
        "PCSTR" | "LPCSTR" => "*const u8".into(),
        "PCWSTR" | "LPCWSTR" => "*const u16".into(),
        "PSTR" | "LPSTR" => "*mut u8".into(),
        "PWSTR" | "LPWSTR" => "*mut u16".into(),

        // Integers (unsigned)
        "UCHAR" | "BYTE" => "u8".into(),
        "USHORT" | "WORD" => "u16".into(),
        "ULONG" | "DWORD" => "u32".into(),
        "ULONG64" | "DWORD64" | "ULONGLONG" | "QWORD" => "u64".into(),
        "ULONG_PTR" | "SIZE_T" | "DWORD_PTR" => "usize".into(),
        "UINT" => "u32".into(),

        // Integers (signed)
        "CHAR" => "i8".into(),
        "SHORT" => "i16".into(),
        "LONG" | "INT" => "i32".into(),
        "LONG64" | "LONGLONG" | "INT64" => "i64".into(),
        "LONG_PTR" | "SSIZE_T" | "INT_PTR" => "isize".into(),
        "NTSTATUS" => "NTSTATUS".into(),
        "LARGE_INTEGER" => "i64".into(),
        "ULARGE_INTEGER" => "u64".into(),

        // Booleans
        "BOOLEAN" => "u8".into(),
        "BOOL" => "i32".into(),

        // Access masks & flags
        "ACCESS_MASK" => "u32".into(),

        // Common pointer-to-NT-struct types: keep type name (Rust side will
        // have its own repr-C equivalent). Parameter types that we don't have
        // a Rust representation for stay as opaque pointers.
        "PUNICODE_STRING" => "*mut UNICODE_STRING".into(),
        "PCUNICODE_STRING" => "*const UNICODE_STRING".into(),
        "POBJECT_ATTRIBUTES" => "*mut OBJECT_ATTRIBUTES".into(),
        "PIO_STATUS_BLOCK" => "*mut IO_STATUS_BLOCK".into(),
        "PCLIENT_ID" => "*mut CLIENT_ID".into(),
        "PSIZE_T" => "*mut usize".into(),
        "PULONG" => "*mut u32".into(),
        "PULONG64" => "*mut u64".into(),
        "PULONG_PTR" => "*mut usize".into(),
        "PUSHORT" => "*mut u16".into(),
        "PUCHAR" => "*mut u8".into(),
        "PLONG" => "*mut i32".into(),
        "PLONG64" => "*mut i64".into(),
        "PLARGE_INTEGER" => "*mut i64".into(),
        "PULARGE_INTEGER" => "*mut u64".into(),
        "PBOOLEAN" => "*mut u8".into(),
        "PBOOL" => "*mut i32".into(),
        "PACCESS_MASK" => "*mut u32".into(),
        "PNTSTATUS" => "*mut NTSTATUS".into(),

        // Fallback heuristic for NT struct / enum names we don't mirror:
        //
        // * phnt spells pointer types with a `P` / `LP` prefix, so
        //   `PFOO` / `LPFOO` → opaque pointer. Keeps byte layout correct.
        // * Everything else (`FOO_CLASS`, `FOO_INFORMATION`, enum-like
        //   names) is passed as a `u32`-sized value. Getting this wrong
        //   would push / pull 8 bytes where 4 were expected; the enum
        //   heuristic matches real NT ABI much more often than the
        //   pointer fallback.
        other => {
            if other.starts_with("LP") || other.starts_with('P') {
                "*mut c_void".into()
            } else {
                "u32".into()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn primitive_roundtrip() {
        assert_eq!(normalize("HANDLE"), "HANDLE");
        assert_eq!(normalize("NTSTATUS"), "NTSTATUS");
        assert_eq!(normalize("ULONG"), "u32");
        assert_eq!(normalize("USHORT"), "u16");
        assert_eq!(normalize("SIZE_T"), "usize");
    }

    #[test]
    fn pointer_levels() {
        assert_eq!(normalize("PVOID"), "*mut c_void");
        assert_eq!(normalize("PVOID*"), "*mut *mut c_void");
        assert_eq!(normalize("PULONG"), "*mut u32");
        assert_eq!(normalize("PULONG*"), "*mut *mut u32");
    }

    #[test]
    fn const_qualifier() {
        assert_eq!(normalize("const ULONG"), "*const u32");
    }

    #[test]
    fn unknown_pointer_types_are_opaque() {
        assert_eq!(normalize("PSECURITY_DESCRIPTOR"), "*mut c_void");
        assert_eq!(normalize("PPORT_MESSAGE"), "*mut c_void");
        assert_eq!(normalize("LPOVERLAPPED_COMPLETION_ROUTINE"), "*mut c_void");
    }

    #[test]
    fn unknown_value_types_are_u32() {
        // Enum classes and DWORD-width flag types.
        assert_eq!(normalize("MEMORY_INFORMATION_CLASS"), "u32");
        assert_eq!(normalize("FILE_INFORMATION_CLASS"), "u32");
    }
}
