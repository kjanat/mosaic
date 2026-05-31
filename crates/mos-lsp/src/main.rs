//! `mos-lsp` — language-server stdio entry point (manifest §17).
//!
//! Editors spawn this binary and speak LSP over stdin/stdout. The
//! protocol implementation lives in `mos_lsp::run` so it can be
//! exercised from tests without owning the process.

#![allow(
    clippy::print_stderr,
    reason = "binary entry point reports fatal protocol startup errors to stderr"
)]

use std::process::ExitCode;

fn main() -> ExitCode {
    match mos_lsp::run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("mos-lsp: {err}");
            ExitCode::FAILURE
        }
    }
}
