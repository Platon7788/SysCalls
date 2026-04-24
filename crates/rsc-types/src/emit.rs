//! TOML emitter for the phnt overlay (see `Docs/DATABASE.md` §2).

use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

use crate::normalizer::normalize;
use crate::parser::{Direction, PhntSignature};

pub const SCHEMA_VERSION: u32 = 1;
pub const TYPES_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Serialize, Deserialize, Debug, Default)]
pub struct PhntSnapshot {
    pub meta: Meta,
    #[serde(rename = "function", default)]
    pub functions: Vec<PhntFunctionEntry>,
}

#[derive(Serialize, Deserialize, Debug, Default)]
pub struct Meta {
    pub schema_version: u32,
    pub phnt_commit: Option<String>,
    pub parsed_at: String,
    pub types_version: String,
    pub source_count: usize,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PhntFunctionEntry {
    pub name: String,
    pub return_type: String,
    pub params: Vec<PhntFunctionParam>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_phnt_version: Option<String>,
    /// Header file the declaration came from.
    pub source: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PhntFunctionParam {
    pub name: String,
    pub r#type: String,
    pub direction: String,
    #[serde(default, skip_serializing_if = "core::ops::Not::not")]
    pub optional: bool,
}

pub fn snapshot_from_signatures(
    signatures: &[PhntSignature],
    phnt_commit: Option<String>,
) -> PhntSnapshot {
    let mut functions: Vec<PhntFunctionEntry> = signatures
        .iter()
        .map(|sig| PhntFunctionEntry {
            name: nt_name_for_storage(&sig.name),
            return_type: normalize(&sig.return_type),
            params: sig
                .params
                .iter()
                .map(|p| PhntFunctionParam {
                    name: p.name.clone(),
                    r#type: normalize(&p.r#type),
                    direction: direction_label(p.direction),
                    optional: p.optional,
                })
                .collect(),
            min_phnt_version: sig.min_phnt_version.clone(),
            source: sig.source.clone(),
        })
        .collect();

    // De-duplicate on name (phnt sometimes re-declares Zw* aliases) — first
    // entry wins. Sort alphabetically for stable output.
    functions.sort_by(|a, b| a.name.cmp(&b.name));
    functions.dedup_by(|a, b| a.name == b.name);

    let parsed_at = OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_else(|_| "unknown".into());

    PhntSnapshot {
        meta: Meta {
            schema_version: SCHEMA_VERSION,
            phnt_commit,
            parsed_at,
            types_version: TYPES_VERSION.into(),
            source_count: signatures.len(),
        },
        functions,
    }
}

/// `Zw*` and `Nt*` are aliases — store everything under the `Nt*` spelling
/// so lookups in `canonical.toml` match what `rsc_syscall!(NtXxx)` emits.
fn nt_name_for_storage(raw: &str) -> String {
    if let Some(tail) = raw.strip_prefix("Zw") {
        format!("Nt{tail}")
    } else {
        raw.to_string()
    }
}

fn direction_label(d: Direction) -> String {
    match d {
        Direction::In => "in".into(),
        Direction::Out => "out".into(),
        Direction::Inout => "inout".into(),
        Direction::Unknown => "inout".into(),
    }
}

pub fn write_atomic(path: &Path, snap: &PhntSnapshot) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    }
    let content = toml::to_string_pretty(snap).context("serialize phnt snapshot to TOML")?;
    let tmp = path.with_extension("toml.tmp");
    fs::write(&tmp, content).with_context(|| format!("write {}", tmp.display()))?;
    fs::rename(&tmp, path).context("rename tmp → dest")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zw_name_is_stored_as_nt() {
        let sig = PhntSignature {
            name: "ZwClose".to_string(),
            return_type: "NTSTATUS".to_string(),
            params: vec![],
            min_phnt_version: None,
            source: "test.h".to_string(),
        };
        let snap = snapshot_from_signatures(&[sig], None);
        assert_eq!(snap.functions[0].name, "NtClose");
    }

    #[test]
    fn dedup_keeps_one_per_name() {
        let sig_a = PhntSignature {
            name: "NtClose".into(),
            return_type: "NTSTATUS".into(),
            params: vec![],
            min_phnt_version: None,
            source: "a.h".into(),
        };
        let sig_b = PhntSignature {
            name: "NtClose".into(),
            return_type: "NTSTATUS".into(),
            params: vec![],
            min_phnt_version: None,
            source: "b.h".into(),
        };
        let snap = snapshot_from_signatures(&[sig_a, sig_b], None);
        assert_eq!(snap.functions.len(), 1);
    }
}
