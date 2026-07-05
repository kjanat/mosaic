//! Bibliography entry-list layout.
//!
//! Renders the entry children that `mos-eval` attaches to a
//! [`NodeKind::Bibliography`] node (paragraphs carrying an `entry_number`
//! attribute) as a numbered hanging-indent list: a right-aligned `[N]`
//! marker in a shared gutter, entry text flowing at the indented column.
//! Mirrors `layout_list`'s ordered-marker geometry so bibliographies and
//! numbered lists read consistently.

use mos_core::{AttrValue, Document, Node, NodeKind};
use mos_fonts::{shape_with_fallback, text_width};

use crate::word::Word;
use crate::{LIST_MARKER_GUTTER_PT, LayoutState, PARA_SPACE_AFTER_PT, PendingMarker};

impl LayoutState {
    /// Lay out a [`NodeKind::Bibliography`] node's rendered entry children.
    ///
    /// A declaration-only node (nothing cited, or a second `#bibliography`
    /// declaration) has no entry children and emits nothing, preserving the
    /// pre-rendering behavior of the directive being invisible in the PDF.
    pub(super) fn layout_bibliography(&mut self, document: &Document, bib_node: &Node) {
        let entries: Vec<&Node> = bib_node
            .children
            .iter()
            .filter_map(|id| document.get(*id))
            .filter(|node| {
                node.kind == NodeKind::Paragraph && node.attributes.contains_key("entry_number")
            })
            .collect();
        if entries.is_empty() {
            return;
        }

        let regular = self.text.family.regular;
        let size = self.text.size_pt;
        let leading = self.text.leading;
        let saved_left = self.current_left_pt;

        // Size the gutter to the widest `[N]` marker so multi-digit entry
        // numbers never overlap the entry text (the `layout_list` ordered
        // gutter rule).
        let widest_marker_pt = entries
            .iter()
            .map(|entry| {
                shape_with_fallback(
                    regular,
                    self.text.family.fallbacks,
                    size,
                    &marker_text(entry),
                )
                .iter()
                .map(|s| s.advance_pt)
                .sum::<f32>()
            })
            .fold(0.0_f32, f32::max);
        let marker_gap_pt = text_width(regular, size, " ");
        let gutter = (widest_marker_pt + marker_gap_pt).max(LIST_MARKER_GUTTER_PT);
        let entry_left = saved_left + gutter;

        for entry in entries {
            let text = marker_text(entry);
            let subruns = shape_with_fallback(regular, self.text.family.fallbacks, size, &text);
            let width_pt: f32 = subruns.iter().map(|s| s.advance_pt).sum();
            let marker_word = Word {
                text,
                actual_text: None,
                space_before_pt: 0.0,
                font: regular,
                size_pt: size,
                width_pt,
                subruns,
                shy_break_offsets: Vec::new(),
            };
            let marker_x = entry_left - marker_gap_pt - marker_word.width_pt;

            self.current_left_pt = entry_left;
            self.pending_marker = Some(PendingMarker {
                x_pt: marker_x,
                word: marker_word,
            });

            let words = self.collect_words(document, entry, regular, size);
            if words.is_empty() {
                self.flush_line(&[], leading);
            } else {
                self.flow_words(&words, leading);
                if self.pending_marker.is_some() {
                    self.flush_line(&[], leading);
                }
            }
        }

        self.current_left_pt = saved_left;
        self.cursor_y += PARA_SPACE_AFTER_PT;
    }
}

