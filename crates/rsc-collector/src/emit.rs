//! TOML snapshot schema per `Docs/DATABASE.md` §1.
//!
//! Entries are kept sorted by `name` so diffing two snapshots of different
//! Windows builds is a trivial line-level operation.

use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;
use tracing::info;

use crate::error::CollectError;

pub const SCHEMA_VERSION: u32 = 1;
pub const COLLECTOR_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Serialize, Deserialize, Debug, Default)]
pub struct AutoSnapshot {
    pub meta: Meta,
    /// Alphabetical list of `[[syscall]]` entries.
    #[serde(rename = "syscall", default)]
    pub syscalls: Vec<SyscallEntry>,
}

#[derive(Serialize, Deserialize, Debug, Default)]
pub struct Meta {
    pub schema_version: u32,
    pub build_id: String,
    pub windows_version: String,
    pub collected_at: String,
    pub collector_version: String,
    /// Total unique function names in this snapshot (x64 ∪ x86).
    #[serde(default)]
    pub function_count: usize,
    /// Functions with an x64 SSN recorded.
    #[serde(default, skip_serializing_if = "is_zero_usize")]
    pub function_count_x64: usize,
    /// Functions with an x86 / WoW64 SSN recorded.
    #[serde(default, skip_serializing_if = "is_zero_usize")]
    pub function_count_x86: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ntdll_x64: Option<NtdllMeta>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ntdll_x86: Option<NtdllMeta>,
}

fn is_zero_usize(v: &usize) -> bool {
    *v == 0
}

#[derive(Serialize, Deserialize, Debug)]
pub struct NtdllMeta {
    pub path: String,
    pub file_size: u64,
    pub pdb_name: String,
    pub pdb_guid: String,
    pub pdb_age: u32,
    pub pdb_sha256: String,
}

#[derive(Serialize, Deserialize, Debug, Default, Clone)]
pub struct SyscallEntry {
    pub name: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub ssn_x64: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ssn_x86: Option<u32>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub arity_x64: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arity_x86: Option<u32>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub rva_x64: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rva_x86: Option<u32>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub stub_x64: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stub_x86: Option<String>,
}

impl AutoSnapshot {
    pub fn new(build_id: String, windows_version: String) -> Self {
        let collected_at = OffsetDateTime::now_utc()
            .format(&Rfc3339)
            .unwrap_or_else(|_| "unknown".to_string());
        Self {
            meta: Meta {
                schema_version: SCHEMA_VERSION,
                build_id,
                windows_version,
                collected_at,
                collector_version: COLLECTOR_VERSION.to_string(),
                function_count: 0,
                function_count_x64: 0,
                function_count_x86: 0,
                ntdll_x64: None,
                ntdll_x86: None,
            },
            syscalls: Vec::new(),
        }
    }

    /// Insert or merge a per-arch record for a function by name.
    pub fn upsert<F: FnOnce(&mut SyscallEntry)>(&mut self, name: &str, update: F) {
        if let Some(existing) = self.syscalls.iter_mut().find(|e| e.name == name) {
            update(existing);
        } else {
            let mut fresh = SyscallEntry {
                name: name.to_string(),
                ..Default::default()
            };
            update(&mut fresh);
            self.syscalls.push(fresh);
        }
    }

    pub fn sort(&mut self) {
        self.syscalls.sort_by(|a, b| a.name.cmp(&b.name));
    }

    /// Refresh the per-arch counts from current syscall vec. Call before
    /// serializing so the `[meta]` block matches the actual contents.
    pub fn update_counts(&mut self) {
        self.meta.function_count = self.syscalls.len();
        self.meta.function_count_x64 =
            self.syscalls.iter().filter(|e| e.ssn_x64.is_some()).count();
        self.meta.function_count_x86 =
            self.syscalls.iter().filter(|e| e.ssn_x86.is_some()).count();
    }
}

/// Writes a snapshot atomically to `path`. Creates parent directories.
pub fn write_atomic(path: &Path, snapshot: &AutoSnapshot) -> Result<(), CollectError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| {
            CollectError::Network(format!("create output dir {}: {e}", parent.display()))
        })?;
    }
    let content = toml::to_string_pretty(snapshot)
        .map_err(|e| CollectError::Network(format!("serialize TOML: {e}")))?;
    let tmp = path.with_extension("toml.tmp");
    fs::write(&tmp, content)
        .map_err(|e| CollectError::Network(format!("write {}: {e}", tmp.display())))?;
    fs::rename(&tmp, path)
        .map_err(|e| CollectError::Network(format!("rename tmp → dest: {e}")))?;
    info!(path = %path.display(), entries = snapshot.syscalls.len(), "wrote snapshot");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upsert_adds_and_merges() {
        let mut snap = AutoSnapshot::new("11_26100_2314".into(), "11 24H2".into());
        snap.upsert("NtClose", |e| {
            e.ssn_x64 = Some(0xF);
            e.arity_x86 = Some(1);
        });
        snap.upsert("NtClose", |e| {
            e.ssn_x86 = Some(0xF);
        });
        assert_eq!(snap.syscalls.len(), 1);
        let e = &snap.syscalls[0];
        assert_eq!(e.ssn_x64, Some(0xF));
        assert_eq!(e.ssn_x86, Some(0xF));
        assert_eq!(e.arity_x86, Some(1));
    }

    #[test]
    fn serialize_skips_nones() {
        let mut snap = AutoSnapshot::new("11_26100_2314".into(), "11 24H2".into());
        snap.upsert("NtClose", |e| {
            e.ssn_x64 = Some(0xF);
            e.arity_x64 = None;
        });
        let s = toml::to_string_pretty(&snap).unwrap();
        assert!(s.contains("ssn_x64"));
        assert!(!s.contains("arity_x64"));
    }
}
