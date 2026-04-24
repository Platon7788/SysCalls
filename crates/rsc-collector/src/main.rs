//! # `rsc-collector`
//!
//! Downloads `ntdll.pdb` from Microsoft Symbol Server, enumerates the
//! `Zw*` symbols, extracts SSN + arity from stub bytes of the on-disk
//! ntdll image, and emits `db/auto/<build>.toml`.
//!
//! See `Docs/modules/rsc_collector.md` and §S-04.

mod emit;
mod error;
mod pe;
mod pdb_reader;
mod stub_disasm;
mod symsrv;
mod version;

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use clap::{Parser, ValueEnum};
use tracing::{info, warn};
use tracing_subscriber::EnvFilter;

use crate::emit::{AutoSnapshot, NtdllMeta};
use crate::error::CollectError;
use crate::pdb_reader::PdbSession;
use crate::pe::ImageMeta;
use crate::stub_disasm::{disasm_x64, disasm_x86};
use crate::symsrv::SymSrv;
use crate::version::WindowsBuild;

#[derive(Parser, Debug)]
#[command(name = "rsc-collector", version, about = "Collect ntdll syscall snapshot from Microsoft Symbol Server")]
struct Cli {
    /// Re-download the PDB and regenerate the snapshot even if one exists.
    #[arg(long)]
    force: bool,

    /// Which architecture(s) to collect from this machine.
    #[arg(long, value_enum, default_value_t = Arch::Both)]
    arch: Arch,

    /// Override the detected build id (useful for testing with committed PDBs).
    #[arg(long)]
    build_id: Option<String>,

    /// Output directory (default: `db/auto` relative to cwd).
    #[arg(long, default_value = "db/auto")]
    db_dir: PathBuf,

    /// Increase log verbosity (repeat: `-v`, `-vv`, `-vvv`).
    #[arg(short, long, action = clap::ArgAction::Count)]
    verbose: u8,
}

#[derive(Copy, Clone, Debug, ValueEnum, PartialEq, Eq)]
enum Arch {
    X64,
    X86,
    Both,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    setup_logging(cli.verbose);

    let build = WindowsBuild::detect().context("detecting Windows build")?;
    let build_id = cli.build_id.clone().unwrap_or_else(|| build.id());
    info!(%build_id, label = %build.label(), "collecting for Windows build");

    let out_path = cli.db_dir.join(format!("{build_id}.toml"));
    if out_path.exists() && !cli.force {
        warn!(
            path = %out_path.display(),
            "snapshot already exists; pass --force to overwrite"
        );
        return Ok(());
    }

    let mut snapshot = AutoSnapshot::new(build_id, build.label());
    let symsrv = SymSrv::new();

    let collect_x64 = matches!(cli.arch, Arch::X64 | Arch::Both);
    let collect_x86 = matches!(cli.arch, Arch::X86 | Arch::Both);

    if collect_x64 {
        let path = Path::new(r"C:\Windows\System32\ntdll.dll");
        match collect_one(path, &symsrv, Bits::X64, &mut snapshot) {
            Ok(n) => info!(n, "x64 collected"),
            Err(e) => warn!(error = %e, "x64 collection failed"),
        }
    }
    if collect_x86 {
        let path = Path::new(r"C:\Windows\SysWOW64\ntdll.dll");
        if path.exists() {
            match collect_one(path, &symsrv, Bits::X86, &mut snapshot) {
                Ok(n) => info!(n, "x86 (WoW64) collected"),
                Err(e) => warn!(error = %e, "x86 collection failed"),
            }
        } else {
            warn!("SysWOW64 ntdll.dll not present — skipping x86 collection");
        }
    }

    if snapshot.syscalls.is_empty() {
        return Err(CollectError::NoSyscallsFound.into());
    }

    snapshot.sort();
    snapshot.update_counts();
    emit::write_atomic(&out_path, &snapshot).context("writing snapshot")?;

