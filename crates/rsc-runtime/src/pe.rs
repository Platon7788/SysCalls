//! Minimal PE / COFF header structures, used to traverse ntdll's export
//! directory. Only the fields we need are modelled — everything else is
//! padding.
//!
//! All structs are `#[repr(C)]` and match the canonical `winnt.h` layout.
//! See §S-03 (bounds checks, magic validation).

#![allow(non_snake_case)]

pub const IMAGE_DOS_SIGNATURE: u16 = 0x5A4D; // "MZ"
pub const IMAGE_NT_SIGNATURE: u32 = 0x0000_4550; // "PE\0\0"

/// DOS header (first 64 bytes of every PE image).
#[repr(C)]
pub struct ImageDosHeader {
    pub e_magic: u16,
    e_cblp: u16,
    e_cp: u16,
    e_crlc: u16,
    e_cparhdr: u16,
    e_minalloc: u16,
    e_maxalloc: u16,
    e_ss: u16,
    e_sp: u16,
    e_csum: u16,
    e_ip: u16,
    e_cs: u16,
    e_lfarlc: u16,
    e_ovno: u16,
    e_res: [u16; 4],
    e_oemid: u16,
    e_oeminfo: u16,
    e_res2: [u16; 10],
    pub e_lfanew: i32,
}

/// COFF file header that follows the `PE\0\0` signature.
#[repr(C)]
pub struct ImageFileHeader {
    pub Machine: u16,
    pub NumberOfSections: u16,
    pub TimeDateStamp: u32,
    pub PointerToSymbolTable: u32,
    pub NumberOfSymbols: u32,
    pub SizeOfOptionalHeader: u16,
    pub Characteristics: u16,
}

/// One entry of the data-directory array (RVA + size). The Export
/// directory is index `0`.
#[repr(C)]
#[derive(Copy, Clone)]
pub struct ImageDataDirectory {
    pub VirtualAddress: u32,
    pub Size: u32,
}

pub const IMAGE_DIRECTORY_ENTRY_EXPORT: usize = 0;
pub const IMAGE_NUMBEROF_DIRECTORY_ENTRIES: usize = 16;

/// 64-bit optional header (immediately follows `ImageFileHeader`).
#[allow(dead_code)] // referenced only in `ImageNtHeaders` under #[cfg(target_arch = "x86_64")]
#[repr(C)]
pub struct ImageOptionalHeader64 {
    pub Magic: u16,
    MajorLinkerVersion: u8,
    MinorLinkerVersion: u8,
    SizeOfCode: u32,
    SizeOfInitializedData: u32,
    SizeOfUninitializedData: u32,
    AddressOfEntryPoint: u32,
    BaseOfCode: u32,
    ImageBase: u64,
    SectionAlignment: u32,
    FileAlignment: u32,
    MajorOperatingSystemVersion: u16,
    MinorOperatingSystemVersion: u16,
    MajorImageVersion: u16,
    MinorImageVersion: u16,
    MajorSubsystemVersion: u16,
    MinorSubsystemVersion: u16,
    Win32VersionValue: u32,
    SizeOfImage: u32,
    SizeOfHeaders: u32,
    CheckSum: u32,
    Subsystem: u16,
    DllCharacteristics: u16,
    SizeOfStackReserve: u64,
    SizeOfStackCommit: u64,
    SizeOfHeapReserve: u64,
    SizeOfHeapCommit: u64,
    LoaderFlags: u32,
    NumberOfRvaAndSizes: u32,
    pub DataDirectory: [ImageDataDirectory; IMAGE_NUMBEROF_DIRECTORY_ENTRIES],
}

