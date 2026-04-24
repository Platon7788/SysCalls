//! DbgHelp wrapper for enumerating `Zw*` symbols in `ntdll.pdb`.
//!
//! We use DbgHelp's wide-char surface (`SymInitializeW` / `SymLoadModuleExW`
//! / `SymEnumSymbolsW`) to avoid any ANSI/UTF-8 translation issues with
//! PDB paths that contain non-ASCII characters.
//!
//! The session is wrapped in an RAII struct (`PdbSession`) with a `Drop`
//! impl so `SymCleanup` runs even on panic.

use std::cell::RefCell;
use std::path::Path;

use tracing::{debug, trace};
use windows::core::PCWSTR;
use windows::Win32::Foundation::HANDLE;
use windows::Win32::System::Diagnostics::Debug::{
    SymCleanup, SymEnumSymbolsW, SymInitializeW, SymLoadModuleExW, SymSetOptions, SYMBOL_INFOW,
    SYMOPT_CASE_INSENSITIVE, SYMOPT_DEBUG, SYMOPT_FAIL_CRITICAL_ERRORS, SYMOPT_UNDNAME,
};

use crate::error::CollectError;

// Shared TLS sink between `enumerate` and the DbgHelp callback. DbgHelp
// enumerates synchronously on the calling thread, so TLS is safe.
thread_local! {
    static SINK: RefCell<Vec<ZwSymbol>> = const { RefCell::new(Vec::new()) };
}

/// One row of the enumeration: name + RVA (address relative to module base).
#[derive(Debug, Clone)]
pub struct ZwSymbol {
    pub name: String,
    pub rva: u32,
}

/// RAII handle for a DbgHelp session scoped to a single PDB.
pub struct PdbSession {
    /// Synthetic handle we pass to every DbgHelp call. We use our own
    /// process handle equivalent — DbgHelp uses this as a session key,
    /// not an actual kernel handle, so any stable sentinel works.
    handle: HANDLE,
    module_base: u64,
}

impl PdbSession {
    /// Initializes DbgHelp, loads the PDB, and leaves the session ready
    /// for enumeration calls.
    ///
    /// `module_base` is a virtual base we assign to the module; it just
    /// has to be non-zero and unique per session (DbgHelp uses it as
    /// a key). The RVAs returned later are always relative to this base.
    pub fn open(pdb_path: &Path) -> Result<Self, CollectError> {
        // Synthesize a session handle. Using a synthetic non-null value is
        // a common DbgHelp pattern — the handle is opaque to the library
        // and never leaves our process. Using `ptr::without_provenance_mut`
        // (stable 1.84+) keeps miri / clippy happy about the integer→ptr.
        let handle = HANDLE(core::ptr::without_provenance_mut(0x1));
        let module_base = 0x1000_0000_u64;

        // SAFETY: `SymSetOptions` is thread-safe and doesn't touch shared state
        // we care about — any concurrent `PdbSession` would see the same set.
        unsafe {
            SymSetOptions(
                SYMOPT_CASE_INSENSITIVE
                    | SYMOPT_UNDNAME
                    | SYMOPT_DEBUG
                    | SYMOPT_FAIL_CRITICAL_ERRORS,
            );
        }

        // Pass `null` for the search path — DbgHelp will default to the
        // directory containing the PDB plus `_NT_SYMBOL_PATH`, which is
        // fine since we hand it an absolute path.
        // SAFETY: FFI call with valid handle; None search path is allowed.
        unsafe {
            SymInitializeW(handle, PCWSTR::null(), false).map_err(|_| CollectError::DbgHelp {
                api: "SymInitializeW",
                os_code: windows::Win32::Foundation::GetLastError().0,
            })?;
        }

        let wide_path = to_wide_null(pdb_path);
        // SAFETY: `handle` initialized above; `wide_path` is null-terminated
        // and alive for the duration of the call.
        let result = unsafe {
            SymLoadModuleExW(
                handle,
                None,
                PCWSTR(wide_path.as_ptr()),
                PCWSTR::null(),
                module_base,
                0,
                None,
                None,
            )
        };
        if result == 0 {
            // Clean up and bail.
            // SAFETY: paired with the earlier SymInitializeW.
            unsafe {
                let _ = SymCleanup(handle);
            }
            return Err(CollectError::DbgHelp {
                api: "SymLoadModuleExW",
                os_code: unsafe { windows::Win32::Foundation::GetLastError().0 },
            });
        }

        debug!(pdb = %pdb_path.display(), module_base = format!("{module_base:#x}"), "PDB loaded");
        Ok(Self { handle, module_base })
    }

