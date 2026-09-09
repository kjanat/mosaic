//! Authored semantic input snapshots, taken before resolution mutates nodes.

use std::collections::BTreeMap;

use mos_core::{AttrValue, ContentHasher, Document, NodeKind};

use crate::{ExternalDependency, LABEL_SPAN_END_ATTR, LABEL_SPAN_START_ATTR};

/// Must run before citation/label resolution: derived bibliography children
/// and rewritten reference/caption text are not authored semantic inputs.
pub(crate) fn stamp(document: &mut Document, dependencies: &[ExternalDependency]) {
    let files: BTreeMap<_, _> = dependencies
        .iter()
        .map(|dependency| {
            (
                dependency.path.as_path(),
                dependency
                    .fingerprint
                    .map(|fingerprint| fingerprint.content),
            )
        })
        .collect();
    document.update_content_hashes(|node| {
        let mut hasher = ContentHasher::new();
        hasher
            .field(b"mos-eval/authored-node/v1")
            .field(kind_tag(node.kind));
        // Before resolution, only label edit offsets and decoded image data
        // sit alongside authored attrs. Paths locate the recorded byte hash
        // below; machine-specific identities never enter the hash encoding.
        for (key, value) in &node.attributes {
            if matches!(
                key.as_str(),
                LABEL_SPAN_START_ATTR | LABEL_SPAN_END_ATTR | "resolved_path"
            ) || (node.kind == NodeKind::Image
                && matches!(
                    key.as_str(),
                    "pixels"
                        | "pixel_width"
                        | "pixel_height"
                        | "color_space"
                        | "bits_per_component"
                ))
                || (matches!(
                    node.kind,
                    NodeKind::Reference | NodeKind::PageReference | NodeKind::Citation
                ) && key == "text")
            {
                continue;
            }
            hasher.field(b"attr").field(key.as_bytes());
            hash_value(&mut hasher, value);
        }
        if matches!(node.kind, NodeKind::Image | NodeKind::Bibliography) {
            let path = match node.attributes.get("src") {
                Some(AttrValue::Str(src)) => {
                    mos_core::resolve_source_path(src, &node.span.file).ok()
                }
                _ => None,
            };
            hasher.field(b"external-input");
            match path.as_deref().and_then(|path| files.get(path)) {
                Some(Some(content)) => {
                    hasher.field(b"read").field(&content.0.to_le_bytes());
                }
                Some(None) => {
                    hasher.field(b"unreadable");
                }
                None => {
                    hasher.field(b"unloaded");
                }
            }
        }
        hasher.finish()
    });
}

// Explicit tags keep the encoding independent of Rust enum discriminants.
fn kind_tag(kind: NodeKind) -> &'static [u8] {
    match kind {
        NodeKind::Document => b"document",
        NodeKind::Section => b"section",
        NodeKind::Paragraph => b"paragraph",
        NodeKind::Text => b"text",
        NodeKind::Emphasis => b"emphasis",
        NodeKind::Strong => b"strong",
        NodeKind::BoldItalic => b"bold-italic",
        NodeKind::Math => b"math",
        NodeKind::Equation => b"equation",
        NodeKind::Figure => b"figure",
        NodeKind::Image => b"image",
        NodeKind::Table => b"table",
        NodeKind::Citation => b"citation",
        NodeKind::Reference => b"reference",
        NodeKind::PageReference => b"page-reference",
        NodeKind::Theorem => b"theorem",
        NodeKind::Footnote => b"footnote",
        NodeKind::Bibliography => b"bibliography",
        NodeKind::Raw => b"raw",
        NodeKind::List => b"list",
        NodeKind::ListItem => b"list-item",
        NodeKind::HardBreak => b"hard-break",
    }
}

fn hash_value(hasher: &mut ContentHasher, value: &AttrValue) {
    match value {
        AttrValue::Bool(value) => {
            hasher.field(b"bool").u32(u32::from(*value));
        }
        AttrValue::Int(value) => {
            hasher.field(b"int").field(&value.to_le_bytes());
        }
        AttrValue::Float(value) => {
            hasher
                .field(b"float")
                .field(&float_bits(*value).to_le_bytes());
        }
        AttrValue::Str(value) => {
            hasher.field(b"str").field(value.as_bytes());
        }
        AttrValue::List(values) => {
            hasher.field(b"list");
            for value in values {
                hash_value(hasher, value);
            }
            hasher.field(b"end-list");
        }
        AttrValue::Length(value) => {
            // Authored dimensions use the design note's 1/64-pt grid. Encode
            // the integral count as canonical f64 bits, avoiding a lossy cast.
            hasher
                .field(b"length")
                .field(&float_bits((value * 64.0).round()).to_le_bytes());
        }
        AttrValue::Bytes(value) => {
            hasher.field(b"bytes").field(value);
        }
    }
}

fn float_bits(value: f64) -> u64 {
    if value.is_nan() {
        // Pin the encoding, including sign/payload, across target platforms.
        0x7ff8_0000_0000_0000
    } else if value == 0.0 {
        0
    } else {
        value.to_bits()
    }
}

#[cfg(test)]
mod tests;
