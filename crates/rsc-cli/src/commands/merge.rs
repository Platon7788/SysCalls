//! `rsc merge` — combines `db/auto/*.toml`, `db/phnt/phnt.toml` and
//! `db/overrides.toml` into `db/canonical.toml`.
//!
//! Priority order per `Docs/DATABASE.md` §4:
//! - SSN / arity / RVA / stubs: auto (overrides `fix_*` may supersede)
//! - Return type / params: overrides (`fix_signature` / `add`) → phnt → opaque
//! - `rsc_hash`: always computed fresh from the name

use std::collections::BTreeMap;
use std::path::PathBuf;

use anyhow::{anyhow, Context, Result};
use clap::Args;
use tracing::{info, warn};

use crate::db::{
    auto_files_in, infer_category, load_auto, load_overrides, load_phnt,
    write_canonical_atomic, AutoSyscall, CanonMeta, CanonParam, CanonSyscall, CanonicalDb,
    Override, PhntFunction,
};

#[derive(Args, Debug)]
pub struct MergeArgs {
    /// Explicit auto snapshot path. When set, only this single snapshot is
    /// read and `--auto-dir` is ignored. Without this flag, **all**
    /// `*.toml` under `--auto-dir` are unioned (compile-time API covers
    /// every Windows build seen).
    #[arg(long)]
    pub auto: Option<PathBuf>,

    /// Source directory for auto snapshots (used when `--auto` is absent).
    #[arg(long, default_value = "db/auto")]
    pub auto_dir: PathBuf,

    /// Which build should supply SSN / RVA / stub bytes when a function
    /// appears in multiple snapshots. Defaults to the lexicographically
    /// latest build-id (typically the newest Windows).
    #[arg(long)]
    pub baseline_build: Option<String>,

    /// Path to the phnt snapshot.
    #[arg(long, default_value = "db/phnt/phnt.toml")]
    pub phnt: PathBuf,

    /// Path to the user-edited overrides layer.
    #[arg(long, default_value = "db/overrides.toml")]
    pub overrides: PathBuf,

    /// Output path.
    #[arg(long, default_value = "db/canonical.toml")]
    pub out: PathBuf,
}

