//! Page-reference resolution and its layout fixpoint (issue #72).
//!
//! `@page(label)` resolves to the *printed page number* of `label`'s target.
//! Unlike a section or figure number, that page number is only known after
//! layout, so it cannot be resolved in the single lowering pass: a page
//! reference's rendered width can shift pagination, which can move the target
//! to a different page, which changes the number. The fixpoint drives layout
//! repeatedly until the label→page map stabilizes.
//!
//! Responsibilities are split by which input each step needs:
//!
//! - [`validate_page_references`](crate::resolve) runs at lower time, where the
//!   label *index* exists, and reports an undeclared `@page(x)` as `MOS0033`,
//!   exactly like an undeclared `@x`. (It lives in `resolve` next to the index.)
//! - [`resolve_page_references`] runs each fixpoint iteration with a label→page
//!   *map* from layout and rewrites each page reference's text to the number.
//! - [`resolve_page_reference_fixpoint`] is the driver. Layout is *injected* as
//!   a closure so this module keeps no `mos-layout` dependency (the one-way
//!   crate flow holds) and the loop is unit-testable with a mock layout.

use std::collections::BTreeMap;

use mos_core::{AttrValue, Document, NodeId, NodeKind};

/// Rewrite every `@page(label)` reference's visible text to its target's page
/// number, drawn from `label_pages` (a label→1-based-page map produced by
/// layout). Returns whether any text changed, so the [fixpoint
/// driver](resolve_page_reference_fixpoint) can tell when the document settled.
///
/// A label absent from `label_pages` resolves to its `?label?` placeholder: an
/// *undeclared* label was already reported as `MOS0033` at lower time, and a
/// *declared* label whose target produced no content simply has no page. The
/// placeholder is written every call, so a reference whose label *drops* out of
/// a later map reverts from a stale number back to the placeholder rather than
/// keeping it.
///
/// Idempotent and a pure function of the document's page-reference labels and
/// `label_pages`: the text is re-derived from the map each call, never from the
/// previously-written number, so repeated calls with the same map are no-ops
/// and the result never depends on call history.
pub fn resolve_page_references(
    document: &mut Document,
    label_pages: &BTreeMap<String, u32>,
) -> bool {
    let page_refs: Vec<NodeId> = document
        .nodes()
        .filter(|node| node.kind == NodeKind::PageReference)
        .map(|node| node.id)
        .collect();

    let mut changed = false;
    for id in page_refs {
        let Some(node) = document.get(id) else {
            continue;
        };
        let Some(AttrValue::Str(label)) = node.attributes.get("label").cloned() else {
            continue;
        };
        // Re-derive the text every call: a present label resolves to its page
        // number, an absent one to the `?label?` placeholder (matching the
        // lowering fallback). Deriving unconditionally is what keeps a dropped
        // label from leaving a stale number behind.
        let resolved = AttrValue::Str(
            label_pages
                .get(&label)
                .map_or_else(|| format!("?{label}?"), u32::to_string),
        );
        if let Some(node) = document.get_mut(id)
            && node.attributes.get("text") != Some(&resolved)
        {
            node.attributes.insert("text".to_owned(), resolved);
            changed = true;
        }
    }
    changed
}

/// The result of driving page references to a fixpoint.
#[derive(Clone, Copy, Eq, PartialEq, Debug)]
pub enum PageFixpointOutcome {
    /// The label→page map stabilized; the rendered page numbers are final.
    Converged {
        /// Resolve↔layout rounds run before the map settled.
        iterations: u32,
    },
    /// The map never settled: it oscillated, or the iteration cap was hit.
    /// The caller keeps the last computed page numbers and should report
    /// `MOS0047`.
    NotConverged {
        /// Resolve↔layout rounds run before giving up.
        iterations: u32,
    },
}