/// 32-bit optional header.
#[allow(dead_code)] // referenced only in `ImageNtHeaders` under #[cfg(target_arch = "x86")]
#[repr(C)]
pub struct ImageOptionalHeader32 {
    pub Magic: u16,
    MajorLinkerVersion: u8,
    MinorLinkerVersion: u8,
    SizeOfCode: u32,
    SizeOfInitializedData: u32,
    SizeOfUninitializedData: u32,
    AddressOfEntryPoint: u32,
    BaseOfCode: u32,
    BaseOfData: u32,
    ImageBase: u32,
    SectionAlignment: u32,
    FileAlignment: u32,
    MajorOperatingSystemVersion: u16,
    MinorOperatingSystemVersion: u16,
    MajorImageVersion: u16,
    MinorImageVersion: u16,
    MajorSubsystemVersion: u16,
    MinorSubsystemVersion: u16,
    Win32VersionValue: u32,
    SizeOfImage: u32,
    SizeOfHeaders: u32,
    CheckSum: u32,
    Subsystem: u16,
    DllCharacteristics: u16,
    SizeOfStackReserve: u32,
    SizeOfStackCommit: u32,
    SizeOfHeapReserve: u32,
    SizeOfHeapCommit: u32,
    LoaderFlags: u32,
    NumberOfRvaAndSizes: u32,
    pub DataDirectory: [ImageDataDirectory; IMAGE_NUMBEROF_DIRECTORY_ENTRIES],
}

/// NT headers for the current architecture (64-bit on x64, 32-bit on x86).
#[cfg(target_arch = "x86_64")]
#[repr(C)]
pub struct ImageNtHeaders {
    pub Signature: u32,
    pub FileHeader: ImageFileHeader,
    pub OptionalHeader: ImageOptionalHeader64,
}

#[cfg(target_arch = "x86")]
#[repr(C)]
pub struct ImageNtHeaders {
    pub Signature: u32,
    pub FileHeader: ImageFileHeader,
    pub OptionalHeader: ImageOptionalHeader32,
}

/// Export directory (pointed to by `DataDirectory[EXPORT].VirtualAddress`).
#[repr(C)]
pub struct ImageExportDirectory {
    Characteristics: u32,
    TimeDateStamp: u32,
    MajorVersion: u16,
    MinorVersion: u16,
    pub Name: u32,
    pub Base: u32,
    pub NumberOfFunctions: u32,
    pub NumberOfNames: u32,
    pub AddressOfFunctions: u32,   // RVA -> u32[]
    pub AddressOfNames: u32,       // RVA -> u32[] (name RVAs)
    pub AddressOfNameOrdinals: u32, // RVA -> u16[]
}

/// Locates the export directory of a loaded PE image.
///
/// Returns `Some((export_dir_ptr, module_base))` on success. Fails if the
/// image's magic bytes don't validate.
///
/// # Safety
///
/// `module_base` must be the load address of a valid, mapped PE image
/// (e.g. a module pointer obtained via the PEB loader list).
pub unsafe fn find_export_directory(
    module_base: *const u8,
) -> Option<&'static ImageExportDirectory> {
    if module_base.is_null() {
        return None;
    }

    // SAFETY: DOS header lives at `module_base + 0` and is always readable
    // for a mapped PE image.
    let dos = unsafe { &*(module_base as *const ImageDosHeader) };
    if dos.e_magic != IMAGE_DOS_SIGNATURE {
        return None;
    }
    if dos.e_lfanew <= 0 {
        return None;
    }

    // SAFETY: `e_lfanew` is the file-offset of the NT headers; on a
    // mapped image, the RVA equals the VA offset.
    let nt = unsafe {
        &*(module_base.add(dos.e_lfanew as usize) as *const ImageNtHeaders)
    };
    if nt.Signature != IMAGE_NT_SIGNATURE {
        return None;
    }

    let export_dir = nt.OptionalHeader.DataDirectory[IMAGE_DIRECTORY_ENTRY_EXPORT];
    if export_dir.VirtualAddress == 0 || export_dir.Size == 0 {
        return None;
    }

    // SAFETY: export directory RVA is within the image per the NT spec.
    let export = unsafe {
        &*(module_base.add(export_dir.VirtualAddress as usize) as *const ImageExportDirectory)
    };
    Some(export)
}

/// Resolves a name-table RVA into a C-string pointer inside the image.
///
/// # Safety
///
/// `base` must be the image base. `rva` must be a RVA obtained from the
/// `AddressOfNames` table of a valid export directory.
#[inline]
pub unsafe fn name_at_rva(base: *const u8, rva: u32) -> *const u8 {
    // SAFETY: caller guarantees `base + rva` lies within the mapped image.
    unsafe { base.add(rva as usize) }
}
