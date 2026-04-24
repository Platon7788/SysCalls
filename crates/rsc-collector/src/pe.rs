//! PE header parsing — just enough to locate the CodeView (RSDS) debug
//! entry in ntdll.dll and pull out the triple `(PDB name, GUID, Age)` that
//! the Symbol Server keys PDBs by.
//!
//! We parse by reading the file manually: that avoids an extra dep
//! (`object` / `goblin`), keeps binary size small, and makes the code
//! explicit about which fields matter. See §S-03.

use std::fs;
use std::path::Path;

use sha2::{Digest, Sha256};
use tracing::debug;

use crate::error::CollectError;

const IMAGE_DOS_SIGNATURE: u16 = 0x5A4D; // "MZ"
const IMAGE_NT_SIGNATURE: u32 = 0x0000_4550; // "PE\0\0"
const IMAGE_DIRECTORY_ENTRY_DEBUG: usize = 6;
const IMAGE_DEBUG_TYPE_CODEVIEW: u32 = 2;
const CODEVIEW_RSDS_MAGIC: u32 = 0x5344_5352; // "RSDS"

/// PE CodeView reference into a PDB on the Symbol Server.
#[derive(Debug, Clone)]
pub struct PdbRef {
    /// `.pdb` filename as recorded in the CodeView entry (e.g. `ntdll.pdb`).
    pub pdb_name: String,
    /// 16-byte GUID — uppercase-hex-without-dashes form is what
    /// the Symbol Server URL uses.
    pub guid: [u8; 16],
    /// Age — an incrementing build sequence number.
    pub age: u32,
}

impl PdbRef {
    /// GUID in the Symbol Server URL format: canonical dashless uppercase
    /// hex with Data1/2/3 in **network byte order** (Data1/2/3 are stored
    /// little-endian in the file, but transmitted big-endian everywhere else).
    ///
    /// Layout:
    /// ```text
    ///   Data1 (4 bytes LE in file → 8 hex chars BE)
    ///   Data2 (2 bytes LE → 4 hex BE)
    ///   Data3 (2 bytes LE → 4 hex BE)
    ///   Data4 (8 bytes, network order — straight hex)
    /// ```
    pub fn guid_hex(&self) -> String {
        let g = self.guid;
        format!(
            "{:02X}{:02X}{:02X}{:02X}{:02X}{:02X}{:02X}{:02X}{:02X}{:02X}{:02X}{:02X}{:02X}{:02X}{:02X}{:02X}",
            g[3],  g[2], g[1], g[0],     // Data1 swapped
            g[5],  g[4],                  // Data2 swapped
            g[7],  g[6],                  // Data3 swapped
            g[8],  g[9],
            g[10], g[11], g[12], g[13], g[14], g[15],
        )
    }

    /// Symbol-Server path segment: `{GUID}{AGE}` in uppercase hex.
    pub fn sym_path(&self) -> String {
        format!("{}{:X}", self.guid_hex(), self.age)
    }

    /// Microsoft's CodeView GUID string with dashes, for meta display only.
    pub fn guid_display(&self) -> String {
        let g = self.guid;
        format!(
            "{:02X}{:02X}{:02X}{:02X}-{:02X}{:02X}-{:02X}{:02X}-{:02X}{:02X}-{:02X}{:02X}{:02X}{:02X}{:02X}{:02X}",
            g[3], g[2], g[1], g[0],      // Data1 (little-endian u32)
            g[5], g[4],                   // Data2 (little-endian u16)
            g[7], g[6],                   // Data3 (little-endian u16)
            g[8], g[9],                   // Data4[0..2] (big-endian)
            g[10], g[11], g[12], g[13], g[14], g[15], // Data4[2..8]
        )
    }
}

/// Metadata captured alongside the PDB reference.
#[derive(Debug, Clone)]
pub struct ImageMeta {
    pub path: String,
    pub file_size: u64,
    pub sha256: String,
    pub pdb: PdbRef,
    /// Raw mapped-image bytes so callers can read stub bytes later.
    pub bytes: Vec<u8>,
}

