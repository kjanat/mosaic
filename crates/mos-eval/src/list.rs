//! Lower parsed lists into semantic list nodes.

use std::collections::BTreeMap;

use mos_core::{AttrMap, AttrValue, Document, NodeId, NodeKind, NodeSpec, SourceSpan};
use mos_parse::{Item, ListItem};

use crate::inline::lower_inlines;

/// Allocate a [`NodeKind::List`] under `parent` and recursively lower
/// its [`ListItem`]s into [`NodeKind::ListItem`] children. The
/// `ordered` flag is preserved as a `Bool` attribute so layout can pick
/// the right marker style without re-walking the tree.
pub(super) fn lower_list(
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
    lower_inlines(doc, item_id, &item.inlines);
    for child in &item.children {
        if let Item::List {
            ordered,
            items,
            span,
        } = child
        {
            lower_list(doc, item_id, *ordered, items, span);
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

    use mos_core::{AttrValue, Node, NodeKind};

    use crate::lower;

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
            let text_child = item
                .children
                .iter()
                .filter_map(|id| r.document.get(*id))
                .find(|n| n.kind == NodeKind::Text)
                .expect("text child");
            assert_eq!(
                text_child.attributes.get("text"),
                Some(&AttrValue::Str(expected.to_owned()))
            );
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
}
