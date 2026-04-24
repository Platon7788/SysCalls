//! `rsc stats` — quick dashboard on the canonical DB.

use std::collections::BTreeMap;
use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::Args;

use crate::db::load_canonical;

#[derive(Args, Debug)]
pub struct StatsArgs {
    #[arg(long, default_value = "db/canonical.toml")]
    pub canonical: PathBuf,
}

pub fn run(args: StatsArgs) -> Result<()> {
    let db = load_canonical(&args.canonical).context("load canonical")?;

    let total = db.syscalls.len();
    let phnt_typed = db.syscalls.iter().filter(|e| !e.opaque_signature).count();
    let opaque = total - phnt_typed;
    let excluded = db.syscalls.iter().filter(|e| e.excluded).count();
    let with_x64 = db.syscalls.iter().filter(|e| e.ssn_x64.is_some()).count();
    let with_x86 = db.syscalls.iter().filter(|e| e.ssn_x86.is_some()).count();

    let mut by_category: BTreeMap<&str, usize> = BTreeMap::new();
    for e in &db.syscalls {
        *by_category.entry(e.category.as_str()).or_default() += 1;
    }

    let mut by_source: BTreeMap<&str, usize> = BTreeMap::new();
    for e in &db.syscalls {
        for src in &e.sources {
            *by_source.entry(src.as_str()).or_default() += 1;
        }
    }

    println!("# canonical.toml @ {}\n", args.canonical.display());
    println!("baseline_build:  {}", db.meta.baseline_build);
    println!(
        "phnt_commit:     {}",
        db.meta.phnt_commit.as_deref().unwrap_or("(none)")
    );
    println!("generated_at:    {}", db.meta.generated_at);
    println!();
    println!("total:           {total}");
    println!(
        "with phnt types: {phnt_typed} ({:.1}%)",
        100.0 * phnt_typed as f64 / total.max(1) as f64
    );
    println!("opaque:          {opaque}");
    println!("excluded:        {excluded}");
    println!("with SSN x64:    {with_x64}");
    println!("with SSN x86:    {with_x86}");
    println!();

    println!("## By category");
    for (cat, n) in &by_category {
        println!("  {cat:<10} {n}");
    }

    println!("\n## By source layer");
    for (src, n) in &by_source {
        println!("  {src:<18} {n}");
    }

    // Per-build coverage across the unioned auto snapshots.
    let mut builds: BTreeMap<&str, usize> = BTreeMap::new();
    let mut exclusive: BTreeMap<&str, usize> = BTreeMap::new();
    for e in &db.syscalls {
        for b in &e.available_on {
            *builds.entry(b.as_str()).or_default() += 1;
        }
        if e.available_on.len() == 1 {
            *exclusive.entry(e.available_on[0].as_str()).or_default() += 1;
        }
    }
    if !builds.is_empty() {
        println!("\n## By Windows build (union across db/auto/*.toml)");
        for (build_id, n) in &builds {
            let excl = exclusive.get(build_id).copied().unwrap_or(0);
            println!("  {build_id:<22} {n:4}  ({excl} exclusive)");
        }
    }

    Ok(())
}