/// Parses `ntdll.dll` and pulls out the CodeView RSDS metadata.
///
/// The DOS header contains `e_lfanew` — the offset of the NT headers. The
/// optional header has a `DataDirectory[DEBUG]` pointer to an array of
/// `IMAGE_DEBUG_DIRECTORY` entries; we scan for the CodeView one, follow
/// its data pointer, and decode the RSDS record.
pub fn read_ntdll(path: &Path) -> Result<ImageMeta, CollectError> {
    let bytes = fs::read(path).map_err(|_| CollectError::NtdllUnavailable {
        path: path.to_path_buf(),
    })?;
    let file_size = bytes.len() as u64;

    debug!(path = %path.display(), file_size, "loaded ntdll bytes");

    let pdb = parse_codeview(&bytes)?;

    let sha256 = {
        let mut hasher = Sha256::new();
        hasher.update(&bytes);
        let out = hasher.finalize();
        hex_lower(&out)
    };

    Ok(ImageMeta {
        path: path.to_string_lossy().into_owned(),
        file_size,
        sha256,
        pdb,
        bytes,
    })
}

fn parse_codeview(bytes: &[u8]) -> Result<PdbRef, CollectError> {
    // DOS header
    if bytes.len() < 0x40 {
        return Err(CollectError::PeParse("too small for DOS header".into()));
    }
    let e_magic = u16_at(bytes, 0)?;
    if e_magic != IMAGE_DOS_SIGNATURE {
        return Err(CollectError::PeParse(format!("bad DOS magic {e_magic:#x}")));
    }
    let e_lfanew = u32_at(bytes, 0x3C)? as usize;

    // NT headers
    let nt_sig = u32_at(bytes, e_lfanew)?;
    if nt_sig != IMAGE_NT_SIGNATURE {
        return Err(CollectError::PeParse(format!("bad NT magic {nt_sig:#x}")));
    }

    let file_header_off = e_lfanew + 4;
    // IMAGE_FILE_HEADER is 20 bytes. Optional header starts right after.
    let machine = u16_at(bytes, file_header_off)?;
    let size_of_optional_header = u16_at(bytes, file_header_off + 0x10)? as usize;
    let optional_header_off = file_header_off + 20;

    let opt_magic = u16_at(bytes, optional_header_off)?;
    let (is_pe64, data_dir_off) = match opt_magic {
        0x20B => (true, optional_header_off + 0x70),  // IMAGE_OPTIONAL_HEADER64, dd at +0x70
        0x10B => (false, optional_header_off + 0x60), // IMAGE_OPTIONAL_HEADER32, dd at +0x60
        m => return Err(CollectError::PeParse(format!("unknown optional magic {m:#x}"))),
    };
    if size_of_optional_header < data_dir_off - optional_header_off + 0x08 * 16 {
        return Err(CollectError::PeParse(
            "optional header too small for DataDirectory".into(),
        ));
    }

    debug!(
        machine = format!("{machine:#x}"),
        is_pe64,
        "parsed NT optional header"
    );

    // DataDirectory[DEBUG] = (RVA, Size) each 4 bytes.
    let debug_dir_rva = u32_at(bytes, data_dir_off + IMAGE_DIRECTORY_ENTRY_DEBUG * 8)?;
    let debug_dir_size = u32_at(bytes, data_dir_off + IMAGE_DIRECTORY_ENTRY_DEBUG * 8 + 4)?;
    if debug_dir_rva == 0 || debug_dir_size == 0 {
        return Err(CollectError::NoCodeView);
    }

    // RVA in a file-on-disk is *almost* the same as a file offset — but not
    // quite: sections can be aligned differently. We do a proper RVA→file
    // offset translation via the section table.
    let debug_dir_file = rva_to_file_offset(bytes, file_header_off, debug_dir_rva)?;

    // IMAGE_DEBUG_DIRECTORY: 28 bytes per entry. `Type` at +0x0C,
    // `SizeOfData` at +0x10, `AddressOfRawData` at +0x14,
    // `PointerToRawData` at +0x18.
    let count = debug_dir_size as usize / 28;
    for i in 0..count {
        let entry = debug_dir_file + i * 28;
        let dtype = u32_at(bytes, entry + 0x0C)?;
        if dtype != IMAGE_DEBUG_TYPE_CODEVIEW {
            continue;
        }
        let size_of_data = u32_at(bytes, entry + 0x10)? as usize;
        let ptr_to_raw = u32_at(bytes, entry + 0x18)? as usize;

        // RSDS layout: magic(4) + guid(16) + age(4) + name(NUL-terminated).
        let end = ptr_to_raw
            .checked_add(size_of_data)
            .ok_or_else(|| CollectError::PeParse("CodeView size overflow".into()))?;
        if end > bytes.len() {
            return Err(CollectError::PeParse(
                "CodeView entry extends past file".into(),
            ));
        }
        let blob = &bytes[ptr_to_raw..end];
        if blob.len() < 24 {
            return Err(CollectError::PeParse("CodeView blob too short".into()));
        }
        let magic = u32_at(blob, 0)?;
        if magic != CODEVIEW_RSDS_MAGIC {
            return Err(CollectError::PeParse(format!(
                "unsupported CodeView magic {magic:#x}"
            )));
        }
        let mut guid = [0u8; 16];
        guid.copy_from_slice(&blob[4..20]);
        let age = u32_at(blob, 20)?;
        let name_bytes = &blob[24..];
        let nul = name_bytes.iter().position(|&b| b == 0).unwrap_or(name_bytes.len());
        let pdb_name = core::str::from_utf8(&name_bytes[..nul])
            .map_err(|_| CollectError::PeParse("PDB name is not UTF-8".into()))?
            .to_string();
        // Keep just the basename — some builds bake in absolute paths.
        let pdb_name = Path::new(&pdb_name)
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or(pdb_name);

        return Ok(PdbRef { pdb_name, guid, age });
    }

    Err(CollectError::NoCodeView)
}

