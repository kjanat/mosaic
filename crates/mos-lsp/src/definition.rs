//! `textDocument/definition` for `@label` cross-references (issue #71)
//! and `[@key]` citation keys.
//!
//! Given a cursor position, this resolves a reference (`@label` or
//! `@page(label)`) under the cursor to the source range of the label's
//! *declaration*. The lookup deliberately mirrors the compiler's own
//! `build_label_index` in `mos-eval`:
//!
//! - only blocks declare labels; [`NodeKind::Reference`] /
//!   [`NodeKind::PageReference`] nodes *consume* them, so they are
//!   skipped when scanning for the declaration site;
//! - **first declaration wins**: a label declared twice (a MOS0030
//!   error) resolves to its first occurrence, the same one the resolver
//!   keeps and points its "first declaration is here" note at.
//!
//! Scope is single-document: there is no workspace index, so a
//! definition always lands in the file the request names. Rename,
//! source/PDF sync, and generated labels (issue #31) are out of scope.

use std::fs;
use std::path::{Path, PathBuf};

use mos_core::{AttrValue, Document, NodeKind, SourceSpan};

use crate::diagnostics::{LspPosition, LspRange, span_to_range};

/// A resolved definition target. Label references point back into the
/// requested Mosaic source file; citation keys can point into a declared
/// BibTeX source.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct DefinitionTarget {
    pub path: PathBuf,
    pub range: LspRange,
}

/// Resolve the reference under `position` to the LSP range of its
/// label's first declaration, lowering `src` on the spot.
///
/// This is the un-cached entry point, kept for callers (and tests) that
/// hold only a source string. The server answers live requests through
/// [`definition_range_in`] instead, reusing a [`Document`] cached per edit
/// by the crate's lowering cache rather than re-lowering on every
/// `textDocument/definition`.
///
/// Returns `None` when the cursor is not on a reference, the referenced
/// label is undeclared, or (defensively) the declaration lives in a
/// different file than the request, which cannot happen for a
/// single-document lowering but keeps the contract explicit.
#[must_use]
pub fn definition_range(file: &Path, src: &str, position: LspPosition) -> Option<LspRange> {
    let lowered = mos_eval::lower(src, file);
    definition_target_in(&lowered.document, file, src, position).map(|target| target.range)
}

/// Resolve the reference under `position` against an already-lowered
/// `document`, returning the LSP range of its label's first declaration.
///
/// The caller supplies the [`Document`] (from a cache or a fresh lowering)
/// alongside the `src` it was lowered from: `src` is needed only to map
/// byte offsets to UTF-16 positions, the document carries the spans. Same
/// `None` contract as [`definition_range`].
#[must_use]
pub fn definition_range_in(
    document: &Document,
    file: &Path,
    src: &str,
    position: LspPosition,
) -> Option<LspRange> {
    definition_target_in(document, file, src, position).map(|target| target.range)
}

/// Resolve the reference or citation under `position` against an
/// already-lowered `document`, returning the target file and range.
#[must_use]
pub(crate) fn definition_target_in(
    document: &Document,
    file: &Path,
    src: &str,
    position: LspPosition,
) -> Option<DefinitionTarget> {
    let offset = position_to_byte(src, position);
    if let Some(label) = reference_label_at(document, file, offset) {
        let span = first_declaration_span(document, &label)?;
        return (span.file == file).then(|| DefinitionTarget {
            path: span.file.clone(),
            range: span_to_range(src, &span),
        });
    }
    let span = citation_target_span_at(document, file, offset)?;
    let target_src = fs::read_to_string(&span.file).ok()?;
    Some(DefinitionTarget {
        path: span.file.clone(),
        range: span_to_range(&target_src, &span),
    })
}

/// The label consumed by the narrowest reference node whose span covers
/// `offset`, or `None` if the cursor sits on no reference. Narrowest
/// wins so that, in the unlikely event reference spans ever nest, the
/// innermost one is selected.
fn reference_label_at(document: &Document, file: &Path, offset: usize) -> Option<String> {
    document
        .nodes()
        .filter(|node| matches!(node.kind, NodeKind::Reference | NodeKind::PageReference))
        .filter(|node| node.span.file == file && span_contains(&node.span, offset))
        .min_by_key(|node| node.span.end().saturating_sub(node.span.start()))
        .and_then(|node| match node.attributes.get("label") {
            Some(AttrValue::Str(label)) => Some(label.clone()),
            _ => None,
        })
}

