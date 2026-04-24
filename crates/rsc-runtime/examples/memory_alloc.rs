//! Realistic allocator example: allocate a large region (2 MiB), write
//! a pattern spanning multiple pages, flip protection between RW and RX,
//! then free.
//!
//! Uses the canonical-generated stubs in `rsc_runtime::syscalls::*`.

#![cfg(windows)]
#![cfg(target_arch = "x86_64")]

use rsc_runtime::constants::*;
use rsc_runtime::error::STATUS_SUCCESS;
use rsc_runtime::syscalls::{NtAllocateVirtualMemory, NtFreeVirtualMemory, NtProtectVirtualMemory};
use rsc_runtime::types::*;

const REGION: SIZE_T = 2 * 1024 * 1024; // 2 MiB

fn main() {
    let mut base: PVOID = core::ptr::null_mut();
    let mut size: SIZE_T = REGION;

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
    assert_eq!(status, STATUS_SUCCESS.code(), "alloc failed: {status:#x}");
    println!("allocated {size} bytes at {base:p}");

    // Fill the whole region with a pattern across page boundaries.
    // SAFETY: `base` is a fresh RW allocation of `size` bytes.
    let slice = unsafe { core::slice::from_raw_parts_mut(base as *mut u32, size / 4) };
    for (i, v) in slice.iter_mut().enumerate() {
        *v = i as u32;
    }
    let mid_page = slice[size / 4 / 2];
    println!("mid-region word = {mid_page:#x} (expected {:#x})", size / 4 / 2);

    // Promote to execute-read.
    let mut old: ULONG = 0;
    let mut prot_size: SIZE_T = size;
    let mut prot_base = base;
    let status = unsafe {
        NtProtectVirtualMemory(
            NT_CURRENT_PROCESS,
            &mut prot_base,
            &mut prot_size,
            PAGE_EXECUTE_READ,
            &mut old,
        )
    };
    assert_eq!(status, STATUS_SUCCESS.code(), "protect failed: {status:#x}");
    println!("flipped protection RW → RX, old = {old:#x}");

    // Free.
    let mut zero: SIZE_T = 0;
    let status = unsafe {
        NtFreeVirtualMemory(NT_CURRENT_PROCESS, &mut base, &mut zero, MEM_RELEASE)
    };
    assert_eq!(status, STATUS_SUCCESS.code(), "free failed: {status:#x}");
    println!("freed 2 MiB region");
}
