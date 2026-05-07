//! `mos-lsp` — language-server stdio entry point (manifest §17).
//!
//! Editors spawn this binary and speak LSP over stdin/stdout. The
//! protocol implementation lives in `mosaic_lsp::run` so it can be
//! exercised from tests without owning the process.

#![allow(clippy::print_stderr)]

use std::process::ExitCode;

fn main() -> ExitCode {
    match mosaic_lsp::run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("mos-lsp: {err}");
            ExitCode::FAILURE
        }
    }
}
