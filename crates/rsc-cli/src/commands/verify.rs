//! `rsc verify` — sanity checks on the canonical DB.

use std::collections::BTreeMap;
use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::Args;
use tracing::{error, info, warn};

use crate::db::{load_canonical, CanonicalDb};

#[derive(Args, Debug)]
pub struct VerifyArgs {
    #[arg(long, default_value = "db/canonical.toml")]
    pub canonical: PathBuf,

    /// Exit with code 1 on any warning (not just errors).
    #[arg(long)]
    pub strict: bool,
}

pub fn run(args: VerifyArgs) -> Result<()> {
    let db = load_canonical(&args.canonical).context("load canonical")?;
    let (errors, warnings) = check(&db);

    for w in &warnings {
        warn!("{w}");
    }
    for e in &errors {
        error!("{e}");
    }
    info!(
        total = db.syscalls.len(),
        errors = errors.len(),
        warnings = warnings.len(),
        "verify complete"
    );

    if !errors.is_empty() || (args.strict && !warnings.is_empty()) {
        anyhow::bail!("verify failed");
    }
    Ok(())
}

pub fn check(db: &CanonicalDb) -> (Vec<String>, Vec<String>) {
    let mut errors = Vec::new();
    let mut warnings = Vec::new();

    // Duplicate names
    let mut name_count: BTreeMap<&str, usize> = BTreeMap::new();
    for e in &db.syscalls {
        *name_count.entry(e.name.as_str()).or_default() += 1;
    }
    for (name, n) in &name_count {
        if *n > 1 {
            errors.push(format!("duplicate name {name:?} appears {n} times"));
        }
    }

    // Hash collisions
    let mut hash_owner: BTreeMap<u32, &str> = BTreeMap::new();
    for e in &db.syscalls {
        if let Some(prev) = hash_owner.get(&e.rsc_hash) {
            if *prev != e.name.as_str() {
                errors.push(format!(
                    "hash collision {:#010x} between {:?} and {:?}",
                    e.rsc_hash, prev, e.name
                ));
            }
        } else {
            hash_owner.insert(e.rsc_hash, e.name.as_str());
        }
    }

    // SSN range (NT table: 0x000..0x0FFF)
    for e in &db.syscalls {
        for (label, v) in [("ssn_x64", e.ssn_x64), ("ssn_x86", e.ssn_x86)] {
            let Some(v) = v else { continue };
            // WoW64 raw SSN has high bits (service-table index) set; mask
            // low 16 for the range check.
            let idx = v & 0xFFFF;
            if idx >= 0x1000 {
                warnings.push(format!(
                    "{}: {label} index {idx:#x} outside NT table range (0x000..0x0FFF)",
                    e.name
                ));
            }
        }
    }

    // Opaque signatures — informational, not fatal
    let opaque = db.syscalls.iter().filter(|e| e.opaque_signature).count();
    if opaque > 0 {
        warnings.push(format!("{opaque} functions fell back to opaque signatures"));
    }

    // Excluded count
    let excluded = db.syscalls.iter().filter(|e| e.excluded).count();
    if excluded > 0 {
        warnings.push(format!("{excluded} functions explicitly excluded"));
    }

    // Hash must never be zero (would collide with "name not found")
    for e in &db.syscalls {
        if e.rsc_hash == 0 {
            errors.push(format!("{}: rsc_hash is zero — would shadow 'not found'", e.name));
        }
    }

    (errors, warnings)
}
