//! `textDocument/rename` for `@label` cross-references; the label-rename
//! groundwork (LSP tracker: prepare label rename support).
//!
//! Given a cursor on a label: either a declaration's `<label>` token or a
//! `@label` / `@page(label)` reference; this collects every source range that
//! spells that label so the editor can rewrite them together:
//!
//! - the **first** declaration's label token (`intro` in `= Intro <intro>`),
//!   mirroring the resolver's first-declaration-wins rule, so a duplicated
//!   label still renames from its canonical site;
//! - the identifier inside every `@label` reference and every `@page(label)`
//!   reference.
//!
//! The ranges deliberately cover only the *identifier*, never the sigil or
//! delimiters: `@intro` rewrites `intro` (the `@` stays), `@page(intro)`
//! rewrites the `intro` between the parentheses, and `<intro>` rewrites the
//! token between the angle brackets. Both declarations and references carry a
//! stamped `label_span` covering exactly that identifier (issue #116), so
//! every range here is read from one attribute rather than computed from span
//! geometry.
//!
//! Scope is single-document: there is no workspace index, no file watching,
//! and no validation of the new name; those are out of this slice.

use std::path::Path;

use mos_core::{AttrValue, Document, NodeKind, SourceSpan};

use crate::definition::position_to_byte;
use crate::diagnostics::{LspPosition, LspRange, span_to_range};

/// Collect every editable range for renaming the label under `position`: the
/// first declaration's label token plus every reference's identifier.
///
/// Returns `None` when the cursor is not on a label at all (neither a
/// declaration token nor a reference), or when the label has no editable
/// occurrence: leaving the caller to answer the rename request with `null`.
#[must_use]
pub fn ranges(
    document: &Document,
    file: &Path,
    src: &str,
    position: LspPosition,
) -> Option<Vec<LspRange>> {
    let offset = position_to_byte(src, position);
    let label = label_under_cursor(document, file, offset)?;

    let mut spans: Vec<SourceSpan> = Vec::new();
    if let Some(declaration) = first_declaration_label_token(document, file, &label) {
        spans.push(declaration);
    }
    for node in document.nodes() {
        if !matches!(node.kind, NodeKind::Reference | NodeKind::PageReference) {
            continue;
        }
        if node.span.file != file || str_attr(node, "label").as_deref() != Some(label.as_str()) {
            continue;
        }
        if let Some(span) = label_token_span(node) {
            spans.push(span);
        }
    }

    if spans.is_empty() {
        return None;
    }
    Some(spans.iter().map(|span| span_to_range(src, span)).collect())
}

/// The label spelled at `offset`, whether the cursor sits inside a reference
/// or inside a declaration's label token.
///
/// The hit-test matches exactly the bytes a rename would *edit*, never more:
/// every label-bearing node carries a stamped `label_span` covering just the
/// identifier (the `intro` in `<intro>`, `@intro`, or `@page(intro)`):
///
/// - references are tested against that identifier span, so a cursor on the
///   `@` sigil, the `@page(` prefix, or the closing `)` is *not* on the label;
/// - a declaration matches only when `offset` lands inside its label-token
///   span **and** that declaration is the *canonical* (first) one for the
///   label. A cursor on a duplicate later `<dup>` returns `None` rather than
///   renaming the first declaration out from under the cursor.
///
/// References win over declarations (and narrowest reference wins), mirroring
/// [`crate::definition`].
fn label_under_cursor(document: &Document, file: &Path, offset: usize) -> Option<String> {
    let on_reference = document
        .nodes()
        .filter(|node| matches!(node.kind, NodeKind::Reference | NodeKind::PageReference))
        .filter_map(|node| label_token_span(node).map(|span| (node, span)))
        .filter(|(_, span)| span.file == file && span_contains(span, offset))
        .min_by_key(|(_, span)| span.end().saturating_sub(span.start()))
        .and_then(|(node, _)| str_attr(node, "label"));
    if on_reference.is_some() {
        return on_reference;
    }

    let declaration = document
        .nodes()
        .filter(|node| !matches!(node.kind, NodeKind::Reference | NodeKind::PageReference))
        .find(|node| {
            label_token_span(node)
                .is_some_and(|span| span.file == file && span_contains(&span, offset))
        })?;
    let label = str_attr(declaration, "label")?;
    // Only the canonical first declaration may drive the rename.
    let canonical = first_declaration_node(document, file, &label)?;
    (canonical.id == declaration.id).then_some(label)
}

