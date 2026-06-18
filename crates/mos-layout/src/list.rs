use mos_core::{AttrValue, Document, Node, NodeKind};
use mos_fonts::{Font, shape_with_fallback, text_width};

use crate::word::Word;
use crate::{LIST_MARKER_GUTTER_PT, LayoutState, PARA_SPACE_AFTER_PT, PendingMarker};

impl LayoutState {
    /// Lay out a [`NodeKind::List`] and its [`NodeKind::ListItem`]
    /// children with hanging indent.
    pub(super) fn layout_list(&mut self, document: &Document, list_node: &Node) {
        let ordered = matches!(
            list_node.attributes.get("ordered"),
            Some(AttrValue::Bool(true))
        );
        let regular = self.text.family.regular;
        let size = self.text.size_pt;
        let leading = self.text.leading;
        let saved_left = self.current_left_pt;
        let widest_marker_pt = if ordered {
            list_node
                .children
                .iter()
                .filter_map(|id| document.get(*id))
                .filter(|n| n.kind == NodeKind::ListItem)
                .enumerate()
                .map(|(idx, _)| {
                    shape_with_fallback(
                        regular,
                        self.text.family.fallbacks,
                        size,
                        &format!("{}.", idx + 1),
                    )
                    .iter()
                    .map(|s| s.advance_pt)
                    .sum()
                })
                .fold(0.0_f32, f32::max)
        } else {
            shape_with_fallback(regular, self.text.family.fallbacks, size, "\u{2022}")
                .iter()
                .map(|s| s.advance_pt)
                .sum()
        };
        let marker_gap_pt = text_width(regular, size, " ");
        let gutter = (widest_marker_pt + marker_gap_pt).max(LIST_MARKER_GUTTER_PT);
        let item_left = saved_left + gutter;

        let mut item_idx = 0_usize;
        for item_id in &list_node.children {
            let Some(item) = document.get(*item_id) else {
                continue;
            };
            if item.kind != NodeKind::ListItem {
                continue;
            }
            item_idx += 1;

            let marker_text = if ordered {
                format!("{item_idx}.")
            } else {
                "\u{2022}".to_owned()
            };
            let subruns =
                shape_with_fallback(regular, self.text.family.fallbacks, size, &marker_text);
            let width_pt: f32 = subruns.iter().map(|s| s.advance_pt).sum();
            let marker_word = Word {
                text: marker_text,
                actual_text: None,
                space_before_pt: 0.0,
                font: regular,
                size_pt: size,
                width_pt,
                subruns,
                shy_break_offsets: Vec::new(),
            };
            let marker_x = item_left - marker_gap_pt - marker_word.width_pt;

            self.current_left_pt = item_left;
            self.pending_marker = Some(PendingMarker {
                x_pt: marker_x,
                word: marker_word,
            });

            let has_block_children = item
                .children
                .iter()
                .filter_map(|id| document.get(*id))
                .any(|child| matches!(child.kind, NodeKind::Paragraph));
            if has_block_children {
                self.layout_list_item_blocks(document, item, regular, size, leading);
            } else {
                let words = self.collect_words(document, item, regular, size);
                if words.is_empty() {
                    self.flush_line(&[], leading);
                } else {
                    self.flow_words(&words, leading);
                    self.flush_marker_only_line_if_pending(leading);
                }

                for child_id in &item.children {
                    let Some(child) = document.get(*child_id) else {
                        continue;
                    };
                    if child.kind == NodeKind::List {
                        self.layout_list(document, child);
                    }
                }
            }
        }

        self.current_left_pt = saved_left;
        if (saved_left - self.page.margin).abs() < f32::EPSILON {
            self.cursor_y += PARA_SPACE_AFTER_PT;
        }
    }

    fn layout_list_item_blocks(
        &mut self,
        document: &Document,
        item: &Node,
        regular: Font,
        size: f32,
        leading: f32,
    ) {
        let mut saw_block = false;
        for child_id in &item.children {
            let Some(child) = document.get(*child_id) else {
                continue;
            };
            match child.kind {
                NodeKind::Paragraph => {
                    let words = self.collect_words(document, child, regular, size);
                    if words.is_empty() {
                        if self.pending_marker.is_some() {
                            self.flush_line(&[], leading);
                        }
                    } else {
                        self.flow_words(&words, leading);
                        self.flush_marker_only_line_if_pending(leading);
                    }
                    saw_block = true;
                }
                NodeKind::List => {
                    if self.pending_marker.is_some() {
                        self.flush_line(&[], leading);
                    }
                    self.layout_list(document, child);
                    saw_block = true;
                }
                _ => {}
            }
        }
        if !saw_block && self.pending_marker.is_some() {
            self.flush_line(&[], leading);
        }
    }

