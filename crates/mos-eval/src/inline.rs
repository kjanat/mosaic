//! Lower inline parser nodes into semantic document children.

use std::collections::BTreeMap;

use mos_core::{AttrMap, AttrValue, Document, NodeId, NodeKind, NodeSpec};
use mos_parse::{Inline, InlineKind};

use crate::insert_label_attributes;

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
                // Stamp the label and its identifier `label_span` (issue #116)
                // exactly as declarations are, so rename reads the editable
                // identifier range directly instead of re-deriving it from the
                // reference node's span geometry. Pre-resolve placeholder text:
                // the resolver overwrites it on success; unresolved refs still
                // render visible `?label?` text.
                insert_label_attributes(&mut attributes, &inline.text, inline.label_span.as_ref());
                attributes.insert(
                    "text".to_owned(),
                    AttrValue::Str(format!("?{}?", inline.text)),
                );
            }
            InlineKind::PageReference => {
                // Same label + identifier-span stamp as a cross-reference
                // (issue #116). The `?label?` placeholder stays visible until
                // the resolve↔layout fixpoint (issue #72) rewrites it to the
                // target's page number; this slice models the node only.
                insert_label_attributes(&mut attributes, &inline.text, inline.label_span.as_ref());
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
            NodeSpec::new(kind, inline.span.clone()).with_attributes(attributes),
        );
    }
}