/// The first block declaring `label`, in document order; the canonical
/// declaration the resolver resolves references to (first-declaration-wins).
fn first_declaration_node<'doc>(
    document: &'doc Document,
    file: &Path,
    label: &str,
) -> Option<&'doc mos_core::Node> {
    document
        .nodes()
        .filter(|node| !matches!(node.kind, NodeKind::Reference | NodeKind::PageReference))
        .filter(|node| node.span.file == file)
        .find(|node| str_attr(node, "label").as_deref() == Some(label))
}

/// The label-token span of the canonical first declaration of `label`. `None`
/// when it carries no stamped label span (so there is nothing safe to rewrite).
fn first_declaration_label_token(
    document: &Document,
    file: &Path,
    label: &str,
) -> Option<SourceSpan> {
    first_declaration_node(document, file, label).and_then(label_token_span)
}

/// A node's label-identifier span, read from the `label_span.start` /
/// `label_span.end` attributes the `mos-eval` lowerer stamps; the `intro` in
/// `<intro>` for a declaration and in `@intro` / `@page(intro)` for a
/// reference (issue #116). This is the editable identifier range, excluding
/// the `@` sigil, the `<>` brackets, and the `@page(`…`)` wrapper. `None` when
/// the attributes are absent or malformed.
fn label_token_span(node: &mos_core::Node) -> Option<SourceSpan> {
    let start = usize::try_from(int_attr(node, "label_span.start")?).ok()?;
    let end = usize::try_from(int_attr(node, "label_span.end")?).ok()?;
    (start <= end).then(|| SourceSpan::new(node.span.file.clone(), start, end))
}

/// Whether `span` covers `offset`, end-exclusive: a cursor resting just past
/// the final byte is treated as outside, matching [`crate::definition`].
const fn span_contains(span: &SourceSpan, offset: usize) -> bool {
    span.start() <= offset && offset < span.end()
}

fn str_attr(node: &mos_core::Node, key: &str) -> Option<String> {
    match node.attributes.get(key) {
        Some(AttrValue::Str(value)) => Some(value.clone()),
        _ => None,
    }
}