/// Translate an RVA to an in-file offset using the PE section table.
fn rva_to_file_offset(
    bytes: &[u8],
    file_header_off: usize,
    rva: u32,
) -> Result<usize, CollectError> {
    let number_of_sections = u16_at(bytes, file_header_off + 2)? as usize;
    let size_of_optional_header = u16_at(bytes, file_header_off + 0x10)? as usize;
    let section_table_off = file_header_off + 20 + size_of_optional_header;

    for i in 0..number_of_sections {
        let section = section_table_off + i * 40; // IMAGE_SECTION_HEADER = 40 bytes
        let virtual_size = u32_at(bytes, section + 0x08)?;
        let virtual_address = u32_at(bytes, section + 0x0C)?;
        let size_of_raw = u32_at(bytes, section + 0x10)?;
        let pointer_to_raw = u32_at(bytes, section + 0x14)?;

        let eff_size = virtual_size.max(size_of_raw);
        if rva >= virtual_address && rva < virtual_address + eff_size {
            let delta = rva - virtual_address;
            return Ok((pointer_to_raw + delta) as usize);
        }
    }
    Err(CollectError::PeParse(format!(
        "RVA {rva:#x} not inside any section"
    )))
}

// --- Tiny byte-read helpers (no dep, no unsafe) ---------------------------

fn u16_at(bytes: &[u8], off: usize) -> Result<u16, CollectError> {
    bytes
        .get(off..off + 2)
        .and_then(|s| s.try_into().ok())
        .map(u16::from_le_bytes)
        .ok_or_else(|| CollectError::PeParse(format!("u16 read out of bounds at {off:#x}")))
}

fn u32_at(bytes: &[u8], off: usize) -> Result<u32, CollectError> {
    bytes
        .get(off..off + 4)
        .and_then(|s| s.try_into().ok())
        .map(u32::from_le_bytes)
        .ok_or_else(|| CollectError::PeParse(format!("u32 read out of bounds at {off:#x}")))
}

fn hex_lower(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}
