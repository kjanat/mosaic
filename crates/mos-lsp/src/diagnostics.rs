//! Translation from compiler [`mos_core::Diagnostic`]s to the
//! LSP `Diagnostic` shape that the server publishes over
//! `textDocument/publishDiagnostics`.
//!
//! The compiler owns the message text and stable diagnostic codes; this
//! module only re-shapes spans into LSP positions and tags every
//! diagnostic with `source = "mosaic"` so editors can distinguish them
//! from other servers.

use std::path::{Path, PathBuf};

use mos_core::{Diagnostic as CoreDiagnostic, Severity, SourceSpan};
use serde::{Deserialize, Serialize};

/// LSP `Position`. Lines and characters are zero-based; `character`
/// counts UTF-16 code units, matching the LSP default position
/// encoding so we don't have to negotiate per-client.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct LspPosition {
    pub line: u32,
    pub character: u32,
}

/// LSP `Range` between two [`LspPosition`]s.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct LspRange {
    pub start: LspPosition,
    pub end: LspPosition,
}

/// LSP `Diagnostic`. Only the fields the compiler currently produces
/// are modelled — adding related-information or tags is a separate
/// slice.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct LspDiagnostic {
    pub range: LspRange,
    /// LSP `DiagnosticSeverity`: 1 Error, 2 Warning, 3 Information, 4 Hint.
    pub severity: u8,
    pub code: String,
    pub source: String,
    pub message: String,
}

/// Convert a byte offset inside `src` into an LSP [`LspPosition`].
///
/// Offsets are clamped to the end of `src` and rounded down to the
/// nearest UTF-8 code-point boundary, so spans that straddle a
/// multibyte sequence still produce a valid position rather than
/// panicking.
#[must_use]
pub fn byte_to_position(src: &str, byte_offset: usize) -> LspPosition {
    let mut clamped = byte_offset.min(src.len());
    while clamped > 0 && !src.is_char_boundary(clamped) {
        clamped -= 1;
    }

    let mut line: u32 = 0;
    let mut line_start: usize = 0;
    for (i, byte) in src.as_bytes().iter().enumerate().take(clamped) {
        if *byte == b'\n' {
            line = line.saturating_add(1);
            line_start = i + 1;
        }
    }

    let character: u32 = src[line_start..clamped]
        .chars()
        .map(|c| u32::try_from(c.len_utf16()).unwrap_or(0))
        .sum();
    LspPosition { line, character }
}

/// Map a [`SourceSpan`] inside `src` to an [`LspRange`].
#[must_use]
pub fn span_to_range(src: &str, span: &SourceSpan) -> LspRange {
    LspRange {
        start: byte_to_position(src, span.start),
        end: byte_to_position(src, span.end),
    }
}

/// Translate the LSP `DiagnosticSeverity` integer for a compiler
/// [`Severity`]. The LSP enum is closed over four values so the
/// mapping is total and infallible.
#[must_use]
const fn lsp_severity(severity: Severity) -> u8 {
    match severity {
        Severity::Error => 1,
        Severity::Warning => 2,
        Severity::Note => 3,
        Severity::Help => 4,
    }
}

/// Parse a `file://` URI into a filesystem path. Percent-escapes are
/// decoded as raw bytes and reassembled into UTF-8 so multibyte
/// sequences like `%C3%A9` (`é`) survive round-tripping. Falls back
/// to treating the URI as a literal path so editors that send bare
/// or non-`file` URIs still match against the same string downstream.
#[must_use]
pub fn path_from_uri(uri: &str) -> PathBuf {
    let Some(rest) = uri.strip_prefix("file://") else {
        return PathBuf::from(uri);
    };
    // Drop the (typically empty) authority segment before the path.
    let path_part = rest.split_once('/').map_or(rest, |(_, p)| p);
    let bytes = path_part.as_bytes();
    let mut decoded: Vec<u8> = Vec::with_capacity(bytes.len() + 1);
    decoded.push(b'/');
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let (Some(hi), Some(lo)) = (
                (bytes[i + 1] as char).to_digit(16),
                (bytes[i + 2] as char).to_digit(16),
            )
        {
            let byte = u8::try_from((hi << 4) | lo).unwrap_or(b'?');
            decoded.push(byte);
            i += 3;
            continue;
        }
        decoded.push(bytes[i]);
        i += 1;
    }
    bytes_to_path(decoded)
}

#[cfg(unix)]
fn bytes_to_path(bytes: Vec<u8>) -> PathBuf {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;
    // Preserve non-UTF-8 byte sequences verbatim; unix paths are
    // arbitrary bytes, not text.
    PathBuf::from(OsString::from_vec(bytes))
}

#[cfg(not(unix))]
fn bytes_to_path(bytes: Vec<u8>) -> PathBuf {
    // Non-unix targets only round-trip UTF-8 paths cleanly. Lossy
    // decode keeps things deterministic for stray bytes.
    PathBuf::from(String::from_utf8_lossy(&bytes).into_owned())
}

/// Lower `src` against `file` and project the resulting compiler
/// diagnostics into the LSP shape, filtering to diagnostics that
/// belong to `file`. Diagnostics without a span are anchored at the
/// start of the document so they remain visible in the editor.
#[must_use]
pub fn diagnostics_for_document(file: &Path, src: &str) -> Vec<LspDiagnostic> {
    let lowered = mos_eval::lower(src, file);
    lowered
        .diagnostics
        .iter()
        .filter_map(|d| project_diagnostic(file, src, d))
        .collect()
}

