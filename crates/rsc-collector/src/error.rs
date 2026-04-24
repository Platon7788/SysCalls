//! Typed collector errors. We use `anyhow` for top-level contextual chain
//! building but expose a small enum for programmatic handling (e.g. "the
//! Symbol Server 404'd the PDB" vs "the PDB parsed but had no `Zw*`
//! symbols").

use std::path::PathBuf;

/// High-level failure categories emitted by the collector.
#[derive(Debug)]
pub enum CollectError {
    /// Couldn't determine the current Windows build id via registry.
    VersionDetection(String),

    /// `ntdll.dll` missing or unreadable at the expected path.
    NtdllUnavailable { path: PathBuf },

    /// PE parsing of the ntdll image failed (bad magic, invalid RVAs, …).
    PeParse(String),

    /// No CodeView debug entry found in the image.
    NoCodeView,

    /// Symbol Server returned an error status (e.g. 404 — PDB not on server).
    SymbolServer { url: String, status: u16 },

    /// Network issue reaching the Symbol Server.
    Network(String),

    /// `expand.exe` failed to decompress a CAB-packaged PDB.
    CabExtract(String),

    /// DbgHelp FFI failure (`SymInitialize`, `SymLoadModuleEx`, …).
    DbgHelp { api: &'static str, os_code: u32 },

    /// The PDB parsed, but no syscall stubs were enumerated.
    NoSyscallsFound,
}

impl core::fmt::Display for CollectError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::VersionDetection(msg) => write!(f, "Windows version detection failed: {msg}"),
            Self::NtdllUnavailable { path } => {
                write!(f, "ntdll.dll unreachable at {}", path.display())
            }
            Self::PeParse(msg) => write!(f, "PE parse error: {msg}"),
            Self::NoCodeView => f.write_str("no CodeView RSDS debug entry in PE"),
            Self::SymbolServer { url, status } => {
                write!(f, "Symbol Server HTTP {status} for {url}")
            }
            Self::Network(msg) => write!(f, "network error: {msg}"),
            Self::CabExtract(msg) => write!(f, "CAB extraction via expand.exe failed: {msg}"),
            Self::DbgHelp { api, os_code } => {
                write!(f, "DbgHelp::{api} failed with os_code={os_code:#x}")
            }
            Self::NoSyscallsFound => f.write_str("no Zw* symbols found in the PDB"),
        }
    }
}

impl core::error::Error for CollectError {}
