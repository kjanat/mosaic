//! Lower inline parser nodes into semantic document children.

use std::collections::BTreeMap;

use mos_core::{AttrMap, AttrValue, Document, Node, NodeId, NodeKind, StyleId};
use mos_parse::{Inline, InlineKind};

pub(super) fn lower_inlines(doc: &mut Document, parent: NodeId, inlines: &[Inline]) {
    for inline in inlines {
        let kind = match inline.kind {
            InlineKind::Text => NodeKind::Text,
            InlineKind::Emphasis => NodeKind::Emphasis,
            InlineKind::Strong => NodeKind::Strong,
            InlineKind::BoldItalic => NodeKind::BoldItalic,
            InlineKind::Code => NodeKind::Raw,
            InlineKind::Reference => NodeKind::Reference,
            InlineKind::PageReference => NodeKind::PageReference,
            InlineKind::Citation => NodeKind::Citation,
            InlineKind::HardBreak => NodeKind::HardBreak,
        };
        let mut attributes: AttrMap = BTreeMap::new();
        match inline.kind {
            InlineKind::Reference => {
                // Pre-resolve placeholder text: resolver overwrites this on success;
                // unresolved refs still render visible `?label?` text.
                attributes.insert("label".to_owned(), AttrValue::Str(inline.text.clone()));
                attributes.insert(
                    "text".to_owned(),
                    AttrValue::Str(format!("?{}?", inline.text)),
                );
            }
            InlineKind::PageReference => {
                // Same placeholder shape as a cross-reference: the label is
                // recorded and `?label?` stays visible until the resolve↔layout
                // fixpoint (issue #72) rewrites it to the target's page number.
                // This slice models the node but does not resolve it.
                attributes.insert("label".to_owned(), AttrValue::Str(inline.text.clone()));
                attributes.insert(
                    "text".to_owned(),
                    AttrValue::Str(format!("?{}?", inline.text)),
                );
            }
            InlineKind::Citation => {
                // Record the bare key and a visible `[?key?]` fallback.
                // `resolve_citations` rewrites this to a numeric label
                // (`[1]`, ...) for keys found in a declared bibliography;
                // unresolved keys keep `[?key?]` so the citation stays
                // visible the same way unresolved refs are.
                attributes.insert("key".to_owned(), AttrValue::Str(inline.text.clone()));
                attributes.insert(
                    "text".to_owned(),
                    AttrValue::Str(format!("[?{}?]", inline.text)),
                );
            }
            // Hard breaks are pure structural markers -- no text payload
            // to lower, no attributes. Layout's `collect_words` matches
            // on `NodeKind::HardBreak` directly.
            InlineKind::HardBreak => {}
            _ => {
                attributes.insert("text".to_owned(), AttrValue::Str(inline.text.clone()));
            }
        }
        doc.alloc_child(
            parent,
            Node {
                id: NodeId::default(),
                kind,
                span: inline.span.clone(),
                content_hash: Default::default(),
                style_id: StyleId::default(),
                children: Vec::new(),
                attributes,
            },
        );
    }
}