    info!(
        path = %out_path.display(),
        count = snapshot.syscalls.len(),
        "done"
    );
    Ok(())
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum Bits {
    X64,
    X86,
}

/// Collect for one (ntdll path, bit-width) pair. Returns the count of
/// `Zw*` entries merged into `snapshot`.
fn collect_one(
    ntdll_path: &Path,
    symsrv: &SymSrv,
    bits: Bits,
    snapshot: &mut AutoSnapshot,
) -> Result<usize> {
    info!(path = %ntdll_path.display(), ?bits, "reading ntdll");
    let image = pe::read_ntdll(ntdll_path).context("reading ntdll")?;

    info!(
        guid = %image.pdb.guid_display(),
        age = image.pdb.age,
        "PDB reference"
    );

    let pdb_path = symsrv.fetch(&image.pdb).context("fetching PDB")?;
    info!(pdb = %pdb_path.display(), "PDB ready");

    let session = PdbSession::open(&pdb_path).context("opening PDB via DbgHelp")?;
    // `Zw*` yields one entry per syscall. The corresponding `Nt*` aliases
    // point to the same addresses — we don't need to enumerate both.
    let symbols = session.enumerate("Zw*").context("enumerating Zw*")?;
    info!(count = symbols.len(), "Zw* symbols enumerated");

    let ntdll_meta = NtdllMeta {
        path: image.path.clone(),
        file_size: image.file_size,
        pdb_name: image.pdb.pdb_name.clone(),
        pdb_guid: image.pdb.guid_display(),
        pdb_age: image.pdb.age,
        pdb_sha256: image.sha256.clone(),
    };
    match bits {
        Bits::X64 => snapshot.meta.ntdll_x64 = Some(ntdll_meta),
        Bits::X86 => snapshot.meta.ntdll_x86 = Some(ntdll_meta),
    }

    let mut inserted = 0_usize;
    for sym in symbols {
        // Translate the `Zw*` name to its `Nt*` alias — that's what
        // `rsc-runtime` hashes.
        let nt_name = match sym.name.strip_prefix("Zw") {
            Some(tail) => format!("Nt{tail}"),
            None => continue,
        };

        let rva = sym.rva;
        let Some(stub) = stub_bytes_at(&image, rva, 32) else {
            warn!(name = %sym.name, rva = format!("{rva:#x}"), "stub bytes out of bounds");
            continue;
        };

        let stub_info = match bits {
            Bits::X64 => disasm_x64(stub),
            Bits::X86 => disasm_x86(stub),
        };

        snapshot.upsert(&nt_name, |e| match bits {
            Bits::X64 => {
                e.ssn_x64 = stub_info.ssn;
                e.arity_x64 = stub_info.arity;
                e.rva_x64 = Some(rva);
                e.stub_x64 = Some(stub_info.bytes_hex);
            }
            Bits::X86 => {
                e.ssn_x86 = stub_info.ssn;
                e.arity_x86 = stub_info.arity;
                e.rva_x86 = Some(rva);
                e.stub_x86 = Some(stub_info.bytes_hex);
            }
        });
        inserted += 1;
    }

    Ok(inserted)
}

/// Slice `len` bytes out of the on-disk image at the given RVA.
///
/// We re-use `pe::rva_to_file_offset` semantics by picking up a `Vec<u8>`
/// snapshot of the whole image — `pe::read_ntdll` already holds it. Since
/// we only need a small window for disassembly, we could also read the
/// file again; but reusing avoids a second I/O pass.
fn stub_bytes_at(image: &ImageMeta, rva: u32, len: usize) -> Option<&[u8]> {
    let file_off = rva_to_file_offset(&image.bytes, rva)?;
    image.bytes.get(file_off..file_off + len)
}

/// Duplicate of `pe::rva_to_file_offset` exposed here so we can lift only
/// the windows we need without exposing internal types.
fn rva_to_file_offset(bytes: &[u8], rva: u32) -> Option<usize> {
    let e_lfanew = u32::from_le_bytes([bytes[0x3C], bytes[0x3D], bytes[0x3E], bytes[0x3F]]) as usize;
    let file_header_off = e_lfanew + 4;
    let number_of_sections = u16::from_le_bytes([
        bytes[file_header_off],
        bytes[file_header_off + 1],
    ]) as usize;
    let size_of_optional_header = u16::from_le_bytes([
        bytes[file_header_off + 0x10],
        bytes[file_header_off + 0x11],
    ]) as usize;
    let section_table_off = file_header_off + 20 + size_of_optional_header;

    for i in 0..number_of_sections {
        let section = section_table_off + i * 40;
        let virtual_size = u32::from_le_bytes([
            bytes[section + 0x08],
            bytes[section + 0x09],
            bytes[section + 0x0A],
            bytes[section + 0x0B],
        ]);
        let virtual_address = u32::from_le_bytes([
            bytes[section + 0x0C],
            bytes[section + 0x0D],
            bytes[section + 0x0E],
            bytes[section + 0x0F],
        ]);
        let size_of_raw = u32::from_le_bytes([
            bytes[section + 0x10],
            bytes[section + 0x11],
            bytes[section + 0x12],
            bytes[section + 0x13],
        ]);
        let pointer_to_raw = u32::from_le_bytes([
            bytes[section + 0x14],
            bytes[section + 0x15],
            bytes[section + 0x16],
            bytes[section + 0x17],
        ]);
        let eff_size = virtual_size.max(size_of_raw);
        if rva >= virtual_address && rva < virtual_address + eff_size {
            let delta = rva - virtual_address;
            return Some((pointer_to_raw + delta) as usize);
        }
    }
    None
}

fn setup_logging(verbose: u8) {
    let default_filter = match verbose {
        0 => "info",
        1 => "debug",
        _ => "trace",
    };
    let filter = EnvFilter::try_from_env("RSC_LOG")
        .unwrap_or_else(|_| EnvFilter::new(default_filter));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .without_time()
        .init();
}