fn citation_target_span_at(document: &Document, file: &Path, offset: usize) -> Option<SourceSpan> {
    let node = document
        .nodes()
        .filter(|node| node.kind == NodeKind::Citation)
        .filter(|node| node.span.file == file && span_contains(&node.span, offset))
        .min_by_key(|node| node.span.end().saturating_sub(node.span.start()))?;
    let Some(AttrValue::Str(path)) = node.attributes.get("target_path") else {
        return None;
    };
    let start = attr_usize(node.attributes.get("target_span.start"))?;
    let end = attr_usize(node.attributes.get("target_span.end"))?;
    (start <= end).then(|| SourceSpan::new(PathBuf::from(path), start, end))
}

/// The declaration span of the first block declaring `label`, in
/// document order.
///
/// `nodes()` yields the arena in allocation order, which the lowerer
/// fills in source order, so `find` returns the first declaration:
/// matching the resolver's first-wins rule. References are excluded
/// because they also carry a `label` attribute (the target they point
/// at), and treating one as a declaration would shadow the real block.
///
/// The returned span is the label *token* (`intro` in `= Intro <intro>`)
/// when the lowerer stamped one, so go-to-definition lands the caret on
/// the declaration itself rather than the whole heading/figure block. It
/// falls back to the block span only when no label-token span is
/// recorded.
fn first_declaration_span(document: &Document, label: &str) -> Option<SourceSpan> {
    let node = document
        .nodes()
        .filter(|node| !matches!(node.kind, NodeKind::Reference | NodeKind::PageReference))
        .find(|node| {
            matches!(node.attributes.get("label"), Some(AttrValue::Str(declared)) if declared == label)
        })?;
    Some(label_token_span(node).unwrap_or_else(|| node.span.clone()))
}

/// The source span of a declaration's label token, read from the
/// `label_span.start` / `label_span.end` attributes the `mos-eval`
/// lowerer stamps (the same span the resolver targets for its MOS0030
/// rename fix-it). `None` when the attributes are absent or malformed,
/// letting the caller fall back to the block span.
fn label_token_span(node: &mos_core::Node) -> Option<SourceSpan> {
    let start = attr_usize(node.attributes.get("label_span.start"))?;
    let end = attr_usize(node.attributes.get("label_span.end"))?;
    (start <= end).then(|| SourceSpan::new(node.span.file.clone(), start, end))
}

fn attr_usize(value: Option<&AttrValue>) -> Option<usize> {
    match value {
        Some(AttrValue::Int(value)) => usize::try_from(*value).ok(),
        _ => None,
    }
}

#[must_use]
pub(crate) fn path_to_uri(path: &Path) -> String {
    let mut raw = path.to_string_lossy().replace('\\', "/");
    if !raw.starts_with('/') && raw.as_bytes().get(1).is_none_or(|byte| *byte != b':') {
        raw.insert(0, '/');
    }
    format!("file://{}", percent_encode_uri_path(&raw))
}

