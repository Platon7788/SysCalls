//! Shared types for reading the three DB layers and emitting the
//! canonical merge result. Mirrors the shape documented in
//! `Docs/DATABASE.md`.
//!
//! We keep this inside the CLI crate because it's the only consumer — the
//! runtime's `build.rs` has its own, narrower canonical reader.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

// --- Auto layer (db/auto/*.toml) ------------------------------------------

#[derive(Serialize, Deserialize, Debug, Default)]
pub struct AutoLayer {
    pub meta: AutoMeta,
    #[serde(rename = "syscall", default)]
    pub syscalls: Vec<AutoSyscall>,
}

#[derive(Serialize, Deserialize, Debug, Default)]
pub struct AutoMeta {
    pub schema_version: u32,
    pub build_id: String,
    pub windows_version: String,
    pub collected_at: String,
    pub collector_version: String,
    #[serde(default)]
    pub ntdll_x64: Option<toml::Value>,
    #[serde(default)]
    pub ntdll_x86: Option<toml::Value>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct AutoSyscall {
    pub name: String,
    pub ssn_x64: Option<u32>,
    pub ssn_x86: Option<u32>,
    pub arity_x64: Option<u32>,
    pub arity_x86: Option<u32>,
    pub rva_x64: Option<u32>,
    pub rva_x86: Option<u32>,
    pub stub_x64: Option<String>,
    pub stub_x86: Option<String>,
}

pub fn load_auto(path: &Path) -> Result<AutoLayer> {
    let s = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    toml::from_str(&s).with_context(|| format!("parse {}", path.display()))
}

/// Pick the lexicographically latest `db/auto/*.toml`. Retained for
/// tooling that wants a single-snapshot view (individual `rsc diff`
/// targets, etc.); `rsc merge` uses `auto_files_in` instead.
#[allow(dead_code)]
pub fn latest_auto_in(dir: &Path) -> Result<PathBuf> {
    let mut entries = auto_files_in(dir)?;
    entries
        .pop()
        .with_context(|| format!("no *.toml snapshots in {}", dir.display()))
}

/// All `*.toml` files under `dir`, sorted lexicographically (older → newer
/// for typical `{Family}_{Build}_{UBR}` names).
pub fn auto_files_in(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut entries: Vec<PathBuf> = fs::read_dir(dir)
        .with_context(|| format!("reading {}", dir.display()))?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("toml"))
        .collect();
    entries.sort();
    Ok(entries)
}

// --- Phnt layer (db/phnt/phnt.toml) ---------------------------------------

#[derive(Serialize, Deserialize, Debug, Default)]
pub struct PhntLayer {
    pub meta: PhntMeta,
    #[serde(rename = "function", default)]
    pub functions: Vec<PhntFunction>,
}

