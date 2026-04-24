//! # `rsc` — unified CLI for SysCalls.
//!
//! ```text
//! rsc merge                      # auto + phnt + overrides → canonical
//! rsc verify [--strict]          # sanity checks
//! rsc diff <a> <b>               # changes between two builds
//! rsc stats                      # dashboard on canonical
//! ```
//!
//! `rsc collect` and `rsc phnt-parse` are *not* re-implemented here —
//! they're separate binaries (`rsc-collector`, `rsc-types`) each
//! standalone; invoking them via this CLI would duplicate flag surface.
//! `scripts/refresh.bat` chains everything end-to-end.

mod commands;
mod db;

use anyhow::Result;
use clap::{Parser, Subcommand};
use tracing_subscriber::EnvFilter;

#[derive(Parser, Debug)]
#[command(name = "rsc", version, about = "SysCalls unified CLI")]
struct Cli {
    #[arg(short, long, action = clap::ArgAction::Count, global = true)]
    verbose: u8,

    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand, Debug)]
enum Cmd {
    /// Merge auto + phnt + overrides → canonical.toml.
    Merge(commands::merge::MergeArgs),
    /// Integrity / collision checks on canonical.toml.
    Verify(commands::verify::VerifyArgs),
    /// Show added / removed / changed syscalls between two builds.
    Diff(commands::diff::DiffArgs),
    /// Print a summary dashboard on canonical.toml.
    Stats(commands::stats::StatsArgs),
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    setup_logging(cli.verbose);

    match cli.cmd {
        Cmd::Merge(a) => commands::merge::run(a),
        Cmd::Verify(a) => commands::verify::run(a),
        Cmd::Diff(a) => commands::diff::run(a),
        Cmd::Stats(a) => commands::stats::run(a),
    }
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