    /// Enumerate symbols whose names match `mask` (wildcard syntax).
    ///
    /// Internally we use a `RefCell<Vec<_>>` to bridge from the FFI
    /// callback (which receives a `*const c_void` user pointer) back into
    /// safe Rust land. `SymEnumSymbolsW` is synchronous and single-threaded
    /// per session, so the cell never sees concurrent borrows.
    pub fn enumerate(&self, mask: &str) -> Result<Vec<ZwSymbol>, CollectError> {
        SINK.with(|cell| cell.borrow_mut().clear());

        let wide_mask = to_wide_null_from_str(mask);

        // SAFETY: all pointers passed to DbgHelp are valid for the call's
        // duration; the callback writes into thread-local storage.
        let ok = unsafe {
            SymEnumSymbolsW(
                self.handle,
                self.module_base,
                PCWSTR(wide_mask.as_ptr()),
                Some(enum_callback),
                None,
            )
            .is_ok()
        };
        if !ok {
            return Err(CollectError::DbgHelp {
                api: "SymEnumSymbolsW",
                os_code: unsafe { windows::Win32::Foundation::GetLastError().0 },
            });
        }

        let found = SINK.with(|cell| cell.borrow_mut().drain(..).collect::<Vec<_>>());
        debug!(mask, count = found.len(), "enumerate complete");
        Ok(found)
    }
}

impl Drop for PdbSession {
    fn drop(&mut self) {
        // SAFETY: paired with SymInitializeW in `open`.
        unsafe {
            let _ = SymCleanup(self.handle);
        }
    }
}

// DbgHelp calls this once per matching symbol. Contract:
//   * Return TRUE to continue, FALSE to abort.
//   * `sym_info` has a `Name` WCHAR array of `NameLen` chars.
//   * `Address` is the absolute VA — subtract module_base to get RVA.
unsafe extern "system" fn enum_callback(
    sym_info: *const SYMBOL_INFOW,
    _symbol_size: u32,
    _user: *const core::ffi::c_void,
) -> windows::core::BOOL {
    // SAFETY (for the `&*sym_info` read):
    //   * DbgHelp's contract for `PSYM_ENUMERATESYMBOLS_CALLBACKW` says the
    //     pointer is non-null and references a fully-initialized
    //     `SYMBOL_INFOW` for the duration of this callback only.
    //   * We do not retain the pointer past this scope.
    let info = unsafe { &*sym_info };

    // SAFETY (for the wide-string slice):
    //   * `info.Name` is declared `[WCHAR; 1]` but DbgHelp over-allocates
    //     the whole struct with an inline flexible array for exactly
    //     `NameLen` wide chars. The maximum cap is `MaxNameLen`, which is
    //     the caller-supplied buffer size; DbgHelp promises `NameLen <=
    //     MaxNameLen`. Both fields are read AFTER the `&*sym_info` above,
    //     so they reflect the same struct instance.
    //   * `Name` is WCHAR-aligned (2-byte) by design of `SYMBOL_INFOW`.
    //   * `NameLen` is `u32`; on 32-bit hosts we rely on Windows keeping
    //     it < isize::MAX (trivially true — names are 4 KB at most).
    let name_slice = unsafe {
        core::slice::from_raw_parts(info.Name.as_ptr(), info.NameLen as usize)
    };
    // DbgHelp (empirically) includes the trailing `\0` in `NameLen` on some
    // Windows versions; strip it unconditionally so downstream code gets
    // clean identifiers.
    let name = String::from_utf16_lossy(name_slice)
        .trim_end_matches('\0')
        .to_string();

    let rva = info.Address.saturating_sub(info.ModBase) as u32;
    trace!(name = %name, rva = format!("{rva:#x}"), "symbol");

    SINK.with(|cell| {
        cell.borrow_mut().push(ZwSymbol { name, rva });
    });

    true.into()
}

fn to_wide_null(p: &Path) -> Vec<u16> {
    let mut v: Vec<u16> = p.as_os_str().encode_wide().collect();
    v.push(0);
    v
}

fn to_wide_null_from_str(s: &str) -> Vec<u16> {
    let mut v: Vec<u16> = s.encode_utf16().collect();
    v.push(0);
    v
}

// Pull in the OS-specific `OsStrExt::encode_wide` on Windows.
use std::os::windows::ffi::OsStrExt;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn to_wide_adds_terminator() {
        let w = to_wide_null(Path::new("hello"));
        assert_eq!(*w.last().unwrap(), 0);
        assert_eq!(w.len(), 6);
    }
}
