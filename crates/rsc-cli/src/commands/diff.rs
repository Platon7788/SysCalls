//! `rsc diff <build-a> <build-b>` — show the NT API differences between
//! two auto-collected snapshots.

use std::collections::BTreeMap;
use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::Args;

use crate::db::{load_auto, AutoSyscall};

#[derive(Args, Debug)]
pub struct DiffArgs {
    /// Source build id (e.g. `10_19045_6466`) or explicit path.
    pub from: String,
    /// Target build id or path.
    pub to: String,

    #[arg(long, default_value = "db/auto")]
    pub auto_dir: PathBuf,
}

pub fn run(args: DiffArgs) -> Result<()> {
    let a_path = resolve(&args.auto_dir, &args.from);
    let b_path = resolve(&args.auto_dir, &args.to);

    let a = load_auto(&a_path).with_context(|| format!("load {}", a_path.display()))?;
    let b = load_auto(&b_path).with_context(|| format!("load {}", b_path.display()))?;

    let a_idx: BTreeMap<&str, &AutoSyscall> =
        a.syscalls.iter().map(|e| (e.name.as_str(), e)).collect();
    let b_idx: BTreeMap<&str, &AutoSyscall> =
        b.syscalls.iter().map(|e| (e.name.as_str(), e)).collect();

    let mut added = Vec::new();
    let mut removed = Vec::new();
    let mut changed = Vec::new();

    for (name, b_entry) in &b_idx {
        match a_idx.get(name) {
            None => added.push(*name),
            Some(a_entry) => {
                if a_entry.ssn_x64 != b_entry.ssn_x64
                    || a_entry.ssn_x86 != b_entry.ssn_x86
                    || a_entry.arity_x86 != b_entry.arity_x86
                {
                    changed.push((*name, *a_entry, *b_entry));
                }
            }
        }
    }
    for name in a_idx.keys() {
        if !b_idx.contains_key(name) {
            removed.push(*name);
        }
    }

    println!(
        "# Diff {} → {}\n\ntotal: {} → {}\nadded: {}\nremoved: {}\nchanged: {}\n",
        a.meta.build_id,
        b.meta.build_id,
        a_idx.len(),
        b_idx.len(),
        added.len(),
        removed.len(),
        changed.len(),
    );

    if !added.is_empty() {
        println!("## Added ({})", added.len());
        for n in &added {
            println!("  + {n}");
        }
    }
    if !removed.is_empty() {
        println!("\n## Removed ({})", removed.len());
        for n in &removed {
            println!("  - {n}");
        }
    }
    if !changed.is_empty() {
        println!("\n## Changed ({})", changed.len());
        for (n, a, b) in &changed {
            println!(
                "  ~ {n}: ssn_x64 {:?}→{:?}, ssn_x86 {:?}→{:?}, arity_x86 {:?}→{:?}",
                a.ssn_x64, b.ssn_x64, a.ssn_x86, b.ssn_x86, a.arity_x86, b.arity_x86
            );
        }
    }

    Ok(())
}

fn resolve(dir: &std::path::Path, arg: &str) -> PathBuf {
    let p = PathBuf::from(arg);
    if p.is_file() {
        return p;
    }
    dir.join(format!("{arg}.toml"))
}
