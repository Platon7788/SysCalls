//! Syscall resolution table — hash ↔ (SSN, syscall slide).
//!
//! The table is populated lazily on first use: we walk ntdll's export
//! directory, filter `Zw*` stubs (SSN-mapped aliases of `Nt*`), hash the
//! corresponding `Nt*` name, locate a `syscall; ret` slide for the JUMPER,
//! and sort entries by function address — the sort order is the SSN per
//! Windows' documented behavior.
//!
//! # Concurrency
//!
//! `AtomicBool` + `UnsafeCell` lazy init (see §S-02, ADR D-08).
//! The populate routine is idempotent; concurrent first-callers produce
//! bit-identical tables and the `Release` store publishes the finished
//! table to other threads' `Acquire` loads.

use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicU8, Ordering};

use crate::hash::rsc_hash;
use crate::jumper::{find_syscall_slide, next_random};
use crate::pe::{find_export_directory, name_at_rva};
use crate::peb::find_ntdll;

/// Maximum number of syscalls the table can hold. Current Windows builds
/// export ~520 `Zw*` functions; 768 leaves headroom for future growth.
pub(crate) const MAX_SYSCALLS: usize = 768;

/// Single entry in the resolution table.
#[repr(C)]
#[derive(Copy, Clone)]
pub(crate) struct SyscallEntry {
    /// `rsc_hash` of the `Nt*` name (we never store names themselves).
    pub hash: u32,
    /// System Service Number — the kernel dispatch index. Assigned after
    /// entries are sorted by `fn_addr`.
    pub ssn: u32,
    /// Absolute address of the `Nt*` stub inside ntdll.
    pub fn_addr: usize,
    /// Absolute address of a `syscall; ret` / `sysenter; ret` slide to
    /// jump through (HalosGate-style). `0` means no slide was found.
    pub syscall_addr: usize,
}

impl SyscallEntry {
    const ZERO: Self = Self {
        hash: 0,
        ssn: 0,
        fn_addr: 0,
        syscall_addr: 0,
    };
}

struct SyscallTable {
    count: u32,
    entries: [SyscallEntry; MAX_SYSCALLS],
}

/// Tri-state init flag. Must be updated with `AcqRel` compare-exchange
/// so losers of the populate race observe a consistent table once the
/// winner reaches `READY`.
const STATE_UNINIT: u8 = 0;
const STATE_POPULATING: u8 = 1;
const STATE_READY: u8 = 2;

struct Lazy {
    cell: UnsafeCell<SyscallTable>,
    state: AtomicU8,
}

// SAFETY: `state` serializes writers via compare-exchange; readers spin
// on `STATE_POPULATING` and only dereference `cell` after `STATE_READY`
// is observed with `Acquire` ordering.
unsafe impl Sync for Lazy {}

static TABLE: Lazy = Lazy {
    cell: UnsafeCell::new(SyscallTable {
        count: 0,
        entries: [SyscallEntry::ZERO; MAX_SYSCALLS],
    }),
    state: AtomicU8::new(STATE_UNINIT),
};

/// Looks up a name hash and returns `(SSN, syscall slide address)`.
///
/// Populates the table on first call. Returns `None` if the hash is not
/// present — typically a sign of a typo in `rsc_syscall!` or an ntdll
/// version change that renamed the function.
pub fn resolve(hash: u32) -> Option<(u32, usize)> {
    ensure_populated();
    // SAFETY: after `ensure_populated` returns, the table is published
    // (Release) and further reads only observe immutable state.
    let table = unsafe { &*TABLE.cell.get() };
    let n = table.count as usize;
    let mut i = 0;
    while i < n {
        let e = &table.entries[i];
        if e.hash == hash {
            return Some((e.ssn, e.syscall_addr));
        }
        i += 1;
    }
    None
}

/// Returns a random `syscall; ret` slide address from the populated pool.
///
/// Used by the JUMPER dispatch path to scramble return-stack patterns.
/// The picker skips entries whose `syscall_addr == 0` (hooked stubs we
/// couldn't recover).
pub(crate) fn pick_random_slide() -> usize {
    ensure_populated();
    // SAFETY: see `resolve` — immutable after init.
    let table = unsafe { &*TABLE.cell.get() };
    let n = table.count as usize;
    if n == 0 {
        return 0;
    }
    let mut idx = (next_random() as usize) % n;
    // Scan forward for a non-zero slide; bail out after one full wrap.
    for _ in 0..n {
        let addr = table.entries[idx].syscall_addr;
        if addr != 0 {
            return addr;
        }
        idx = (idx + 1) % n;
    }
    0
}

/// Returns the number of resolved entries (populates on demand).
#[inline]
pub fn count() -> u32 {
    ensure_populated();
    // SAFETY: see `resolve`.
    let table = unsafe { &*TABLE.cell.get() };
    table.count
}

// --- ABI-compatible resolvers for naked stubs -----------------------------