    fn flush_marker_only_line_if_pending(&mut self, leading: f32) {
        if self.pending_marker.is_some() {
            self.flush_line(&[], leading);
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::expect_used,
        reason = "tests panic loudly on setup failure; matches crate-wide test-module convention"
    )]

    use std::fmt::Write as _;
    use std::path::PathBuf;

    use mos_core::{AttrMap, NodeId, NodeSpec, SourceSpan};

    use crate::{A4_WIDTH_PT, LayoutEngine, MARGIN_PT, TextRun};

    use super::*;

    fn alloc_inline(doc: &mut Document, parent: NodeId, kind: NodeKind, text: &str) {
        let mut attrs = AttrMap::new();
        attrs.insert("text".to_owned(), AttrValue::Str(text.to_owned()));
        doc.alloc_child(parent, node(kind, attrs));
    }

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

    fn alloc_list(doc: &mut Document, parent: NodeId, ordered: bool) -> NodeId {
        let mut attrs = AttrMap::new();
        attrs.insert("ordered".to_owned(), AttrValue::Bool(ordered));
        doc.alloc_child(parent, node(NodeKind::List, attrs))
    }

    fn alloc_list_item(doc: &mut Document, parent: NodeId, text: &str) -> NodeId {
        let id = doc.alloc_child(parent, node(NodeKind::ListItem, AttrMap::new()));
        alloc_inline(doc, id, NodeKind::Text, text);
        id
    }

    fn alloc_list_item_paragraph(doc: &mut Document, parent: NodeId, text: &str) -> NodeId {
        let item = doc.alloc_child(parent, node(NodeKind::ListItem, AttrMap::new()));
        let paragraph = doc.alloc_child(item, node(NodeKind::Paragraph, AttrMap::new()));
        alloc_inline(doc, paragraph, NodeKind::Text, text);
        item
    }

    fn alloc_paragraph(doc: &mut Document, parent: NodeId, text: &str) -> NodeId {
        let paragraph = doc.alloc_child(parent, node(NodeKind::Paragraph, AttrMap::new()));
        alloc_inline(doc, paragraph, NodeKind::Text, text);
        paragraph
    }

    fn alloc_hard_break(doc: &mut Document, parent: NodeId) {
        doc.alloc_child(parent, node(NodeKind::HardBreak, AttrMap::new()));
    }

    #[test]
    fn unordered_list_emits_bullet_markers() {
        let mut doc = Document::new(PathBuf::from("test.mos"));
        pin_helvetica(&mut doc);
        let root = doc.root;
        let list = alloc_list(&mut doc, root, false);
        alloc_list_item(&mut doc, list, "alpha");
        alloc_list_item(&mut doc, list, "beta");

        let result = LayoutEngine::new().layout(&doc);

        assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
        let runs = &result.graph.pages[0].runs;
        let bullets: Vec<&TextRun> = runs.iter().filter(|r| r.text == "\u{2022}").collect();
        assert_eq!(bullets.len(), 2, "expected 2 bullets, got runs {runs:?}");
        for bullet in &bullets {
            let bullet_w = text_width(bullet.font, bullet.size_pt, &bullet.text);
            assert!(bullet.x_pt >= MARGIN_PT - 0.5);
            assert!(bullet.x_pt + bullet_w <= MARGIN_PT + LIST_MARKER_GUTTER_PT + 0.5);
        }
        let alpha = runs.iter().find(|r| r.text == "alpha").expect("alpha run");
        assert!(alpha.x_pt > bullets[0].x_pt);
    }

    #[test]
    fn ordered_list_numbers_from_one() {
        let mut doc = Document::new(PathBuf::from("test.mos"));
        pin_helvetica(&mut doc);
        let root = doc.root;
        let list = alloc_list(&mut doc, root, true);
        alloc_list_item(&mut doc, list, "first");
        alloc_list_item(&mut doc, list, "second");
        alloc_list_item(&mut doc, list, "third");

        let result = LayoutEngine::new().layout(&doc);

        let markers: Vec<&str> = result.graph.pages[0]
            .runs
            .iter()
            .filter(|r| {
                r.text.ends_with('.') && r.text.chars().next().is_some_and(|c| c.is_ascii_digit())
            })
            .map(|r| r.text.as_str())
            .collect();
        assert_eq!(markers, vec!["1.", "2.", "3."]);
    }

