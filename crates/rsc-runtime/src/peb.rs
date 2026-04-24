//! PEB access and ntdll discovery via the in-memory module list.
//!
//! The PEB segment offsets differ per ISA:
//!
//! | ISA | segment | offset |
//! |-----|---------|--------|
//! | x64 | `GS`    | `0x60` |
//! | x86 | `FS`    | `0x30` |
//!
//! We deliberately avoid `GetModuleHandle`, `LoadLibrary`, or any Win32
//! API that could be user-mode hooked. See §S-03.

use core::arch::asm;

use crate::types::UNICODE_STRING;

#[repr(C)]
pub(crate) struct ListEntry {
    pub flink: *mut ListEntry,
    pub blink: *mut ListEntry,
}

/// Subset of `PEB_LDR_DATA`. Only fields up to `InLoadOrderModuleList`
/// are modelled — the rest is opaque.
#[repr(C)]
pub(crate) struct PebLdrData {
    _length: u32,
    _initialized: u32,
    _ss_handle: *mut u8,
    pub in_load_order_module_list: ListEntry,
    pub in_memory_order_module_list: ListEntry,
    pub in_initialization_order_module_list: ListEntry,
}

/// Subset of `LDR_DATA_TABLE_ENTRY`.
///
/// The list-head offset matters: `InLoadOrderLinks` is the **first** field,
/// so a pointer obtained from `in_load_order_module_list.flink` can be
/// cast directly to this struct.
#[repr(C)]
pub(crate) struct LdrDataTableEntry {
    pub in_load_order_links: ListEntry,
    _in_memory_order_links: ListEntry,
    _in_initialization_order_links: ListEntry,
    pub dll_base: *mut u8,
    _entry_point: *mut u8,
    _size_of_image: u32,
    _full_dll_name: UNICODE_STRING,
    pub base_dll_name: UNICODE_STRING,
}

/// Subset of the Process Environment Block. Only the `Ldr` pointer is
/// required for syscall resolution.
#[repr(C)]
pub(crate) struct Peb {
    _inherited_address_space: u8,
    _read_image_file_exec_options: u8,
    _being_debugged: u8,
    _reserved: u8,
    _mutant: *mut u8,
    _image_base_address: *mut u8,
    pub ldr: *mut PebLdrData,
}

/// Reads the PEB pointer from the thread environment block.
///
/// # Safety
///
/// Returns a valid PEB pointer on any Windows thread; semantics of the
/// inline asm rely on a conforming NT TEB layout.
#[cfg(target_arch = "x86_64")]
#[inline]
pub(crate) unsafe fn get_peb() -> *mut Peb {
    let peb: *mut Peb;
    // SAFETY: `gs:[0x60]` is the documented location of the PEB pointer
    // in a 64-bit TEB on Windows.
    unsafe {
        asm!(
            "mov {}, gs:[0x60]",
            out(reg) peb,
            options(nostack, nomem, preserves_flags),
        );
    }
    peb
}

/// Reads the PEB pointer on 32-bit Windows / WoW64.
///
/// # Safety
///
/// See [`get_peb`] for x64.
#[cfg(target_arch = "x86")]
#[inline]
pub(crate) unsafe fn get_peb() -> *mut Peb {
    let peb: *mut Peb;
    // SAFETY: `fs:[0x30]` is the documented location of the PEB pointer
    // in a 32-bit TEB on Windows.
    unsafe {
        asm!(
            "mov {}, fs:[0x30]",
            out(reg) peb,
            options(nostack, nomem, preserves_flags),
        );
    }
    peb
}

/// UTF-16 lowercase-insensitive byte compare (A–Z only — good enough for
/// the module list, which only ever contains ASCII filenames).
#[inline]
fn utf16_eq_ascii_ci(buffer: *const u16, len_bytes: u16, needle: &[u16]) -> bool {
    if (len_bytes as usize) != needle.len() * 2 {
        return false;
    }
    for (i, &want) in needle.iter().enumerate() {
        // SAFETY: caller ensures `buffer` is readable for `len_bytes`.
        let got = unsafe { *buffer.add(i) };
        let got_lc = ascii_lower_u16(got);
        let want_lc = ascii_lower_u16(want);
        if got_lc != want_lc {
            return false;
        }
    }
    true
}

#[inline]
const fn ascii_lower_u16(c: u16) -> u16 {
    if c >= b'A' as u16 && c <= b'Z' as u16 {
        c + 0x20
    } else {
        c
    }
}

/// UTF-16 literal for `"ntdll.dll"`.
const NTDLL_UTF16: [u16; 9] = [
    b'n' as u16, b't' as u16, b'd' as u16, b'l' as u16, b'l' as u16,
    b'.' as u16, b'd' as u16, b'l' as u16, b'l' as u16,
];

/// Locates `ntdll.dll`'s load address by walking `InLoadOrderModuleList`.
///
/// Returns `Some(base)` on success. A return of `None` indicates the PEB
/// / loader structures are not in the expected shape (which should only
/// happen on a corrupt process — never under normal execution).
///
/// # Safety
///
/// Must be called from code running inside a normal Windows user-mode
/// process. The returned pointer, once non-null, remains valid for the
/// lifetime of the process (ntdll is never unloaded).
pub(crate) unsafe fn find_ntdll() -> Option<*mut u8> {
    // SAFETY: read PEB → Ldr per Windows ABI.
    let peb = unsafe { get_peb() };
    if peb.is_null() {
        return None;
    }
    let ldr = unsafe { (*peb).ldr };
    if ldr.is_null() {
        return None;
    }

    // Head sentinel lives inside PebLdrData — not a real LDR_DATA_TABLE_ENTRY.
    let head = unsafe { &(*ldr).in_load_order_module_list as *const ListEntry as *mut ListEntry };
    let mut cursor = unsafe { (*head).flink };

    // Bounded walk — a healthy Ldr list is small; cap at 1024 entries to
    // guard against loops in corrupted state.
    for _ in 0..1024 {
        if cursor.is_null() || cursor == head {
            return None;
        }

        // `InLoadOrderLinks` is the first field of LDR_DATA_TABLE_ENTRY, so
        // the list pointer is the entry pointer.
        let entry = cursor as *mut LdrDataTableEntry;
        // SAFETY: `entry` is a valid loader entry per Ldr list invariants.
        let name = unsafe { &(*entry).base_dll_name };
        if !name.Buffer.is_null()
            && utf16_eq_ascii_ci(name.Buffer, name.Length, &NTDLL_UTF16)
        {
            let base = unsafe { (*entry).dll_base };
            if !base.is_null() {
                return Some(base);
            }
        }

        cursor = unsafe { (*cursor).flink };
    }

    None
}