/// Drive page-reference resolution to a fixpoint against an injected `layout`.
///
/// `layout` lays `document` out and returns its label→page map plus the full
/// layout artifact `T`, so the caller keeps the final artifact without an extra
/// pass. Each round resolves page references from the previous map, then
/// re-lays-out; it converges when the map stops changing. Oscillation (a
/// previously-seen map recurs) and exhausting `max_iterations` both yield
/// [`NotConverged`](PageFixpointOutcome::NotConverged) with the most recent
/// artifact.
///
/// Layout is a parameter rather than a direct call so `mos-eval` does not depend
/// on `mos-layout` and the convergence logic can be tested with a mock.
pub fn resolve_page_reference_fixpoint<T>(
    document: &mut Document,
    mut layout: impl FnMut(&Document) -> (BTreeMap<String, u32>, T),
    max_iterations: u32,
) -> (PageFixpointOutcome, T) {
    let (mut map, mut artifact) = layout(document);
    let mut seen = vec![map.clone()];
    let mut iterations = 0;
    while iterations < max_iterations {
        iterations += 1;
        if !resolve_page_references(document, &map) {
            // No page-reference text changed: the document has settled.
            return (PageFixpointOutcome::Converged { iterations }, artifact);
        }
        let (next_map, next_artifact) = layout(document);
        artifact = next_artifact;
        if next_map == map {
            // The numbers we wrote match the layout they produced.
            return (PageFixpointOutcome::Converged { iterations }, artifact);
        }
        if seen.contains(&next_map) {
            // A previously-seen map recurred without converging: oscillation.
            return (PageFixpointOutcome::NotConverged { iterations }, artifact);
        }
        seen.push(next_map.clone());
        map = next_map;
    }
    (PageFixpointOutcome::NotConverged { iterations }, artifact)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    use mos_core::{AttrValue, Document, NodeKind};

    use super::{PageFixpointOutcome, resolve_page_reference_fixpoint, resolve_page_references};
    use crate::lower;

    fn page_map(pairs: &[(&str, u32)]) -> BTreeMap<String, u32> {
        pairs.iter().map(|(k, v)| ((*k).to_owned(), *v)).collect()
    }

    fn page_ref_text(document: &Document, label: &str) -> Option<String> {
        document
            .nodes()
            .filter(|n| n.kind == NodeKind::PageReference)
            .find(|n| n.attributes.get("label") == Some(&AttrValue::Str(label.to_owned())))
            .and_then(|n| match n.attributes.get("text") {
                Some(AttrValue::Str(text)) => Some(text.clone()),
                _ => None,
            })
    }

    fn doc_with_one_page_ref() -> Document {
        // The `?x?` placeholder survives lowering; the MOS0033 for the
        // undeclared label is irrelevant to these fixpoint-driver tests.
        lower("See @page(x) here.\n", &PathBuf::from("test.mos")).document
    }

    #[test]
    fn resolve_writes_the_page_number_and_is_idempotent() {
        let mut document = doc_with_one_page_ref();
        let map = page_map(&[("x", 3)]);
        assert!(resolve_page_references(&mut document, &map));
        assert_eq!(page_ref_text(&document, "x"), Some("3".to_owned()));
        // Re-running with the same map changes nothing.
        assert!(!resolve_page_references(&mut document, &map));
    }

    #[test]
    fn resolve_reverts_to_placeholder_when_a_label_drops_from_the_map() {
        // The text is a pure function of the current map: if a label that was
        // resolved to a page disappears from a later map, its stale number must
        // revert to the placeholder rather than linger.
        let mut document = doc_with_one_page_ref();
        assert!(resolve_page_references(
            &mut document,
            &page_map(&[("x", 3)])
        ));
        assert_eq!(page_ref_text(&document, "x"), Some("3".to_owned()));

        assert!(resolve_page_references(&mut document, &page_map(&[])));
        assert_eq!(page_ref_text(&document, "x"), Some("?x?".to_owned()));
    }

    #[test]
    fn resolve_leaves_a_label_with_no_page_as_placeholder() {
        let mut document = doc_with_one_page_ref();
        assert!(!resolve_page_references(&mut document, &page_map(&[])));
        assert_eq!(page_ref_text(&document, "x"), Some("?x?".to_owned()));
    }

    #[test]
    fn fixpoint_converges_when_the_map_is_stable() {
        let mut document = doc_with_one_page_ref();
        let (outcome, ()) =
            resolve_page_reference_fixpoint(&mut document, |_doc| (page_map(&[("x", 2)]), ()), 8);
        assert_eq!(outcome, PageFixpointOutcome::Converged { iterations: 1 });
        assert_eq!(page_ref_text(&document, "x"), Some("2".to_owned()));
    }

    #[test]
    fn fixpoint_converges_immediately_with_no_page_references() {
        let mut document = lower("plain paragraph\n", &PathBuf::from("test.mos")).document;
        let (outcome, ()) =
            resolve_page_reference_fixpoint(&mut document, |_doc| (page_map(&[]), ()), 8);
        assert_eq!(outcome, PageFixpointOutcome::Converged { iterations: 1 });
    }

    #[test]
    fn fixpoint_reports_non_convergence_on_oscillation() {
        // A mock layout that flip-flops the page between two values: resolving
        // never settles, and the first map recurs, so the driver gives up.
        let mut document = doc_with_one_page_ref();
        let mut round = 0_u32;
        let (outcome, ()) = resolve_page_reference_fixpoint(
            &mut document,
            |_doc| {
                round += 1;
                // maps: round 1 -> {x:1}, 2 -> {x:2}, 3 -> {x:1} (recurs)
                let page = if round % 2 == 1 { 1 } else { 2 };
                (page_map(&[("x", page)]), ())
            },
            8,
        );
        assert_eq!(outcome, PageFixpointOutcome::NotConverged { iterations: 2 });
    }

    #[test]
    fn fixpoint_reports_non_convergence_at_the_iteration_cap() {
        // A mock layout whose page strictly increases every call never repeats
        // and never stabilizes, so the cap is the only stop.
        let mut document = doc_with_one_page_ref();
        let mut page = 0_u32;
        let (outcome, ()) = resolve_page_reference_fixpoint(
            &mut document,
            |_doc| {
                page += 1;
                (page_map(&[("x", page)]), ())
            },
            4,
        );
        assert_eq!(outcome, PageFixpointOutcome::NotConverged { iterations: 4 });
    }
}