    #[test]
    fn list_item_text_indented_past_marker_gutter() {
        let mut doc = Document::new(PathBuf::from("test.mos"));
        pin_helvetica(&mut doc);
        let root = doc.root;
        let list = alloc_list(&mut doc, root, false);
        alloc_list_item(&mut doc, list, "hello");

        let result = LayoutEngine::new().layout(&doc);

        let hello = result.graph.pages[0]
            .runs
            .iter()
            .find(|r| r.text == "hello")
            .expect("hello run");
        let expected = MARGIN_PT + LIST_MARKER_GUTTER_PT;
        assert!((hello.x_pt - expected).abs() < 0.5);
    }

    #[test]
    fn nested_list_indents_one_more_level() {
        let mut doc = Document::new(PathBuf::from("test.mos"));
        pin_helvetica(&mut doc);
        let root = doc.root;
        let outer = alloc_list(&mut doc, root, false);
        let outer_item = alloc_list_item(&mut doc, outer, "outer");
        let inner = alloc_list(&mut doc, outer_item, false);
        alloc_list_item(&mut doc, inner, "inner");

        let result = LayoutEngine::new().layout(&doc);

        let runs = &result.graph.pages[0].runs;
        let bullets: Vec<&TextRun> = runs.iter().filter(|r| r.text == "\u{2022}").collect();
        assert_eq!(bullets.len(), 2);
        let inner = runs.iter().find(|r| r.text == "inner").unwrap();
        assert!((inner.x_pt - 2.0f32.mul_add(LIST_MARKER_GUTTER_PT, MARGIN_PT)).abs() < 0.5);
        assert!(bullets[1].x_pt > bullets[0].x_pt);
    }

    #[test]
    fn long_ordered_list_widens_gutter_so_markers_dont_overlap_text() {
        let mut doc = Document::new(PathBuf::from("test.mos"));
        pin_helvetica(&mut doc);
        let root = doc.root;
        let list = alloc_list(&mut doc, root, true);
        for i in 0..100 {
            alloc_list_item(&mut doc, list, &format!("item{i}"));
        }

        let result = LayoutEngine::new().layout(&doc);

        let all_runs: Vec<&TextRun> = result
            .graph
            .pages
            .iter()
            .flat_map(|p| p.runs.iter())
            .collect();
        let marker_100 = all_runs
            .iter()
            .find(|r| r.text == "100.")
            .expect("`100.` marker emitted");
        let text_99 = all_runs
            .iter()
            .find(|r| r.text == "item99")
            .expect("`item99` text emitted");
        let marker_right =
            marker_100.x_pt + text_width(marker_100.font, marker_100.size_pt, &marker_100.text);
        assert!(marker_right <= text_99.x_pt + 0.01);
    }

    #[test]
    fn marker_only_item_still_emits_marker() {
        let mut doc = Document::new(PathBuf::from("test.mos"));
        pin_helvetica(&mut doc);
        let root = doc.root;
        let list = alloc_list(&mut doc, root, false);
        doc.alloc_child(list, node(NodeKind::ListItem, AttrMap::new()));
        alloc_list_item(&mut doc, list, "second");

        let result = LayoutEngine::new().layout(&doc);

        let bullets: Vec<&TextRun> = result.graph.pages[0]
            .runs
            .iter()
            .filter(|r| r.text == "\u{2022}")
            .collect();
        assert_eq!(bullets.len(), 2);
        assert!(bullets[0].baseline_from_top_pt < bullets[1].baseline_from_top_pt);
    }