pub fn run(args: MergeArgs) -> Result<()> {
    // 0) Load one or all auto snapshots.
    let auto_paths: Vec<PathBuf> = match args.auto.clone() {
        Some(p) => vec![p],
        None => auto_files_in(&args.auto_dir).context("list auto dir")?,
    };
    if auto_paths.is_empty() {
        anyhow::bail!(
            "no auto snapshots found — run `rsc-collector` first (or pass --auto explicitly)"
        );
    }

    let mut builds: Vec<(String, crate::db::AutoLayer)> = Vec::new();
    for p in &auto_paths {
        info!(auto = %p.display(), "loading auto layer");
        let layer = load_auto(p).with_context(|| format!("load {}", p.display()))?;
        let build_id = layer.meta.build_id.clone();
        builds.push((build_id, layer));
    }

    // Choose baseline: explicit flag wins; otherwise pick the
    // lexicographically largest build-id (typically = newest Windows).
    let baseline_build = match args.baseline_build.clone() {
        Some(b) => b,
        None => builds
            .iter()
            .map(|(id, _)| id.clone())
            .max()
            .unwrap_or_default(),
    };
    if !builds.iter().any(|(id, _)| id == &baseline_build) {
        anyhow::bail!(
            "baseline build {baseline_build:?} not found among loaded snapshots — \
             available: {:?}",
            builds.iter().map(|(id, _)| id.as_str()).collect::<Vec<_>>()
        );
    }
    info!(%baseline_build, "baseline chosen for SSN/RVA/stub fields");

    info!(phnt = %args.phnt.display(), "loading phnt layer");
    let phnt = load_phnt(&args.phnt).context("load phnt layer")?;

    info!(overrides = %args.overrides.display(), "loading overrides");
    let overrides = load_overrides(&args.overrides).context("load overrides layer")?;

    let phnt_index: BTreeMap<&str, &PhntFunction> =
        phnt.functions.iter().map(|f| (f.name.as_str(), f)).collect();
    let overrides_index: BTreeMap<&str, &Override> =
        overrides.overrides.iter().map(|o| (o.name.as_str(), o)).collect();

    // 1) Union by name across every auto snapshot.
    //    Value per name holds: all build-ids that saw it, and the auto
    //    record from the baseline build (if it saw it).
    struct Union<'a> {
        seen_on: Vec<String>,
        baseline_record: Option<&'a AutoSyscall>,
        /// Fallback record from the lex-largest non-baseline build that
        /// has the function — only used when baseline doesn't.
        fallback_record: Option<(&'a str, &'a AutoSyscall)>,
    }

    let mut union_index: BTreeMap<String, Union<'_>> = BTreeMap::new();
    for (build_id, layer) in &builds {
        for sys in &layer.syscalls {
            let entry = union_index
                .entry(sys.name.clone())
                .or_insert_with(|| Union {
                    seen_on: Vec::new(),
                    baseline_record: None,
                    fallback_record: None,
                });
            entry.seen_on.push(build_id.clone());
            if build_id == &baseline_build {
                entry.baseline_record = Some(sys);
            } else {
                // Keep the lexicographically largest non-baseline build as
                // a fallback — newest build's SSN is likely more current.
                match entry.fallback_record {
                    Some((prev, _)) if build_id.as_str() <= prev => {}
                    _ => entry.fallback_record = Some((build_id.as_str(), sys)),
                }
            }
        }
    }

    let mut merged: Vec<CanonSyscall> = Vec::new();
    let mut hashes: BTreeMap<u32, String> = BTreeMap::new();
    let mut stats = MergeStats::default();

    for (name, union) in &union_index {
        let o = overrides_index.get(name.as_str()).copied();
        if is_excluded(o) {
            stats.excluded += 1;
            continue;
        }
        let p = phnt_index.get(name.as_str()).copied();

        // Pick the auto record for SSN/RVA/stub: baseline first, fall back
        // to any other build that saw the function so the canonical still
        // has sensible values for cross-version-only functions.
        let auto_record = union
            .baseline_record
            .or_else(|| union.fallback_record.map(|(_, s)| s))
            .expect("union has at least one build by construction");
        let mut entry = build_entry(auto_record, p, o);
        entry.available_on = {
            let mut v = union.seen_on.clone();
            v.sort();
            v.dedup();
            v
        };

        check_collision(&mut hashes, &entry.name, entry.rsc_hash)
            .map_err(|e| anyhow!("{e}"))?;
        if p.is_none() {
            stats.opaque += 1;
        }
        if o.is_some() {
            stats.overridden += 1;
        }
        merged.push(entry);
    }

    // 2) overrides with kind="add" for functions not seen in any build.
    for o in &overrides.overrides {
        if o.kind != "add" {
            continue;
        }
        if merged.iter().any(|e| e.name == o.name) {
            warn!(name = %o.name, "`add` override duplicates existing function; ignored");
            continue;
        }
        let mut entry = build_from_add(o);
        entry.available_on = vec!["overrides/add".to_string()];
        check_collision(&mut hashes, &entry.name, entry.rsc_hash).map_err(|e| anyhow!("{e}"))?;
        merged.push(entry);
        stats.added_by_override += 1;
    }

    // 3) phnt-only count (warning, not fatal).
    for name in phnt_index.keys() {
        if !merged.iter().any(|e| e.name.as_str() == *name) {
            stats.phnt_only += 1;
        }
    }

    merged.sort_by(|a, b| a.name.cmp(&b.name));

    let db = CanonicalDb {
        meta: CanonMeta {
            schema_version: 1,
            generated_at: time::OffsetDateTime::now_utc()
                .format(&time::format_description::well_known::Rfc3339)
                .unwrap_or_else(|_| "unknown".into()),
            merge_tool_version: env!("CARGO_PKG_VERSION").to_string(),
            baseline_build,
            phnt_commit: phnt.meta.phnt_commit.clone(),
            layers_merged: auto_paths
                .iter()
                .map(|p| format!("auto = {}", p.display()))
                .chain([
                    format!("phnt = {}", args.phnt.display()),
                    format!("overrides = {}", args.overrides.display()),
                ])
                .collect(),
        },
        syscalls: merged,
    };

    write_canonical_atomic(&args.out, &db).context("write canonical")?;
    info!(
        path = %args.out.display(),
        total = db.syscalls.len(),
        builds_unioned = builds.len(),
        phnt_matched = db.syscalls.len() as i64 - stats.opaque as i64,
        opaque = stats.opaque,
        excluded = stats.excluded,
        overridden = stats.overridden,
        added = stats.added_by_override,
        phnt_only_warnings = stats.phnt_only,
        "canonical written"
    );
    Ok(())
}

#[derive(Default)]
struct MergeStats {
    opaque: usize,
    excluded: usize,
    overridden: usize,
    added_by_override: usize,
    phnt_only: usize,
}

fn is_excluded(o: Option<&Override>) -> bool {
    o.is_some_and(|o| o.kind == "exclude")
}

fn check_collision(
    hashes: &mut BTreeMap<u32, String>,
    name: &str,
    hash: u32,
) -> core::result::Result<(), String> {
    match hashes.get(&hash) {
        Some(existing) if existing != name => Err(format!(
            "hash {hash:#010x} collision between {existing:?} and {name:?}"
        )),
        _ => {
            hashes.insert(hash, name.to_string());
            Ok(())
        }
    }
}

