//! `textDocument/hover`: surface a node's attached `/** … */` doc comment.
//!
//! Hovering an `@label` / `@page(label)` reference shows the doc comment of the
//! block it points at; hovering the documented block itself (a heading, or a
//! labelled paragraph) shows its own. The `doc` attribute is stamped onto the
//! target node by the `mos-eval` lowerer ([`mos_eval::DOC_ATTR`]).
//!
//! Helper duplication (`span_contains`, `str_attr`) matches the house style in
//! [`crate::definition`] and [`crate::rename`], which keep their own copies.

use std::path::Path;

use mos_core::{AttrValue, Document, Node, NodeKind};
use mos_eval::DOC_ATTR;

use crate::{LspPosition, position_to_byte};

/// The doc-comment text to show for a hover at `position`, or `None` when the
/// cursor is not on a documented symbol.
#[must_use]
pub fn doc_at(
    document: &Document,
    file: &Path,
    src: &str,
    position: LspPosition,
) -> Option<String> {
    let offset = position_to_byte(src, position);

    // (a) On an `@label` / `@page(label)` reference: show the *target's* doc, so
    // documentation follows the symbol rather than only its declaration site.
    if let Some(label) = reference_label_at(document, file, offset)
        && let Some(node) = first_declaration_node(document, file, &label)
        && let Some(doc) = str_attr(node, DOC_ATTR)
    {
        return Some(doc);
    }

    // (b) On the documented block itself: the narrowest non-reference node whose
    // span covers the cursor and carries a doc attribute.
    document
        .nodes()
        .filter(|node| !matches!(node.kind, NodeKind::Reference | NodeKind::PageReference))
        .filter(|node| node.span.file == file && span_contains(node, offset))
        .filter(|node| node.attributes.contains_key(DOC_ATTR))
        .min_by_key(|node| node.span.end().saturating_sub(node.span.start()))
        .and_then(|node| str_attr(node, DOC_ATTR))
}

/// The label consumed by the narrowest reference node covering `offset`, or
/// `None` if the cursor sits on no reference. Mirrors
/// [`crate::definition`]'s resolution so hover and go-to-definition agree.
fn reference_label_at(document: &Document, file: &Path, offset: usize) -> Option<String> {
    document
        .nodes()
        .filter(|node| matches!(node.kind, NodeKind::Reference | NodeKind::PageReference))
        .filter(|node| node.span.file == file && span_contains(node, offset))
        .min_by_key(|node| node.span.end().saturating_sub(node.span.start()))
        .and_then(|node| str_attr(node, "label"))
}

/// The first block declaring `label`, in document order (first-declaration
/// wins, matching the resolver). References are excluded because they also
/// carry a `label` attribute (the target they point at).
fn first_declaration_node<'doc>(
    document: &'doc Document,
    file: &Path,
    label: &str,
) -> Option<&'doc Node> {
    document
        .nodes()
        .filter(|node| !matches!(node.kind, NodeKind::Reference | NodeKind::PageReference))
        .filter(|node| node.span.file == file)
        .find(|node| str_attr(node, "label").as_deref() == Some(label))
}

/// Whether `node`'s span covers `offset`, end-exclusive (a cursor resting just
/// past the final byte is outside), matching [`crate::definition`].
fn span_contains(node: &Node, offset: usize) -> bool {
    node.span.start() <= offset && offset < node.span.end()
}

fn str_attr(node: &Node, key: &str) -> Option<String> {
    match node.attributes.get(key) {
        Some(AttrValue::Str(value)) => Some(value.clone()),
        _ => None,
    }
}