// Returning a 16-byte struct from `extern "system"` is implementation-
// defined on Windows x64 — Rust may choose a hidden-pointer ABI, which
// breaks naked-asm callers expecting `RAX:RDX`. Splitting into two fixed
// `-> integer` functions side-steps the ambiguity entirely. One resolve
// costs a table walk; the second is served from CPU cache on the same ns.

/// Returns the System Service Number for the given function-name hash,
/// or `0` if the hash is unknown. Result is in `EAX` per MS x64 ABI.
///
/// # Safety
///
/// Called from `rsc_syscall!`-generated naked stubs.
#[no_mangle]
pub unsafe extern "system" fn __rsc_resolve_ssn(hash: u32) -> u32 {
    resolve(hash).map(|(ssn, _)| ssn).unwrap_or(0)
}

/// Returns a **random** `syscall; ret` slide from the populated pool.
/// Called by `rsc_syscall!`-generated stubs on every syscall so that the
/// kernel-return RIP / stack-trace never settles on a deterministic
/// pattern — JUMPER bounces through a different ntdll stub each time.
///
/// Takes no arguments; the pick is seeded by `xorshift32` off `rdtsc`.
/// Returns `0` only when the table is empty or every entry was hooked
/// beyond recovery.
///
/// # Safety
///
/// Called from ABI-specific naked stubs only. No preconditions.
#[no_mangle]
pub unsafe extern "system" fn __rsc_random_slide() -> usize {
    pick_random_slide()
}

#[cfg(test)]
mod abi_tests {
    use super::*;

    /// Smoke: `__rsc_resolve_ssn` agrees with the public tuple-returning
    /// `resolve` for a known-good name, and the random-slide helper
    /// returns a non-zero address (hook-free host assumption).
    #[test]
    #[cfg(all(windows, any(target_arch = "x86_64", target_arch = "x86")))]
    fn ssn_resolver_matches_public_resolve_and_random_slide_nonzero() {
        let hash = crate::hash::rsc_hash(b"NtClose");
        let ssn = unsafe { __rsc_resolve_ssn(hash) };
        let slide = unsafe { __rsc_random_slide() };
        let (pub_ssn, _) = resolve(hash).expect("NtClose must resolve");
        assert_eq!(ssn, pub_ssn);
        assert!(slide != 0, "random_slide returned null — no slides found?");
    }
}

// --- Population ------------------------------------------------------------

fn ensure_populated() {
    loop {
        match TABLE.state.load(Ordering::Acquire) {
            STATE_READY => return,
            STATE_UNINIT => {
                // Try to claim the populate role. Only one thread wins;
                // everyone else spins on `STATE_POPULATING`.
                if TABLE
                    .state
                    .compare_exchange(
                        STATE_UNINIT,
                        STATE_POPULATING,
                        Ordering::AcqRel,
                        Ordering::Acquire,
                    )
                    .is_ok()
                {
                    // SAFETY: we are the sole writer — no other thread can
                    // access `TABLE.cell` until we move `state` to READY.
                    unsafe { populate_impl() };
                    TABLE.state.store(STATE_READY, Ordering::Release);
                    return;
                }
                // Lost the race — another thread claimed it. Spin.
            }
            STATE_POPULATING => {
                core::hint::spin_loop();
            }
            _ => {
                // Should be unreachable; defensive.
                core::hint::spin_loop();
            }
        }
    }
}

/// Walks ntdll's export table and fills the global syscall table.
///
/// # Safety
///
/// Called exactly once per process (effectively) from `ensure_populated`.
/// Accesses raw pointers from the loader's data structures.
unsafe fn populate_impl() {
    // SAFETY: zeroing via `get()` is safe before any Acquire observer exists.
    let table = unsafe { &mut *TABLE.cell.get() };
    table.count = 0;

    let ntdll = match unsafe { find_ntdll() } {
        Some(p) => p as *const u8,
        None => return,
    };

    let export = match unsafe { find_export_directory(ntdll) } {
        Some(e) => e,
        None => return,
    };

    let names_ptr = unsafe { ntdll.add(export.AddressOfNames as usize) as *const u32 };
    let ordinals_ptr = unsafe { ntdll.add(export.AddressOfNameOrdinals as usize) as *const u16 };
    let funcs_ptr = unsafe { ntdll.add(export.AddressOfFunctions as usize) as *const u32 };
    let num_names = export.NumberOfNames;

    let mut count: usize = 0;

    let mut i = num_names;
    while i > 0 && count < MAX_SYSCALLS {
        i -= 1;

        // SAFETY: `i` is within `AddressOfNames` bounds per export spec.
        let name_rva = unsafe { *names_ptr.add(i as usize) };
        let name = unsafe { name_at_rva(ntdll, name_rva) };

        // Filter `Zw*` stubs: they are aliases of `Nt*` and map 1:1 to
        // kernel-side SSN dispatch. Hashing happens on the `Nt*` spelling,
        // which is what consumers will write in `rsc_syscall!` calls.
        if unsafe { *name } != b'Z' || unsafe { *name.add(1) } != b'w' {
            continue;
        }

        let ordinal = unsafe { *ordinals_ptr.add(i as usize) } as usize;
        let fn_rva = unsafe { *funcs_ptr.add(ordinal) };
        let fn_addr = unsafe { ntdll.add(fn_rva as usize) } as usize;

        // Hash as if the export had been spelled "Nt…".
        // SAFETY: `name` is the export-name pointer we just resolved; it
        // is a valid null-terminated C string inside ntdll.
        let hash = unsafe { hash_as_nt(name) };
        // Find a `syscall; ret` slide (may be 0 if hooked).
        let slide = unsafe { find_syscall_slide(fn_addr as *const u8) };

        table.entries[count] = SyscallEntry {
            hash,
            ssn: 0, // assigned below, after sorting
            fn_addr,
            syscall_addr: slide,
        };
        count += 1;
    }

    // Sort by function address → SSN is the sort index.
    insertion_sort_by_fn_addr(&mut table.entries[..count]);

    let mut idx = 0;
    while idx < count {
        table.entries[idx].ssn = idx as u32;
        idx += 1;
    }

    table.count = count as u32;
}