fn build_entry(a: &AutoSyscall, p: Option<&PhntFunction>, o: Option<&Override>) -> CanonSyscall {
    let (return_type, mut params, opaque) = resolve_signature(&a.name, p, o);
    let mut sources = vec!["auto".to_string()];
    if p.is_some() {
        sources.push("phnt".into());
    }
    if o.is_some() {
        sources.push("overrides".into());
    }

    let ssn_x64 = o.and_then(|o| o.ssn_x64).or(a.ssn_x64);
    let ssn_x86 = o.and_then(|o| o.ssn_x86).or(a.ssn_x86);
    let arity_x64 = o.and_then(|o| o.arity_x64).or(a.arity_x64);
    let arity_x86 = o.and_then(|o| o.arity_x86).or(a.arity_x86);

    // For opaque functions, synthesize word-sized `*mut c_void` parameters
    // from the arity we got from stub disassembly — otherwise the generated
    // stub would call the syscall with the wrong number of stack / register
    // arguments and corrupt caller state.
    if opaque {
        let arity = arity_x86.or(arity_x64).unwrap_or(0);
        params = (0..arity)
            .map(|i| CanonParam {
                name: format!("arg{}", i + 1),
                r#type: "*mut c_void".into(),
                direction: "inout".into(),
                optional: false,
            })
            .collect();
    }

    CanonSyscall {
        rsc_hash: rsc_runtime::rsc_hash(a.name.as_bytes()),
        name: a.name.clone(),
        sources,
        available_on: Vec::new(),    // populated by the caller after build_entry
        ssn_x64,
        ssn_x86,
        arity_x64,
        arity_x86,
        rva_x64: a.rva_x64,
        rva_x86: a.rva_x86,
        return_type,
        params,
        category: infer_category(&a.name).to_string(),
        excluded: false,
        opaque_signature: opaque,
        min_phnt_version: p.and_then(|p| p.min_phnt_version.clone()),
    }
}

fn build_from_add(o: &Override) -> CanonSyscall {
    let params: Vec<CanonParam> = o
        .params
        .iter()
        .map(|p| CanonParam {
            name: p.name.clone(),
            r#type: p.r#type.clone(),
            direction: p.direction.clone(),
            optional: p.optional,
        })
        .collect();
    CanonSyscall {
        rsc_hash: rsc_runtime::rsc_hash(o.name.as_bytes()),
        name: o.name.clone(),
        sources: vec!["overrides(add)".into()],
        available_on: Vec::new(),   // populated by the caller
        ssn_x64: o.ssn_x64,
        ssn_x86: o.ssn_x86,
        arity_x64: o.arity_x64,
        arity_x86: o.arity_x86,
        rva_x64: None,
        rva_x86: None,
        return_type: o.return_type.clone().unwrap_or_else(|| "NTSTATUS".into()),
        params,
        category: infer_category(&o.name).to_string(),
        excluded: false,
        opaque_signature: false,
        min_phnt_version: None,
    }
}

fn resolve_signature(
    _name: &str,
    p: Option<&PhntFunction>,
    o: Option<&Override>,
) -> (String, Vec<CanonParam>, bool) {
    if let Some(o) = o {
        if o.kind == "fix_signature" {
            let return_type = o
                .return_type
                .clone()
                .unwrap_or_else(|| "NTSTATUS".to_string());
            let params = o
                .params
                .iter()
                .map(|p| CanonParam {
                    name: p.name.clone(),
                    r#type: p.r#type.clone(),
                    direction: p.direction.clone(),
                    optional: p.optional,
                })
                .collect();
            return (return_type, params, false);
        }
    }
    if let Some(p) = p {
        let params = p
            .params
            .iter()
            .map(|p| CanonParam {
                name: sanitize_ident(&p.name),
                r#type: p.r#type.clone(),
                direction: p.direction.clone(),
                optional: p.optional,
            })
            .collect();
        return (p.return_type.clone(), params, false);
    }

    // Opaque fallback: no phnt info. We emit zero params here — the
    // `build_entry` caller stitches in a correct arity based on the stub's
    // `arity_x86` disassembly (x86 stubs encode `ret N`, giving us a
    // reliable count). Caller wraps the params in `*mut c_void`.
    ("NTSTATUS".into(), Vec::new(), /* opaque = */ true)
}

/// Keep the phnt name if it's a valid Rust identifier, else fall back to
/// `arg_<n>`. Phnt is ASCII and tends to follow Rust conventions, but the
/// parser may occasionally surface something with digits-first or a
/// reserved keyword.
fn sanitize_ident(raw: &str) -> String {
    // First-char check without `.unwrap()`: handles empty + non-ASCII cases
    // uniformly. An ASCII letter or `_` is required to start a Rust ident.
    let Some(first) = raw.chars().next() else {
        return "arg".to_string();
    };
    if !first.is_ascii_alphabetic() && first != '_' {
        return format!("_arg_{raw}");
    }
    // Rust reserved keywords we may actually hit.
    matches!(
        raw,
        "as" | "async" | "await" | "break" | "const" | "continue" | "crate"
        | "dyn" | "else" | "enum" | "extern" | "fn" | "for" | "if" | "impl"
        | "in" | "let" | "loop" | "match" | "mod" | "move" | "mut" | "pub"
        | "ref" | "return" | "self" | "Self" | "static" | "struct" | "super"
        | "trait" | "true" | "false" | "type" | "unsafe" | "use" | "where"
        | "while" | "yield" | "box"
    )
    .then(|| format!("r#{raw}"))
    .unwrap_or_else(|| raw.to_string())
}
