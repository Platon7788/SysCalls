//! Core NT types used across the runtime and its C surface.
//!
//! All structures are `#[repr(C)]`. Pointer types are raw (`*mut` / `*const`)
//! to cross FFI boundaries safely — see §S-07.
//!
//! Field names intentionally mirror the Windows SDK (PascalCase) so the
//! generated C header (`rsc.h`, Phase 7) remains drop-in compatible.

#![allow(non_snake_case)]

use core::ffi::c_void;

// --- Primitive aliases (match Windows SDK) --------------------------------

pub type HANDLE = *mut c_void;
pub type PVOID = *mut c_void;
pub type LPVOID = *mut c_void;

pub type NTSTATUS = i32;
pub type BOOL = i32;
pub type BOOLEAN = u8;

pub type UCHAR = u8;
pub type USHORT = u16;
pub type ULONG = u32;
pub type ULONG64 = u64;
pub type ULONG_PTR = usize;
pub type SIZE_T = usize;
pub type ACCESS_MASK = u32;
pub type LARGE_INTEGER = i64;
pub type ULONGLONG = u64;

/// Pseudo-handle for the current process.
pub const NT_CURRENT_PROCESS: HANDLE = -1isize as HANDLE;

/// Pseudo-handle for the current thread.
pub const NT_CURRENT_THREAD: HANDLE = -2isize as HANDLE;

// --- NT string & object descriptor types ----------------------------------

/// Counted UTF-16 string used throughout the NT API.
#[repr(C)]
#[derive(Copy, Clone)]
pub struct UNICODE_STRING {
    pub Length: USHORT,
    pub MaximumLength: USHORT,
    pub Buffer: *mut u16,
}

pub type PUNICODE_STRING = *mut UNICODE_STRING;

/// Object attributes block for `NtCreate*` / `NtOpen*` calls.
#[repr(C)]
#[derive(Copy, Clone)]
pub struct OBJECT_ATTRIBUTES {
    pub Length: ULONG,
    pub RootDirectory: HANDLE,
    pub ObjectName: PUNICODE_STRING,
    pub Attributes: ULONG,
    pub SecurityDescriptor: PVOID,
    pub SecurityQualityOfService: PVOID,
}

pub type POBJECT_ATTRIBUTES = *mut OBJECT_ATTRIBUTES;

/// I/O status block returned by most file / device operations.
#[repr(C)]
pub struct IO_STATUS_BLOCK {
    pub Status: NTSTATUS,
    pub Information: ULONG_PTR,
}

pub type PIO_STATUS_BLOCK = *mut IO_STATUS_BLOCK;

/// Identifies a process / thread pair.
#[repr(C)]
#[derive(Copy, Clone)]
pub struct CLIENT_ID {
    pub UniqueProcess: HANDLE,
    pub UniqueThread: HANDLE,
}

pub type PCLIENT_ID = *mut CLIENT_ID;

// --- Memory information ---------------------------------------------------

/// Layout must match `MEMORY_BASIC_INFORMATION` in `winnt.h`.
#[repr(C)]
#[derive(Copy, Clone)]
pub struct MEMORY_BASIC_INFORMATION {
    pub BaseAddress: PVOID,
    pub AllocationBase: PVOID,
    pub AllocationProtect: ULONG,
    pub PartitionId: USHORT,
    pub RegionSize: SIZE_T,
    pub State: ULONG,
    pub Protect: ULONG,
    pub Type: ULONG,
}

pub type PMEMORY_BASIC_INFORMATION = *mut MEMORY_BASIC_INFORMATION;

/// Classes for `NtQueryVirtualMemory`.
#[repr(C)]
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum MEMORY_INFORMATION_CLASS {
    MemoryBasicInformation = 0,
    MemoryWorkingSetInformation = 1,
    MemoryMappedFilenameInformation = 2,
    MemoryRegionInformation = 3,
    MemoryWorkingSetExInformation = 4,
}
