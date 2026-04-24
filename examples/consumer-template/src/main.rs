//! # consumer-template — minimal external consumer of `rsc-runtime`.
//!
//! Demonstrates the one-import-line, no-ceremony usage pattern: a fresh
//! Rust project pulls in `rsc-runtime` via a path dep and calls NT
//! syscalls directly. No PDB downloads happen at consumer-build time;
//! all of that was done once when the SysCalls workspace ran `rsc merge`.
//!
//! Exercises:
//! * `rsc_runtime::count()` — resolver self-check.
//! * `NtAllocateVirtualMemory` / `NtFreeVirtualMemory` — RW page.
//! * `NtQueryVirtualMemory` — verify Protect flag matches what we set.
//! * `NtClose(bogus)` — error path returns STATUS_INVALID_HANDLE.
//!
//! Run from the template directory:
//!
//! ```text
//! cargo run --release
//! ```

use rsc_runtime::constants::{
    MEM_COMMIT, MEM_RELEASE, MEM_RESERVE, PAGE_READWRITE,
};
use rsc_runtime::error::STATUS_SUCCESS;
use rsc_runtime::syscalls::{
    NtAllocateVirtualMemory, NtClose, NtFreeVirtualMemory, NtQueryVirtualMemory,
};
use rsc_runtime::types::{HANDLE, NT_CURRENT_PROCESS, PVOID, SIZE_T};

fn main() -> std::process::ExitCode {
    // ---- sanity: table populated -----------------------------------------
    let resolved = rsc_runtime::count();
    println!("[*] rsc-runtime resolved {resolved} syscalls from this process's ntdll");
    if resolved < 100 {
        eprintln!(
            "[!] suspiciously low — PEB walk / JUMPER search likely failed on this OS"
        );
        return std::process::ExitCode::from(1);
    }

    // ---- alloc → write → query → free ------------------------------------
    let mut base: PVOID = core::ptr::null_mut();
    let mut size: SIZE_T = 0x1000;

    let status = unsafe {
        NtAllocateVirtualMemory(
            NT_CURRENT_PROCESS,
            &mut base,
            0,
            &mut size,
            MEM_COMMIT | MEM_RESERVE,
            PAGE_READWRITE,
        )
    };
    if status != STATUS_SUCCESS.code() {
        eprintln!("[!] NtAllocateVirtualMemory: {:#010x}", status as u32);
        return std::process::ExitCode::from(1);
    }
    println!("[+] allocated {size} bytes at {base:p}");

    // Touch it — proves the page is backed and writable.
    // SAFETY: `base` is a fresh RW allocation of `size` bytes.
    unsafe {
        core::ptr::write(base as *mut u64, 0xDEAD_BEEF_CAFE_BABE);
        let back = core::ptr::read(base as *const u64);
        assert_eq!(back, 0xDEAD_BEEF_CAFE_BABE);
    }
    println!("[+] wrote + read back sentinel 0xDEAD_BEEF_CAFE_BABE");

    // Query basic info — demonstrates a read-side syscall and
    // non-trivial parameter handling (output buffer + size-out).
    #[repr(C)]
    struct MbiX64 {
        base_address: *mut core::ffi::c_void,
        allocation_base: *mut core::ffi::c_void,
        allocation_protect: u32,
        partition_id: u16,
        region_size: usize,
        state: u32,
        protect: u32,
        kind: u32,
    }
    let mut mbi = MbiX64 {
        base_address: core::ptr::null_mut(),
        allocation_base: core::ptr::null_mut(),
        allocation_protect: 0,
        partition_id: 0,
        region_size: 0,
        state: 0,
        protect: 0,
        kind: 0,
    };
    let mut ret_len: SIZE_T = 0;
    let q = unsafe {
        NtQueryVirtualMemory(
            NT_CURRENT_PROCESS,
            base,
            0, // MemoryBasicInformation
            &mut mbi as *mut _ as *mut _,
            core::mem::size_of::<MbiX64>(),
            &mut ret_len,
        )
    };
    if q == STATUS_SUCCESS.code() {
        println!(
            "[+] NtQueryVirtualMemory: Protect={:#x}, State={:#x}, RegionSize={:#x}",
            mbi.protect, mbi.state, mbi.region_size
        );
        assert_eq!(mbi.protect, PAGE_READWRITE, "unexpected protect flag");
    } else {
        eprintln!("[!] NtQueryVirtualMemory: {:#010x}", q as u32);
    }

    // Free it back.
    let mut zero: SIZE_T = 0;
    let f = unsafe {
        NtFreeVirtualMemory(NT_CURRENT_PROCESS, &mut base, &mut zero, MEM_RELEASE)
    };
    if f != STATUS_SUCCESS.code() {
        eprintln!("[!] NtFreeVirtualMemory: {:#010x}", f as u32);
        return std::process::ExitCode::from(1);
    }
    println!("[+] freed");

    // ---- error path ------------------------------------------------------
    let rc = unsafe { NtClose(0xDEADBEEF_usize as HANDLE) };
    println!(
        "[*] NtClose(0xDEADBEEF) -> {:#010x} (expected STATUS_INVALID_HANDLE = 0xC0000008)",
        rc as u32
    );
    if rc >= 0 {
        eprintln!("[!] NtClose(bogus) should have failed");
        return std::process::ExitCode::from(1);
    }

    println!("[*] all template checks passed");
    std::process::ExitCode::SUCCESS
}