#[derive(Serialize, Deserialize, Debug, Default)]
pub struct PhntMeta {
    pub schema_version: u32,
    #[serde(default)]
    pub phnt_commit: Option<String>,
    pub parsed_at: String,
    pub types_version: String,
    #[serde(default)]
    pub source_count: usize,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PhntFunction {
    pub name: String,
    pub return_type: String,
    pub params: Vec<PhntParam>,
    #[serde(default)]
    pub min_phnt_version: Option<String>,
    #[serde(default)]
    pub source: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PhntParam {
    pub name: String,
    pub r#type: String,
    pub direction: String,
    #[serde(default)]
    pub optional: bool,
}

pub fn load_phnt(path: &Path) -> Result<PhntLayer> {
    let s = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    toml::from_str(&s).with_context(|| format!("parse {}", path.display()))
}

// --- Overrides layer (db/overrides.toml) ----------------------------------

#[derive(Serialize, Deserialize, Debug, Default)]
pub struct OverridesLayer {
    pub meta: OverridesMeta,
    #[serde(rename = "override", default)]
    pub overrides: Vec<Override>,
}

#[derive(Serialize, Deserialize, Debug, Default)]
pub struct OverridesMeta {
    pub schema_version: u32,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Override {
    pub kind: String, // "fix_signature" | "add" | "exclude" | "fix_arity" | "fix_ssn"
    pub name: String,
    pub reason: String,
    #[serde(default)]
    pub return_type: Option<String>,
    #[serde(default)]
    pub params: Vec<PhntParam>,
    #[serde(default)]
    pub ssn_x64: Option<u32>,
    #[serde(default)]
    pub ssn_x86: Option<u32>,
    #[serde(default)]
    pub arity_x64: Option<u32>,
    #[serde(default)]
    pub arity_x86: Option<u32>,
    #[serde(default)]
    pub verified_on: Vec<String>,
}

pub fn load_overrides(path: &Path) -> Result<OverridesLayer> {
    if !path.exists() {
        return Ok(OverridesLayer::default());
    }
    let s = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    toml::from_str(&s).with_context(|| format!("parse {}", path.display()))
}

// --- Canonical DB (db/canonical.toml) -------------------------------------

#[derive(Serialize, Deserialize, Debug, Default)]
pub struct CanonicalDb {
    pub meta: CanonMeta,
    #[serde(rename = "syscall", default)]
    pub syscalls: Vec<CanonSyscall>,
}

#[derive(Serialize, Deserialize, Debug, Default)]
pub struct CanonMeta {
    pub schema_version: u32,
    pub generated_at: String,
    pub merge_tool_version: String,
    pub baseline_build: String,
    #[serde(default)]
    pub phnt_commit: Option<String>,
    #[serde(default)]
    pub layers_merged: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CanonSyscall {
    pub name: String,
    /// Compile-time hash — computed via `rsc_runtime::rsc_hash`.
    pub rsc_hash: u32,
    /// Which layers contributed: `["auto", "phnt", "overrides"]` subset.
    pub sources: Vec<String>,
    /// Windows builds (by build-id) whose `auto/*.toml` contained this
    /// function. When `rsc merge` unions multiple snapshots, this records
    /// "first seen on", "also on", etc. — purely informational because
    /// SSN is resolved at runtime anyway.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub available_on: Vec<String>,
    /// From auto layer of the baseline build.
    pub ssn_x64: Option<u32>,
    pub ssn_x86: Option<u32>,
    pub arity_x64: Option<u32>,
    pub arity_x86: Option<u32>,
    pub rva_x64: Option<u32>,
    pub rva_x86: Option<u32>,
    /// From phnt / overrides.
    pub return_type: String,
    pub params: Vec<CanonParam>,
    /// Inferred from `Nt` name prefix.
    pub category: String,
    /// `exclude` override sets this — runtime skips the stub entirely.
    #[serde(default, skip_serializing_if = "core::ops::Not::not")]
    pub excluded: bool,
    /// `true` if we fell back to opaque `*mut c_void` params (no phnt match).
    #[serde(default, skip_serializing_if = "core::ops::Not::not")]
    pub opaque_signature: bool,
    #[serde(default)]
    pub min_phnt_version: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CanonParam {
    pub name: String,
    pub r#type: String,
    pub direction: String,
    #[serde(default, skip_serializing_if = "core::ops::Not::not")]
    pub optional: bool,
}

pub fn load_canonical(path: &Path) -> Result<CanonicalDb> {
    let s = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    toml::from_str(&s).with_context(|| format!("parse {}", path.display()))
}

pub fn write_canonical_atomic(path: &Path, db: &CanonicalDb) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    }
    let content = toml::to_string_pretty(db).context("serialize canonical")?;
    let tmp = path.with_extension("toml.tmp");
    fs::write(&tmp, content).with_context(|| format!("write {}", tmp.display()))?;
    fs::rename(&tmp, path).context("rename tmp → dest")?;
    Ok(())
}

/// Quick category guess based on common NT function prefixes.
pub fn infer_category(name: &str) -> &'static str {
    // Strip the "Nt" prefix before matching.
    let rest = name.strip_prefix("Nt").unwrap_or(name);
    match rest {
        s if s.starts_with("AllocateVirtualMemory")
            || s.starts_with("FreeVirtualMemory")
            || s.starts_with("ProtectVirtualMemory")
            || s.starts_with("QueryVirtualMemory")
            || s.starts_with("ReadVirtualMemory")
            || s.starts_with("WriteVirtualMemory")
            || s.starts_with("MapViewOf")
            || s.starts_with("UnmapViewOf")
            || s.starts_with("CreateSection")
            || s.starts_with("OpenSection")
            || s.starts_with("FlushVirtualMemory") => "memory",

        s if s.starts_with("CreateProcess")
            || s.starts_with("OpenProcess")
            || s.starts_with("TerminateProcess")
            || s.starts_with("SuspendProcess")
            || s.starts_with("ResumeProcess")
            || s.starts_with("QueryInformationProcess")
            || s.starts_with("SetInformationProcess") => "process",

        s if s.starts_with("CreateThread")
            || s.starts_with("OpenThread")
            || s.starts_with("TerminateThread")
            || s.starts_with("SuspendThread")
            || s.starts_with("ResumeThread")
            || s.starts_with("QueryInformationThread")
            || s.starts_with("SetInformationThread") => "thread",

        s if s.starts_with("CreateFile")
            || s.starts_with("OpenFile")
            || s.starts_with("ReadFile")
            || s.starts_with("WriteFile")
            || s.starts_with("DeviceIoControlFile")
            || s.starts_with("QueryInformationFile")
            || s.starts_with("SetInformationFile")
            || s.starts_with("QueryDirectoryFile")
            || s.starts_with("FlushBuffersFile") => "file",

        s if s.starts_with("CreateKey")
            || s.starts_with("OpenKey")
            || s.starts_with("DeleteKey")
            || s.starts_with("EnumerateKey")
            || s.starts_with("EnumerateValueKey")
            || s.starts_with("QueryKey")
            || s.starts_with("QueryValueKey")
            || s.starts_with("SetValueKey") => "registry",

        s if s.starts_with("OpenProcessToken")
            || s.starts_with("OpenThreadToken")
            || s.starts_with("AdjustPrivilegesToken")
            || s.starts_with("QueryInformationToken")
            || s.starts_with("SetInformationToken")
            || s.starts_with("DuplicateToken")
            || s.starts_with("ImpersonateAnonymousToken") => "token",

        s if s.starts_with("CreateEvent")
            || s.starts_with("CreateMutant")
            || s.starts_with("CreateSemaphore")
            || s.starts_with("WaitFor") => "sync",

        s if s.starts_with("Close")
            || s.starts_with("DuplicateObject")
            || s.starts_with("QueryObject")
            || s.starts_with("MakeTemporaryObject") => "object",

        s if s.starts_with("QuerySystemInformation")
            || s.starts_with("SetSystemInformation")
            || s.starts_with("QuerySystemTime") => "system",

        _ => "other",
    }
}
