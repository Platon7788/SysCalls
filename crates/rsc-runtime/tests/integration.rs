//! End-to-end runtime tests.
//!
//! Exercises the auto-generated syscall stubs from `canonical.toml`:
//! `NtAllocateVirtualMemory` → `NtProtectVirtualMemory` →
//! `NtQueryVirtualMemory` → `NtFreeVirtualMemory`, and a few others.
//! Must be run on Windows (all stubs are no-ops on other OSes by virtue
//! of not being callable at all).

#![cfg(windows)]
#![cfg(target_arch = "x86_64")] // x86/WoW64 stubs are Phase 2b placeholders

use rsc_runtime::constants::{
    MEM_COMMIT, MEM_RELEASE, MEM_RESERVE, PAGE_EXECUTE_READ, PAGE_READWRITE,
};
use rsc_runtime::error::STATUS_SUCCESS;
use rsc_runtime::syscalls::{
    NtAllocateVirtualMemory, NtFreeVirtualMemory, NtProtectVirtualMemory, NtQueryVirtualMemory,
};
use rsc_runtime::types::{HANDLE, MEMORY_BASIC_INFORMATION, NTSTATUS, NT_CURRENT_PROCESS, PVOID, SIZE_T};

#[test]
fn alloc_write_read_free_round_trip() {
    let mut base: PVOID = core::ptr::null_mut();
    let mut size: SIZE_T = 0x1000;

    // Allocate RW page.
    let status: NTSTATUS = unsafe {
        NtAllocateVirtualMemory(
            NT_CURRENT_PROCESS,
            &mut base,
            0,
            &mut size,
            MEM_COMMIT | MEM_RESERVE,
            PAGE_READWRITE,
        )
    };
    assert_eq!(status, STATUS_SUCCESS.code(), "alloc failed");
    assert!(!base.is_null());
    assert!(size >= 0x1000);

    // Write / read pattern.
    const PATTERN: u32 = 0xCAFE_BABE;
    unsafe {
        core::ptr::write(base as *mut u32, PATTERN);
        let back = core::ptr::read(base as *const u32);
        assert_eq!(back, PATTERN);
    }

    // Flip to PAGE_EXECUTE_READ.
    let mut old_protect: u32 = 0;
    let mut protect_size: SIZE_T = 0x1000;
    let mut protect_base = base;
    let status: NTSTATUS = unsafe {
        NtProtectVirtualMemory(
            NT_CURRENT_PROCESS,
            &mut protect_base,
            &mut protect_size,
            PAGE_EXECUTE_READ,
            &mut old_protect,
        )
    };
    assert_eq!(status, STATUS_SUCCESS.code(), "protect failed");
    assert_eq!(old_protect, PAGE_READWRITE);

    // Query — validate the page is now execute-read.
    let mut info = MEMORY_BASIC_INFORMATION {
        BaseAddress: core::ptr::null_mut(),
        AllocationBase: core::ptr::null_mut(),
        AllocationProtect: 0,
        PartitionId: 0,
        RegionSize: 0,
        State: 0,
        Protect: 0,
        Type: 0,
    };
    let mut ret_len: SIZE_T = 0;
    let status: NTSTATUS = unsafe {
        NtQueryVirtualMemory(
            NT_CURRENT_PROCESS,
            base,
            0, // MemoryBasicInformation
            &mut info as *mut _ as *mut _,
            core::mem::size_of::<MEMORY_BASIC_INFORMATION>(),
            &mut ret_len,
        )
    };
    assert_eq!(status, STATUS_SUCCESS.code(), "query failed");
    assert_eq!(info.Protect, PAGE_EXECUTE_READ);

    // Free.
    let mut zero: SIZE_T = 0;
    let status: NTSTATUS =
        unsafe { NtFreeVirtualMemory(NT_CURRENT_PROCESS, &mut base, &mut zero, MEM_RELEASE) };
    assert_eq!(status, STATUS_SUCCESS.code(), "free failed");
}

#[test]
fn nt_close_bogus_handle_returns_error() {
    let status: NTSTATUS =
        unsafe { rsc_runtime::syscalls::NtClose(0xDEADBEEFusize as HANDLE) };
    assert!(status < 0, "NtClose(bogus) should fail, got {:#010x}", status as u32);
}

#[test]
fn table_populated_with_expected_count() {
    let n = rsc_runtime::count();
    assert!(n >= 400, "syscall table looks short: {n}");
    assert!(n < 1024, "syscall table looks bogus: {n}");
}

#[test]
fn resolve_nt_close_is_stable() {
    let h = rsc_runtime::rsc_hash(b"NtClose");
    let (ssn, slide) = rsc_runtime::resolve(h).expect("NtClose must resolve");
    assert!(ssn < 0x1000, "SSN out of range: {ssn:#x}");
    assert!(slide != 0, "JUMPER slide for NtClose is null");
}
