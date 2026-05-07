//! Parser for the Mosaic source language (`.mos`).
//!
//! See manifest §3 (language design) and §6 stages 1–2 (parse + lower).
//! This crate produces a concrete syntax tree that preserves spans,
//! comments, and recoverable errors.

use std::path::Path;

use mosaic_core::{Diagnostic, DiagnosticCode};

/// Concrete syntax tree. The real type will be a typed tree;
/// for the scaffold it is just an opaque marker.
#[derive(Default, Debug)]
pub struct SyntaxTree {
    _private: (),
}

/// Parse a Mosaic source string.
///
/// Returns the syntax tree on success or a non-empty list of diagnostics
/// on failure. Diagnostics are recoverable per manifest §6 stage 1.
pub fn parse(_src: &str, _file: &Path) -> Result<SyntaxTree, Vec<Diagnostic>> {
    Err(vec![Diagnostic::error(
        DiagnosticCode("E000"),
        "mosaic-parse::parse is not yet implemented (MVP 0)",
    )])
}