/// Hashes a `Zw*` C-string as if it had been spelled `Nt*`.
///
/// # Safety
///
/// `zw_name` must be a valid null-terminated C string.
unsafe fn hash_as_nt(zw_name: *const u8) -> u32 {
    // Generous fixed buffer — real NT function names cap out well below 80.
    let mut buf = [0u8; 80];
    buf[0] = b'N';
    buf[1] = b't';

    let mut j = 2usize;
    while j < buf.len() {
        // SAFETY: caller-guaranteed null-terminated string.
        let c = unsafe { *zw_name.add(j) };
        if c == 0 {
            break;
        }
        buf[j] = c;
        j += 1;
    }
    rsc_hash(&buf[..j])
}

/// Branch-free (structurally) insertion sort by `fn_addr`. O(n²) worst-case,
/// but with n ≈ 500 and native u64 swaps it's on the order of microseconds —
/// negligible next to the one-off PEB walk and export-table parse.
fn insertion_sort_by_fn_addr(entries: &mut [SyscallEntry]) {
    let n = entries.len();
    let mut i = 1;
    while i < n {
        let mut j = i;
        while j > 0 && entries[j - 1].fn_addr > entries[j].fn_addr {
            entries.swap(j - 1, j);
            j -= 1;
        }
        i += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entry_layout_is_stable() {
        // On x64: u32 + u32 + usize + usize = 24 bytes with natural alignment.
        // Layout drift is a real compatibility hazard — pin the size.
        #[cfg(target_pointer_width = "64")]
        assert_eq!(core::mem::size_of::<SyscallEntry>(), 24);

        // On x86: u32 + u32 + usize + usize = 16 bytes.
        #[cfg(target_pointer_width = "32")]
        assert_eq!(core::mem::size_of::<SyscallEntry>(), 16);
    }

    #[test]
    fn insertion_sort_orders_by_fn_addr() {
        let mut entries = [
            SyscallEntry { hash: 3, ssn: 0, fn_addr: 300, syscall_addr: 0 },
            SyscallEntry { hash: 1, ssn: 0, fn_addr: 100, syscall_addr: 0 },
            SyscallEntry { hash: 2, ssn: 0, fn_addr: 200, syscall_addr: 0 },
            SyscallEntry { hash: 4, ssn: 0, fn_addr: 400, syscall_addr: 0 },
        ];
        insertion_sort_by_fn_addr(&mut entries);
        assert_eq!(entries[0].hash, 1);
        assert_eq!(entries[1].hash, 2);
        assert_eq!(entries[2].hash, 3);
        assert_eq!(entries[3].hash, 4);
    }

    /// Live test: walk the real ntdll in this process and verify we find
    /// `NtClose`. Skipped when the static state has already been touched
    /// by another test (populate() runs once per process).
    #[test]
    #[cfg(all(windows, any(target_arch = "x86_64", target_arch = "x86")))]
    fn live_resolve_nt_close() {
        let h = crate::hash::rsc_hash(b"NtClose");
        // populate via resolve
        let result = resolve(h);
        assert!(result.is_some(), "NtClose not found (populate failure?)");
        let (ssn, slide) = result.unwrap();
        // SSN for NtClose has historically been in the 0x00..0x40 range.
        assert!(ssn < 0x1000, "unexpectedly large SSN: {ssn:#x}");
        // Slide may legitimately be 0 if the stub is hooked; just require
        // non-zero on a vanilla system (we're running tests, so no EDR hook
        // is typically present).
        assert!(slide != 0, "no JUMPER slide found for NtClose");
    }

    #[test]
    #[cfg(all(windows, any(target_arch = "x86_64", target_arch = "x86")))]
    fn live_count_reasonable() {
        let n = count();
        assert!(n >= 100, "suspiciously few entries: {n}");
        assert!((n as usize) < MAX_SYSCALLS, "table overflow");
    }
}
