//! `rsc-types` CLI — runs the phnt parser and emits `db/phnt/phnt.toml`.

use std::path::PathBuf;
use std::process::Command;

use anyhow::{Context, Result};
use clap::Parser;
use tracing::{info, warn};
use tracing_subscriber::EnvFilter;

use rsc_types::emit::{snapshot_from_signatures, write_atomic};
use rsc_types::parser::parse_directory;

#[derive(Parser, Debug)]
#[command(
    name = "rsc-types",
    version,
    about = "Parse vendored phnt headers → db/phnt/phnt.toml"
)]
struct Cli {
    /// Directory containing phnt `.h` files (the vendored submodule).
    #[arg(long, default_value = "vendor/phnt")]
    phnt_dir: PathBuf,

    /// Output path for the TOML snapshot.
    #[arg(long, default_value = "db/phnt/phnt.toml")]
    output: PathBuf,

    /// Increase log verbosity (-v, -vv, -vvv).
    #[arg(short, long, action = clap::ArgAction::Count)]
    verbose: u8,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    setup_logging(cli.verbose);

    info!(phnt_dir = %cli.phnt_dir.display(), "parsing phnt headers");
    let signatures =
        parse_directory(&cli.phnt_dir).with_context(|| "parse phnt directory failed")?;
    info!(total = signatures.len(), "signatures extracted");

    let commit = phnt_commit(&cli.phnt_dir);
    if commit.is_some() {
        info!(commit = commit.as_deref().unwrap_or(""), "phnt commit");
    } else {
        warn!("phnt submodule commit could not be determined");
    }

    let snapshot = snapshot_from_signatures(&signatures, commit);
    info!(
        unique_functions = snapshot.functions.len(),
        output = %cli.output.display(),
        "writing snapshot"
    );
    write_atomic(&cli.output, &snapshot).context("writing phnt snapshot failed")?;

    Ok(())
}

fn phnt_commit(dir: &std::path::Path) -> Option<String> {
    let output = Command::new("git")
        .args(["-C", dir.to_str()?, "rev-parse", "HEAD"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&output.stdout).trim().to_string();
    (!s.is_empty()).then_some(s)
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