fn int_attr(node: &mos_core::Node, key: &str) -> Option<i64> {
    match node.attributes.get(key) {
        Some(AttrValue::Int(value)) => Some(*value),
        _ => None,
    }
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
    use crate::diagnostics::byte_to_position;

    /// Resolve `position` for the byte `offset` via the production mapping.
    fn at(src: &str, offset: usize) -> LspPosition {
        byte_to_position(src, offset)
    }

    /// The source substring a range covers, via a byte round-trip: lets a
    /// test assert *what text* an edit range points at, not just coordinates.
    fn ranged<'a>(src: &'a str, range: &LspRange) -> &'a str {
        let start = position_to_byte(src, range.start);
        let end = position_to_byte(src, range.end);
        &src[start..end]
    }

    fn found_ranges(src: &str, cursor: usize) -> Vec<LspRange> {
        let file = PathBuf::from("/virtual/main.mos");
        let lowered = mos_eval::lower(src, &file);
        ranges(&lowered.document, &file, src, at(src, cursor)).unwrap_or_default()
    }

    #[test]
    fn renames_declaration_and_all_references_to_identifier_only() {
        let src = "= Intro <intro>\n\nSee @intro and @page(intro).\n";
        // Cursor inside the `@intro` reference.
        let cursor = src.find("@intro").expect("reference") + 2;
        let found = found_ranges(src, cursor);
        // Declaration token + `@intro` ref + `@page(intro)` ref = 3 ranges.
        assert_eq!(found.len(), 3, "decl + two refs: {found:?}");
        // Every range covers exactly the identifier `intro`; never the `@`,
        // the angle brackets, or the `@page(`…`)` delimiters.
        for range in &found {
            assert_eq!(
                ranged(src, range),
                "intro",
                "range must cover only the identifier"
            );
        }
    }

    #[test]
    fn rename_from_declaration_cursor_finds_references() {
        let src = "= Intro <intro>\n\nSee @intro here.\n";
        // Cursor inside the `<intro>` declaration token.
        let cursor = src.find("<intro>").expect("declaration") + 1;
        let found = found_ranges(src, cursor);
        assert_eq!(found.len(), 2, "decl token + one ref: {found:?}");
        assert!(found.iter().all(|r| ranged(src, r) == "intro"));
    }

    #[test]
    fn rename_from_page_reference_cursor_covers_inner_label() {
        let src = "= Intro <intro>\n\nOn @page(intro).\n";
        let cursor = src.find("@page(intro)").expect("page ref") + "@page(".len();
        let found = found_ranges(src, cursor);
        assert_eq!(found.len(), 2, "decl token + page ref: {found:?}");
        assert!(found.iter().all(|r| ranged(src, r) == "intro"));
    }

    #[test]
    fn duplicate_label_renames_from_first_declaration() {
        // First-declaration-wins: only the first `<dup>` token renames; the
        // duplicate later declaration is left untouched (it is a MOS0030
        // duplicate, not part of the canonical label's occurrence set).
        let src = "= First <dup>\n\n= Second <dup>\n\nSee @dup here.\n";
        let cursor = src.find("@dup").expect("reference") + 2;
        let found = found_ranges(src, cursor);
        assert_eq!(found.len(), 2, "first decl token + the ref only: {found:?}");
        // The first declaration token sits on line 0, not the line-2 duplicate.
        assert!(found.iter().any(|r| r.start.line == 0));
        assert!(
            !found.iter().any(|r| r.start.line == 2),
            "the duplicate second declaration must not be renamed"
        );
    }

    #[test]
    fn cursor_off_any_label_yields_nothing() {
        let src = "= Intro <intro>\n\nSee @intro here.\n";
        let file = PathBuf::from("/virtual/main.mos");
        let lowered = mos_eval::lower(src, &file);
        // Column 0 of the heading is the `=` glyph, not a label.
        assert!(ranges(&lowered.document, &file, src, at(src, 0)).is_none());
    }

    #[test]
    fn cursor_on_reference_sigil_or_delimiters_yields_nothing() {
        // The hit-test must match only the editable identifier, never the
        // surrounding syntax: otherwise a cursor on `@`, inside `@page(`, or
        // on the closing `)` would rename a label whose `@`/parens it can't
        // actually edit.
        let src = "= Intro <intro>\n\nSee @intro and @page(intro).\n";
        let reference = src.find("@intro").expect("reference");
        assert!(
            found_ranges(src, reference).is_empty(),
            "cursor on the `@` sigil is not on the label"
        );

        let page = src.find("@page(intro)").expect("page reference");
        assert!(
            found_ranges(src, page + 2).is_empty(),
            "cursor inside the `@page(` prefix is not on the label"
        );
        let closing = page + "@page(intro".len();
        assert_eq!(&src[closing..=closing], ")", "offset points at the `)`");
        assert!(
            found_ranges(src, closing).is_empty(),
            "cursor on the closing `)` is not on the label"
        );
    }

    #[test]
    fn cursor_on_duplicate_declaration_yields_nothing() {
        // Renaming may only be driven from the canonical first declaration: a
        // cursor on the duplicate later `<dup>` returns nothing rather than
        // silently editing the first token instead of the one under the cursor.
        let src = "= First <dup>\n\n= Second <dup>\n\nSee @dup here.\n";
        let second = src.rfind("<dup>").expect("second declaration") + 1;
        assert!(
            found_ranges(src, second).is_empty(),
            "cursor on a duplicate declaration must not rename the canonical one"
        );
        // The canonical first declaration still drives the rename.
        let first = src.find("<dup>").expect("first declaration") + 1;
        assert_eq!(
            found_ranges(src, first).len(),
            2,
            "canonical declaration token + the reference"
        );
    }

    #[test]
    fn renames_styled_reference_to_identifier_only() {
        // A reference inside emphasis: the parser widens the reference node's
        // span to the `*…*` delimiters, but the stamped `label_span` still
        // covers exactly the identifier. Rename must edit `intro`, never
        // `@intro*`; this is the latent range drift #116 removed by reading
        // the stamped span instead of deriving from node-span geometry.
        let src = "= Intro <intro>\n\nSee *@intro* now.\n";
        let cursor = src.find("@intro").expect("reference") + 2;
        let found = found_ranges(src, cursor);
        assert_eq!(
            found.len(),
            2,
            "decl token + the styled reference: {found:?}"
        );
        for range in &found {
            assert_eq!(
                ranged(src, range),
                "intro",
                "edit covers only the identifier, not the `@` or the `*` delimiters"
            );
        }
    }
}