fn project_diagnostic(
    file: &Path,
    src: &str,
    diag: &CoreDiagnostic,
) -> Option<LspDiagnostic> {
    let range = match &diag.span {
        Some(span) if span.file == file => span_to_range(src, span),
        Some(_) => return None,
        None => LspRange {
            start: LspPosition { line: 0, character: 0 },
            end: LspPosition { line: 0, character: 0 },
        },
    };
    Some(LspDiagnostic {
        range,
        severity: lsp_severity(diag.severity),
        code: diag.code.0.to_owned(),
        source: "mosaic".to_owned(),
        message: diag.message.clone(),
    })
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        reason = "tests panic loudly on setup failure; matches crate-wide test-module convention"
    )]

    use std::path::PathBuf;

    use super::*;

    #[test]
    fn byte_to_position_handles_multibyte_lines() {
        let src = "µx\n字y";
        // Start of file.
        assert_eq!(byte_to_position(src, 0), LspPosition { line: 0, character: 0 });
        // After `µ` — one UTF-16 code unit consumed.
        assert_eq!(byte_to_position(src, 2), LspPosition { line: 0, character: 1 });
        // Start of line 2.
        assert_eq!(byte_to_position(src, 4), LspPosition { line: 1, character: 0 });
        // Past the end clamps to the final boundary.
        assert_eq!(byte_to_position(src, 9999), LspPosition { line: 1, character: 2 });
    }

    #[test]
    fn byte_to_position_rounds_inside_codepoint() {
        // Pointing at the second byte of `µ` rounds down to its start.
        let src = "µ";
        assert_eq!(byte_to_position(src, 1), LspPosition { line: 0, character: 0 });
    }

    #[test]
    fn byte_to_position_counts_surrogate_pairs_as_two_units() {
        // 𝕏 (U+1D54F) is outside the BMP — one Unicode scalar value
        // but two UTF-16 code units. LSP's default position encoding
        // is UTF-16, so the character right after 𝕏 must report
        // column 2.
        let src = "𝕏!";
        let bang = src.find('!').expect("test setup: `!` must exist");
        assert_eq!(
            byte_to_position(src, bang),
            LspPosition { line: 0, character: 2 }
        );
    }

    #[test]
    fn path_from_uri_decodes_percent_escapes() {
        assert_eq!(
            path_from_uri("file:///tmp/with%20space.mos"),
            PathBuf::from("/tmp/with space.mos")
        );
    }

    #[test]
    fn path_from_uri_reassembles_utf8_byte_sequences() {
        // `café.mos` percent-encodes to `caf%C3%A9.mos`; decoding
        // byte-wise and then reinterpreting as UTF-8 must round-trip
        // to the original character rather than two Latin-1 bytes.
        assert_eq!(
            path_from_uri("file:///tmp/caf%C3%A9.mos"),
            PathBuf::from("/tmp/café.mos")
        );
    }

    #[test]
    fn path_from_uri_passes_relative_uris_through_literally() {
        // Bare relative paths (no scheme) come through editors that
        // open files outside a workspace. Treat them as opaque keys
        // so the file matcher can still compare URI ↔ span paths.
        assert_eq!(
            path_from_uri("relative/sub/main.mos"),
            PathBuf::from("relative/sub/main.mos")
        );
    }

    #[test]
    fn path_from_uri_falls_back_to_literal() {
        assert_eq!(
            path_from_uri("untitled:Untitled-1"),
            PathBuf::from("untitled:Untitled-1")
        );
    }

    #[test]
    fn unknown_reference_publishes_e042_diagnostic() {
        // Mirrors `mos-eval`'s `unknown_label_emits_e042` test: an
        // `@no:such` reference with no matching label should surface
        // an E042 diagnostic, here projected into the LSP shape with
        // a non-empty range covering the reference span.
        let file = PathBuf::from("/virtual/main.mos");
        let src = "see @no:such\n";
        let diagnostics = diagnostics_for_document(&file, src);
        let e042 = diagnostics
            .iter()
            .find(|d| d.code == "E042")
            .expect("E042 diagnostic must be present");
        assert_eq!(e042.severity, 1);
        assert_eq!(e042.source, "mosaic");
        assert!(
            e042.message.contains("no:such"),
            "expected message to mention the unknown label, got {:?}",
            e042.message
        );
        // The reference starts at byte 4 (`@`); the range must be
        // non-empty so the editor can highlight it.
        assert_eq!(e042.range.start.line, 0);
        assert!(e042.range.end.character > e042.range.start.character);
    }

    #[test]
    fn diagnostics_filter_by_file() {
        // A diagnostic whose span belongs to a different file is
        // dropped so the editor doesn't get phantom squigglies on the
        // wrong document.
        let file = PathBuf::from("/virtual/main.mos");
        let other = PathBuf::from("/virtual/other.mos");
        let src = "see @no:such\n";
        let lowered = mos_eval::lower(src, &file);
        assert!(
            lowered
                .diagnostics
                .iter()
                .any(|d| d.span.as_ref().is_some_and(|s| s.file == file)),
            "test setup: expected at least one diagnostic for {file:?}"
        );
        assert!(
            lowered
                .diagnostics
                .iter()
                .all(|d| project_diagnostic(&other, src, d).is_none()),
            "diagnostics for a different file must be filtered out"
        );
    }
}