    #[test]
    fn hard_break_only_list_paragraph_does_not_leak_or_drop_marker() {
        let mut doc = Document::new(PathBuf::from("test.mos"));
        pin_helvetica(&mut doc);
        let root = doc.root;
        let list = alloc_list(&mut doc, root, false);

        let first_item = doc.alloc_child(list, node(NodeKind::ListItem, AttrMap::new()));
        let first_blank = doc.alloc_child(first_item, node(NodeKind::Paragraph, AttrMap::new()));
        alloc_hard_break(&mut doc, first_blank);
        alloc_hard_break(&mut doc, first_blank);
        let first_body = doc.alloc_child(first_item, node(NodeKind::Paragraph, AttrMap::new()));
        alloc_inline(&mut doc, first_body, NodeKind::Text, "body");

        let second_item = doc.alloc_child(list, node(NodeKind::ListItem, AttrMap::new()));
        let second_blank = doc.alloc_child(second_item, node(NodeKind::Paragraph, AttrMap::new()));
        alloc_hard_break(&mut doc, second_blank);

        let result = LayoutEngine::new().layout(&doc);

        let runs = &result.graph.pages[0].runs;
        let bullets: Vec<&TextRun> = runs.iter().filter(|r| r.text == "\u{2022}").collect();
        assert_eq!(bullets.len(), 2, "got {runs:?}");
        let body = runs.iter().find(|r| r.text == "body").expect("body run");
        assert!(bullets[0].baseline_from_top_pt < body.baseline_from_top_pt);
        assert!(bullets[1].baseline_from_top_pt > body.baseline_from_top_pt);
        for bullet in bullets {
            assert!(
                (bullet.baseline_from_top_pt - body.baseline_from_top_pt).abs() > 0.01,
                "marker leaked onto body line: {runs:?}"
            );
        }
    }

    #[test]
    fn item_with_only_nested_child_keeps_its_marker() {
        let mut doc = Document::new(PathBuf::from("test.mos"));
        pin_helvetica(&mut doc);
        let root = doc.root;
        let outer = alloc_list(&mut doc, root, false);
        let outer_item = doc.alloc_child(outer, node(NodeKind::ListItem, AttrMap::new()));
        let inner = alloc_list(&mut doc, outer_item, false);
        alloc_list_item(&mut doc, inner, "deep");

        let result = LayoutEngine::new().layout(&doc);

        let bullets: Vec<&TextRun> = result.graph.pages[0]
            .runs
            .iter()
            .filter(|r| r.text == "\u{2022}")
            .collect();
        assert_eq!(bullets.len(), 2);
        assert!(bullets[1].x_pt > bullets[0].x_pt);
        assert!(bullets[1].baseline_from_top_pt > bullets[0].baseline_from_top_pt);
    }

    #[test]
    fn list_marker_baseline_matches_first_line_of_item() {
        let mut doc = Document::new(PathBuf::from("test.mos"));
        pin_helvetica(&mut doc);
        let root = doc.root;
        let list = alloc_list(&mut doc, root, false);
        alloc_list_item(&mut doc, list, "one");

        let result = LayoutEngine::new().layout(&doc);

        let runs = &result.graph.pages[0].runs;
        let bullet = runs.iter().find(|r| r.text == "\u{2022}").unwrap();
        let one = runs.iter().find(|r| r.text == "one").unwrap();
        assert!((bullet.baseline_from_top_pt - one.baseline_from_top_pt).abs() < 1e-3);
    }

    #[test]
    fn list_text_wraps_within_indented_column() {
        let mut doc = Document::new(PathBuf::from("test.mos"));
        pin_helvetica(&mut doc);
        let root = doc.root;
        let list = alloc_list(&mut doc, root, false);
        let long = (0..40).fold(String::new(), |mut long, i| {
            let _ = write!(long, "word{i} ");
            long
        });
        alloc_list_item(&mut doc, list, long.trim());

        let result = LayoutEngine::new().layout(&doc);

        let text_left = MARGIN_PT + LIST_MARKER_GUTTER_PT;
        let text_right = A4_WIDTH_PT - MARGIN_PT;
        for run in result.graph.pages[0]
            .runs
            .iter()
            .filter(|r| r.text != "\u{2022}")
        {
            assert!(run.x_pt >= text_left - 0.5);
            let end = run.x_pt + text_width(run.font, run.size_pt, &run.text);
            assert!(end <= text_right + 1e-3);
        }
    }

    #[test]
    fn list_continuation_text_uses_soft_whitespace() {
        let mut doc = Document::new(PathBuf::from("test.mos"));
        pin_helvetica(&mut doc);
        let root = doc.root;
        let list = alloc_list(&mut doc, root, false);
        alloc_list_item_paragraph(&mut doc, list, "first\n  second");

        let result = LayoutEngine::new().layout(&doc);

        let runs = &result.graph.pages[0].runs;
        let first = runs.iter().find(|r| r.text == "first").expect("first run");
        let second = runs
            .iter()
            .find(|r| r.text == "second")
            .expect("second run");
        assert!((first.baseline_from_top_pt - second.baseline_from_top_pt).abs() < 1e-3);
        assert!(second.x_pt > first.x_pt);
    }

