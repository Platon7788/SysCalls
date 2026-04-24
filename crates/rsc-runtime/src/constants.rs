//! Named constants (page protection, allocation, access masks, …).
//!
//! Keep this file minimal — only add constants that are used by
//! consumers of the public API or needed by tests / examples. Internal
//! helpers should use literals to avoid polluting the surface.

// --- Page protection (PAGE_*) ---------------------------------------------

pub const PAGE_NOACCESS: u32 = 0x01;
pub const PAGE_READONLY: u32 = 0x02;
pub const PAGE_READWRITE: u32 = 0x04;
pub const PAGE_WRITECOPY: u32 = 0x08;
pub const PAGE_EXECUTE: u32 = 0x10;
pub const PAGE_EXECUTE_READ: u32 = 0x20;
pub const PAGE_EXECUTE_READWRITE: u32 = 0x40;
pub const PAGE_EXECUTE_WRITECOPY: u32 = 0x80;
pub const PAGE_GUARD: u32 = 0x100;
pub const PAGE_NOCACHE: u32 = 0x200;
pub const PAGE_WRITECOMBINE: u32 = 0x400;

// --- Virtual memory allocation types (MEM_*) ------------------------------

pub const MEM_COMMIT: u32 = 0x0000_1000;
pub const MEM_RESERVE: u32 = 0x0000_2000;
pub const MEM_DECOMMIT: u32 = 0x0000_4000;
pub const MEM_RELEASE: u32 = 0x0000_8000;
pub const MEM_FREE: u32 = 0x0001_0000;
pub const MEM_PRIVATE: u32 = 0x0002_0000;
pub const MEM_MAPPED: u32 = 0x0004_0000;
pub const MEM_RESET: u32 = 0x0008_0000;
pub const MEM_TOP_DOWN: u32 = 0x0010_0000;
pub const MEM_WRITE_WATCH: u32 = 0x0020_0000;
pub const MEM_PHYSICAL: u32 = 0x0040_0000;
pub const MEM_ROTATE: u32 = 0x0080_0000;
pub const MEM_LARGE_PAGES: u32 = 0x2000_0000;
pub const MEM_4MB_PAGES: u32 = 0x8000_0000;

// --- Standard access rights (ACCESS_MASK) ---------------------------------

pub const DELETE: u32 = 0x0001_0000;
pub const READ_CONTROL: u32 = 0x0002_0000;
pub const WRITE_DAC: u32 = 0x0004_0000;
pub const WRITE_OWNER: u32 = 0x0008_0000;
pub const SYNCHRONIZE: u32 = 0x0010_0000;

pub const STANDARD_RIGHTS_REQUIRED: u32 = 0x000F_0000;
pub const STANDARD_RIGHTS_READ: u32 = READ_CONTROL;
pub const STANDARD_RIGHTS_WRITE: u32 = READ_CONTROL;
pub const STANDARD_RIGHTS_EXECUTE: u32 = READ_CONTROL;
pub const STANDARD_RIGHTS_ALL: u32 = 0x001F_0000;

// --- Process access rights (PROCESS_*) ------------------------------------

pub const PROCESS_TERMINATE: u32 = 0x0001;
pub const PROCESS_CREATE_THREAD: u32 = 0x0002;
pub const PROCESS_SET_SESSIONID: u32 = 0x0004;
pub const PROCESS_VM_OPERATION: u32 = 0x0008;
pub const PROCESS_VM_READ: u32 = 0x0010;
pub const PROCESS_VM_WRITE: u32 = 0x0020;
pub const PROCESS_DUP_HANDLE: u32 = 0x0040;
pub const PROCESS_CREATE_PROCESS: u32 = 0x0080;
pub const PROCESS_SET_QUOTA: u32 = 0x0100;
pub const PROCESS_SET_INFORMATION: u32 = 0x0200;
pub const PROCESS_QUERY_INFORMATION: u32 = 0x0400;
pub const PROCESS_SUSPEND_RESUME: u32 = 0x0800;
pub const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;
pub const PROCESS_SET_LIMITED_INFORMATION: u32 = 0x2000;
pub const PROCESS_ALL_ACCESS: u32 = STANDARD_RIGHTS_REQUIRED | SYNCHRONIZE | 0xFFFF;

// --- Thread access rights (THREAD_*) --------------------------------------

pub const THREAD_TERMINATE: u32 = 0x0001;
pub const THREAD_SUSPEND_RESUME: u32 = 0x0002;
pub const THREAD_GET_CONTEXT: u32 = 0x0008;
pub const THREAD_SET_CONTEXT: u32 = 0x0010;
pub const THREAD_QUERY_INFORMATION: u32 = 0x0040;
pub const THREAD_SET_INFORMATION: u32 = 0x0020;
pub const THREAD_SET_THREAD_TOKEN: u32 = 0x0080;
pub const THREAD_IMPERSONATE: u32 = 0x0100;
pub const THREAD_DIRECT_IMPERSONATION: u32 = 0x0200;
pub const THREAD_SET_LIMITED_INFORMATION: u32 = 0x0400;
pub const THREAD_QUERY_LIMITED_INFORMATION: u32 = 0x0800;
pub const THREAD_ALL_ACCESS: u32 = STANDARD_RIGHTS_REQUIRED | SYNCHRONIZE | 0xFFFF;

// --- Object attribute flags (OBJ_*) ---------------------------------------

pub const OBJ_INHERIT: u32 = 0x0000_0002;
pub const OBJ_PERMANENT: u32 = 0x0000_0010;
pub const OBJ_EXCLUSIVE: u32 = 0x0000_0020;
pub const OBJ_CASE_INSENSITIVE: u32 = 0x0000_0040;
pub const OBJ_OPENIF: u32 = 0x0000_0080;
pub const OBJ_OPENLINK: u32 = 0x0000_0100;
pub const OBJ_KERNEL_HANDLE: u32 = 0x0000_0200;
pub const OBJ_FORCE_ACCESS_CHECK: u32 = 0x0000_0400;
