//! `mos` — command-line interface for the Mosaic typesetting engine.
//!
//! Subcommands mirror manifest §15.1. Every command currently prints
//! a "not yet implemented" message to stderr and exits non-zero so
//! that scripts and CI can detect the placeholder; real work is
//! sequenced through MVP 0–6.

#![allow(clippy::print_stderr)]

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(
    name = "mos",
    bin_name = "mos",
    version,
    about = "Mosaic — semantic, incremental typesetting compiler",
    long_about = "Mosaic compiles `.mos` source files to PDF, HTML, and EPUB.\n\
                  See manifest.md in the repository root for the full design."
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Initialise a new Mosaic project in the current directory.
    Init {
        #[arg(default_value = ".")]
        path: PathBuf,
    },

    /// Build the project to its declared outputs.
    Build {
        #[arg(default_value = "main.mos")]
        entry: PathBuf,
        /// Refuse to update dependencies (manifest §15.3).
        #[arg(long)]
        frozen: bool,
        /// Make the build deterministic (manifest §24).
        #[arg(long)]
        reproducible: bool,
    },

    /// Watch sources and rebuild on change (manifest §8).
    Watch {
        #[arg(default_value = "main.mos")]
        entry: PathBuf,
    },

    /// Type-check and validate without producing output.
    Check {
        #[arg(default_value = "main.mos")]
        entry: PathBuf,
    },

    /// Format `.mos` sources (manifest §18).
    Fmt {
        #[arg(default_value = ".")]
        path: PathBuf,
    },

    /// Run document and package tests (manifest §28).
    Test,

    /// Profile a build and report layout hot spots (manifest §16).
    Profile {
        #[arg(default_value = "main.mos")]
        entry: PathBuf,
    },

    /// Remove build artefacts and the local cache.
    Clean,

    /// Bundle a project into a `.mosaicbundle` archive (manifest §15.3).
    Package {
        #[arg(default_value = "main.mos")]
        entry: PathBuf,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();

    let cmd = match &cli.command {
        Command::Init { .. } => "init",
        Command::Build { .. } => "build",
        Command::Watch { .. } => "watch",
        Command::Check { .. } => "check",
        Command::Fmt { .. } => "fmt",
        Command::Test => "test",
        Command::Profile { .. } => "profile",
        Command::Clean => "clean",
        Command::Package { .. } => "package",
    };

    eprintln!("mos {cmd}: not yet implemented (see manifest §30 MVP roadmap)");
    ExitCode::FAILURE
}