    #[test]
    fn direct_and_block_paragraph_list_items_have_same_layout() {
        fn positions(block_paragraph: bool) -> (f32, f32, f32, f32) {
            let mut doc = Document::new(PathBuf::from("test.mos"));
            pin_helvetica(&mut doc);
            let root = doc.root;
            let list = alloc_list(&mut doc, root, false);
            if block_paragraph {
                alloc_list_item_paragraph(&mut doc, list, "alpha");
            } else {
                alloc_list_item(&mut doc, list, "alpha");
            }

            let result = LayoutEngine::new().layout(&doc);
            let runs = &result.graph.pages[0].runs;
            let bullet = runs.iter().find(|r| r.text == "\u{2022}").expect("bullet");
            let text = runs.iter().find(|r| r.text == "alpha").expect("alpha");
            (
                bullet.x_pt,
                bullet.baseline_from_top_pt,
                text.x_pt,
                text.baseline_from_top_pt,
            )
        }

        let direct = positions(false);
        let block = positions(true);
        assert!((direct.0 - block.0).abs() < 0.01);
        assert!((direct.1 - block.1).abs() < 0.01);
        assert!((direct.2 - block.2).abs() < 0.01);
        assert!((direct.3 - block.3).abs() < 0.01);
    }

    #[test]
    fn list_hard_break_hangs_under_item_text() {
        let mut doc = Document::new(PathBuf::from("test.mos"));
        pin_helvetica(&mut doc);
        let root = doc.root;
        let list = alloc_list(&mut doc, root, false);
        let item = doc.alloc_child(list, node(NodeKind::ListItem, AttrMap::new()));
        let paragraph = doc.alloc_child(item, node(NodeKind::Paragraph, AttrMap::new()));
        alloc_inline(&mut doc, paragraph, NodeKind::Text, "first");
        alloc_hard_break(&mut doc, paragraph);
        alloc_inline(&mut doc, paragraph, NodeKind::Text, "second");

        let result = LayoutEngine::new().layout(&doc);

        let runs = &result.graph.pages[0].runs;
        let first = runs.iter().find(|r| r.text == "first").expect("first run");
        let second = runs
            .iter()
            .find(|r| r.text == "second")
            .expect("second run");
        let expected = MARGIN_PT + LIST_MARKER_GUTTER_PT;
        assert!((first.x_pt - expected).abs() < 0.5);
        assert!((second.x_pt - expected).abs() < 0.5);
        assert!(second.baseline_from_top_pt > first.baseline_from_top_pt);
    }

    #[test]
    fn parent_tail_after_nested_list_returns_to_parent_column() {
        let mut doc = Document::new(PathBuf::from("test.mos"));
        pin_helvetica(&mut doc);
        let root = doc.root;
        let outer = alloc_list(&mut doc, root, false);
        let item = doc.alloc_child(outer, node(NodeKind::ListItem, AttrMap::new()));
        alloc_paragraph(&mut doc, item, "parent");
        let inner = alloc_list(&mut doc, item, false);
        alloc_list_item_paragraph(&mut doc, inner, "child");
        alloc_paragraph(&mut doc, item, "tail");

        let result = LayoutEngine::new().layout(&doc);

        let runs = &result.graph.pages[0].runs;
        let parent = runs
            .iter()
            .find(|r| r.text == "parent")
            .expect("parent run");
        let child = runs.iter().find(|r| r.text == "child").expect("child run");
        let tail = runs.iter().find(|r| r.text == "tail").expect("tail run");
        let parent_left = MARGIN_PT + LIST_MARKER_GUTTER_PT;
        let child_left = 2.0f32.mul_add(LIST_MARKER_GUTTER_PT, MARGIN_PT);
        assert!((parent.x_pt - parent_left).abs() < 0.5);
        assert!((tail.x_pt - parent_left).abs() < 0.5);
        assert!((child.x_pt - child_left).abs() < 0.5);
        assert!(tail.baseline_from_top_pt > child.baseline_from_top_pt);
    }
}