/// The `[N]` marker for one entry paragraph. `entry_number` is stamped by
/// `mos-eval` for every entry it renders, so the fallback is unreachable on
/// compiler-produced documents; it keeps hand-built documents visible
/// instead of panicking.
fn marker_text(entry: &Node) -> String {
    match entry.attributes.get("entry_number") {
        Some(AttrValue::Int(number)) => format!("[{number}]"),
        _ => "[?]".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::expect_used,
        reason = "tests panic loudly on setup failure; matches crate-wide test-module convention"
    )]

    use std::path::PathBuf;

    use mos_core::{AttrMap, NodeId, NodeSpec, SourceSpan};

    use crate::{LayoutEngine, MARGIN_PT, TextRun};

    use super::*;

    fn node(kind: NodeKind, attributes: AttrMap) -> NodeSpec {
        NodeSpec::new(kind, SourceSpan::placeholder(PathBuf::from("test.mos")))
            .with_attributes(attributes)
    }

    fn pin_helvetica(doc: &mut Document) {
        let mut attrs = AttrMap::new();
        attrs.insert("set".to_owned(), AttrValue::Str("text".to_owned()));
        attrs.insert(
            "set.arg.font".to_owned(),
            AttrValue::Str("Helvetica".to_owned()),
        );
        doc.alloc_child(doc.root, node(NodeKind::Raw, attrs));
    }

    fn alloc_bibliography(doc: &mut Document) -> NodeId {
        doc.alloc_child(doc.root, node(NodeKind::Bibliography, AttrMap::new()))
    }

    fn alloc_entry(doc: &mut Document, bib: NodeId, number: i64, text: &str) {
        let mut attrs = AttrMap::new();
        attrs.insert("entry_number".to_owned(), AttrValue::Int(number));
        attrs.insert("entry_key".to_owned(), AttrValue::Str(format!("k{number}")));
        let paragraph = doc.alloc_child(bib, node(NodeKind::Paragraph, attrs));
        let mut text_attrs = AttrMap::new();
        text_attrs.insert("text".to_owned(), AttrValue::Str(text.to_owned()));
        doc.alloc_child(paragraph, node(NodeKind::Text, text_attrs));
    }

    #[test]
    fn entries_emit_numbered_markers_with_hanging_indent() {
        let mut doc = Document::new(PathBuf::from("test.mos"));
        pin_helvetica(&mut doc);
        let bib = alloc_bibliography(&mut doc);
        alloc_entry(&mut doc, bib, 1, "First Entry. 1990.");
        alloc_entry(&mut doc, bib, 2, "Second Entry. 1991.");

        let result = LayoutEngine::new().layout(&doc);

        assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
        let runs = &result.graph.pages[0].runs;
        let markers: Vec<&TextRun> = runs
            .iter()
            .filter(|r| r.text == "[1]" || r.text == "[2]")
            .collect();
        assert_eq!(markers.len(), 2, "expected [1] and [2], got {runs:?}");
        assert!(markers[0].baseline_from_top_pt < markers[1].baseline_from_top_pt);
        for marker in &markers {
            let width = text_width(marker.font, marker.size_pt, &marker.text);
            assert!(marker.x_pt >= MARGIN_PT - 0.5, "{runs:?}");
            assert!(marker.x_pt + width <= MARGIN_PT + LIST_MARKER_GUTTER_PT + 0.5);
        }
        let first = runs.iter().find(|r| r.text == "First").expect("entry text");
        let expected_left = MARGIN_PT + LIST_MARKER_GUTTER_PT;
        assert!((first.x_pt - expected_left).abs() < 0.5, "{runs:?}");
        assert!(
            (first.baseline_from_top_pt - markers[0].baseline_from_top_pt).abs() < 1e-3,
            "marker sits on the entry's first baseline"
        );
    }

    #[test]
    fn declaration_only_bibliography_emits_nothing() {
        let mut doc = Document::new(PathBuf::from("test.mos"));
        pin_helvetica(&mut doc);
        alloc_bibliography(&mut doc);

        let result = LayoutEngine::new().layout(&doc);

        assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
        assert!(
            result.graph.pages[0].runs.is_empty(),
            "a childless #bibliography declaration stays invisible: {:?}",
            result.graph.pages[0].runs
        );
    }

    #[test]
    fn long_entry_text_wraps_within_the_indented_column() {
        let mut doc = Document::new(PathBuf::from("test.mos"));
        pin_helvetica(&mut doc);
        let bib = alloc_bibliography(&mut doc);
        let long = "word ".repeat(60);
        alloc_entry(&mut doc, bib, 1, long.trim());

        let result = LayoutEngine::new().layout(&doc);

        let text_left = MARGIN_PT + LIST_MARKER_GUTTER_PT;
        let wrapped: Vec<&TextRun> = result.graph.pages[0]
            .runs
            .iter()
            .filter(|r| r.text != "[1]")
            .collect();
        assert!(wrapped.len() > 1, "long entry should wrap: {wrapped:?}");
        for run in wrapped {
            assert!(run.x_pt >= text_left - 0.5, "hanging indent holds: {run:?}");
        }
    }
}