fn percent_encode_uri_path(path: &str) -> String {
    let mut encoded = String::new();
    for byte in path.bytes() {
        if matches!(byte, b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' | b'/' | b':')
        {
            encoded.push(char::from(byte));
        } else {
            encoded.push('%');
            encoded.push(hex_digit(byte >> 4));
            encoded.push(hex_digit(byte & 0x0f));
        }
    }
    encoded
}

const fn hex_digit(value: u8) -> char {
    match value {
        0..=9 => (b'0' + value) as char,
        _ => (b'A' + (value - 10)) as char,
    }
}

/// Whether `span` covers `offset`. The end is exclusive: a cursor
/// resting just past the final byte of `@intro` is treated as off the
/// token, matching how editors place the caret inside the identifier
/// when invoking go-to-definition.
const fn span_contains(span: &SourceSpan, offset: usize) -> bool {
    span.start() <= offset && offset < span.end()
}

/// Convert an LSP [`LspPosition`] (zero-based line, UTF-16 `character`)
/// into a byte offset inside `src`. The inverse of
/// [`byte_to_position`](crate::diagnostics::byte_to_position).
///
/// Positions beyond the end of a line clamp to the line's terminating
/// newline; positions beyond the last line clamp to `src.len()`. A
/// `character` that would land inside a surrogate pair rounds up to the
/// following code-point boundary, so the returned offset is always a
/// valid `char` boundary.
#[must_use]
pub fn position_to_byte(src: &str, position: LspPosition) -> usize {
    let Some(line_start) = line_start_offset(src, position.line) else {
        return src.len();
    };
    let mut utf16: u32 = 0;
    for (byte_in_line, ch) in src[line_start..].char_indices() {
        if ch == '\n' || utf16 >= position.character {
            return line_start + byte_in_line;
        }
        utf16 = utf16.saturating_add(u32::try_from(ch.len_utf16()).unwrap_or(0));
    }
    src.len()
}

/// Byte offset of the start of `line` (zero-based). `None` if the
/// document has no such line, signalling a clamp to end-of-document.
fn line_start_offset(src: &str, line: u32) -> Option<usize> {
    if line == 0 {
        return Some(0);
    }
    let mut seen: u32 = 0;
    for (i, byte) in src.bytes().enumerate() {
        if byte == b'\n' {
            seen = seen.saturating_add(1);
            if seen == line {
                return Some(i + 1);
            }
        }
    }
    None
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

    fn position(line: u32, character: u32) -> LspPosition {
        LspPosition { line, character }
    }

    #[test]
    fn position_to_byte_inverts_byte_to_position() {
        use crate::diagnostics::byte_to_position;
        let src = "= µ字 <intro>\nsee @intro\n";
        for offset in 0..=src.len() {
            if !src.is_char_boundary(offset) {
                continue;
            }
            let round_tripped = position_to_byte(src, byte_to_position(src, offset));
            assert_eq!(
                round_tripped, offset,
                "byte {offset} did not round-trip (got {round_tripped})"
            );
        }
    }

    #[test]
    fn position_past_line_end_clamps_to_newline() {
        let src = "ab\ncd\n";
        // Column 99 on line 0 clamps to the newline after `ab` (byte 2).
        assert_eq!(position_to_byte(src, position(0, 99)), 2);
    }

    #[test]
    fn position_past_last_line_clamps_to_end() {
        let src = "ab\ncd";
        assert_eq!(position_to_byte(src, position(9, 0)), src.len());
    }

    #[test]
    fn resolves_reference_to_section_declaration() {
        let file = PathBuf::from("/virtual/main.mos");
        let src = "= Intro <intro>\n\nSee @intro here.\n";
        // Cursor inside the `@intro` reference on line 2.
        let at = src.find("@intro").expect("reference present");
        let position = byte_position(src, at + 2);
        let range = definition_range(&file, src, position).expect("definition resolved");
        // The declaration's label token sits on the heading line 0.
        assert_eq!(range.start.line, 0);
        // The range must be non-empty, not collapse to a point.
        assert!(range.end.character > range.start.character || range.end.line > range.start.line);
    }

    #[test]
    fn definition_targets_the_label_token_not_the_whole_block() {
        // Issue #71 acceptance: `@intro` jumps to the `<intro>` label
        // declaration, not the whole `= Intro <intro>` heading. The label
        // token is the identifier `intro` between the angle brackets, so
        // the returned range must cover exactly those bytes.
        let file = PathBuf::from("/virtual/main.mos");
        let src = "= Intro <intro>\n\nSee @intro here.\n";
        let reference = src.find("@intro").expect("reference present");
        let range = definition_range(&file, src, byte_position(src, reference + 2))
            .expect("definition resolved");

        // `<intro>` opens at the `<`; the token itself starts one byte in.
        let token_start = src.find("<intro>").expect("label present") + 1;
        let token_end = token_start + "intro".len();
        assert_eq!(range.start, byte_position(src, token_start));
        assert_eq!(range.end, byte_position(src, token_end));
    }

    #[test]
    fn unknown_label_resolves_to_nothing() {
        let file = PathBuf::from("/virtual/main.mos");
        let src = "See @nope here.\n";
        let at = src.find("@nope").expect("reference present");
        assert!(definition_range(&file, src, byte_position(src, at + 1)).is_none());
    }

    #[test]
    fn cursor_off_any_reference_resolves_to_nothing() {
        let file = PathBuf::from("/virtual/main.mos");
        let src = "= Intro <intro>\n\nSee @intro here.\n";
        // Column 0 of the heading line is plain text, not a reference.
        assert!(definition_range(&file, src, position(0, 0)).is_none());
    }

    #[test]
    fn duplicate_label_resolves_to_first_declaration() {
        let file = PathBuf::from("/virtual/main.mos");
        let src = "= First <dup>\n\n= Second <dup>\n\nSee @dup here.\n";
        let at = src.find("@dup").expect("reference present");
        let range =
            definition_range(&file, src, byte_position(src, at + 2)).expect("definition resolved");
        // First declaration is the line-0 heading, not the line-2 one.
        assert_eq!(range.start.line, 0);
    }

    /// Build an [`LspPosition`] for `offset` via the production
    /// byte→position mapping, so reference tests address the cursor the
    /// same way an editor would.
    fn byte_position(src: &str, offset: usize) -> LspPosition {
        crate::diagnostics::byte_to_position(src, offset)
    }
}
