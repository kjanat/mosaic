//! Lower parsed lists into semantic list nodes.

use std::collections::BTreeMap;

use mos_core::{AttrMap, AttrValue, Document, NodeId, NodeKind, NodeSpec, SourceSpan};
use mos_parse::{ListItem, ListItemBlock};

use crate::inline::lower_inlines;

/// Allocate a [`NodeKind::List`] under `parent`.
///
/// The `ordered` flag is preserved as a `Bool` attribute so layout can pick
/// the right marker style without re-walking the tree.
pub fn lower(
    doc: &mut Document,
    parent: NodeId,
    ordered: bool,
    items: &[ListItem],
    span: &SourceSpan,
) {
    let mut attributes: AttrMap = BTreeMap::new();
    attributes.insert("ordered".to_owned(), AttrValue::Bool(ordered));
    let list_id = doc.alloc_child(
        parent,
        NodeSpec::new(NodeKind::List, span.clone()).with_attributes(attributes),
    );
    for item in items {
        lower_list_item(doc, list_id, item);
    }
}

fn lower_list_item(doc: &mut Document, parent: NodeId, item: &ListItem) {
    let item_id = doc.alloc_child(parent, NodeSpec::new(NodeKind::ListItem, item.span.clone()));
    if item.blocks.is_empty() {
        // Degenerate item (parser recovered without a marker line): fall
        // back to the first-paragraph mirror, which is equally empty but
        // keeps the item node consistent.
        lower_inlines(doc, item_id, &item.inlines);
        return;
    }
    for block in &item.blocks {
        match block {
            ListItemBlock::Paragraph { inlines, span } => {
                let paragraph =
                    doc.alloc_child(item_id, NodeSpec::new(NodeKind::Paragraph, span.clone()));
                lower_inlines(doc, paragraph, inlines);
            }
            ListItemBlock::List {
                ordered,
                items,
                span,
            } => lower(doc, item_id, *ordered, items, span),
        }
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

    use mos_core::{AttrValue, Document, Node, NodeKind};

    use crate::lower;

    fn first_text_child<'a>(document: &'a Document, node: &Node) -> Option<&'a str> {
        node.children
            .iter()
            .filter_map(|id| document.get(*id))
            .find(|child| child.kind == NodeKind::Text)
            .and_then(|child| match child.attributes.get("text") {
                Some(AttrValue::Str(text)) => Some(text.as_str()),
                _ => None,
            })
    }

    fn first_paragraph_child<'a>(document: &'a Document, node: &Node) -> Option<&'a Node> {
        node.children
            .iter()
            .filter_map(|id| document.get(*id))
            .find(|child| child.kind == NodeKind::Paragraph)
    }

    #[test]
    fn lowers_unordered_list() {
        let r = lower("- one\n- two\n", &PathBuf::from("test.mos"));
        assert!(!r.has_errors(), "{:?}", r.diagnostics);
        let list = r
            .document
            .nodes()
            .find(|n| n.kind == NodeKind::List)
            .expect("list node");
        assert_eq!(
            list.attributes.get("ordered"),
            Some(&AttrValue::Bool(false))
        );
        assert_eq!(list.children.len(), 2);
        let items: Vec<&Node> = list
            .children
            .iter()
            .filter_map(|id| r.document.get(*id))
            .collect();
        assert!(items.iter().all(|n| n.kind == NodeKind::ListItem));
        for (item, expected) in items.iter().zip(["one", "two"]) {
            let paragraph = first_paragraph_child(&r.document, item).expect("paragraph child");
            assert_eq!(first_text_child(&r.document, paragraph), Some(expected));
        }
    }

    #[test]
    fn lowers_ordered_list_flag() {
        let r = lower("1. a\n2. b\n", &PathBuf::from("test.mos"));
        assert!(!r.has_errors(), "{:?}", r.diagnostics);
        let list = r
            .document
            .nodes()
            .find(|n| n.kind == NodeKind::List)
            .expect("list node");
        assert_eq!(list.attributes.get("ordered"), Some(&AttrValue::Bool(true)));
    }

    #[test]
    fn lowers_nested_list_as_listitem_child() {
        let r = lower("- outer\n  - inner\n", &PathBuf::from("test.mos"));
        assert!(!r.has_errors(), "{:?}", r.diagnostics);
        let outer = r
            .document
            .nodes()
            .find(|n| n.kind == NodeKind::List)
            .expect("outer list");
        let outer_item = r.document.get(outer.children[0]).unwrap();
        let nested = outer_item
            .children
            .iter()
            .filter_map(|id| r.document.get(*id))
            .find(|n| n.kind == NodeKind::List)
            .expect("nested list");
        assert_eq!(nested.children.len(), 1);
        let nested_item = r.document.get(nested.children[0]).unwrap();
        assert_eq!(nested_item.kind, NodeKind::ListItem);
    }

    #[test]
    fn lowers_list_continuation_as_single_item_paragraph() {
        let r = lower("- first\n  second\n", &PathBuf::from("test.mos"));
        assert!(!r.has_errors(), "{:?}", r.diagnostics);
        let list = r
            .document
            .nodes()
            .find(|n| n.kind == NodeKind::List)
            .expect("list");
        let item = r.document.get(list.children[0]).expect("list item");
        let paragraph = first_paragraph_child(&r.document, item).expect("paragraph");
        assert_eq!(
            first_text_child(&r.document, paragraph),
            Some("first\n  second")
        );
    }

    #[test]
    fn lowers_hard_break_inside_list_paragraph() {
        let r = lower("- first\\\\\n  second\n", &PathBuf::from("test.mos"));
        assert!(!r.has_errors(), "{:?}", r.diagnostics);
        let list = r
            .document
            .nodes()
            .find(|n| n.kind == NodeKind::List)
            .expect("list");
        let item = r.document.get(list.children[0]).expect("list item");
        let paragraph = first_paragraph_child(&r.document, item).expect("paragraph");
        let kinds: Vec<NodeKind> = paragraph
            .children
            .iter()
            .filter_map(|id| r.document.get(*id))
            .map(|node| node.kind)
            .collect();
        assert_eq!(
            kinds,
            vec![NodeKind::Text, NodeKind::HardBreak, NodeKind::Text]
        );
    }

    #[test]
    fn lowers_parent_tail_after_nested_child_in_source_order() {
        let r = lower(
            "- parent\n  - child\n  parent tail\n",
            &PathBuf::from("test.mos"),
        );
        assert!(!r.has_errors(), "{:?}", r.diagnostics);
        let outer = r
            .document
            .nodes()
            .find(|n| n.kind == NodeKind::List)
            .expect("outer list");
        let item = r.document.get(outer.children[0]).expect("outer item");
        let kinds: Vec<NodeKind> = item
            .children
            .iter()
            .filter_map(|id| r.document.get(*id))
            .map(|node| node.kind)
            .collect();
        assert_eq!(
            kinds,
            vec![NodeKind::Paragraph, NodeKind::List, NodeKind::Paragraph]
        );
        let tail = r.document.get(item.children[2]).expect("tail paragraph");
        assert_eq!(first_text_child(&r.document, tail), Some("parent tail"));
    }
}
