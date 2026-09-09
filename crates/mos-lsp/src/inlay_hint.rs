//! Display names for unnamed code blocks, projected from the cached document.

use std::collections::BTreeMap;

use mos_core::{AttrValue, Document, NodeKind};
use mos_eval::CODE_LANGUAGE_ATTR;
use serde_json::{Value, json};

use crate::diagnostics::{LspRange, byte_to_position};

/// Show a generated name immediately after each unnamed `#code` block's
/// closing delimiter in the requested range. Names are display metadata;
/// they never enter the label index or supply edits or runnable commands.
#[must_use]
pub fn code_block_hints(document: &Document, src: &str, range: LspRange) -> Vec<Value> {
    let start = (range.start.line, range.start.character);
    let end = (range.end.line, range.end.character);
    if start > end {
        return Vec::new();
    }
    let mut nodes: Vec<_> = document.nodes().filter(|node| {
        node.kind == NodeKind::Raw
            && matches!(node.attributes.get("raw.kind"), Some(AttrValue::Str(kind)) if kind == "code")
            && !node.attributes.contains_key("label")
    }).collect();
    nodes.sort_by_key(|node| node.span.start());
    let mut occurrences = BTreeMap::<String, usize>::new();
    let mut hints = Vec::new();
    for node in nodes {
        let Some(AttrValue::Str(text)) = node.attributes.get("text") else {
            continue;
        };
        if src.get(node.span.start()..node.span.end()).is_none() {
            continue;
        }
        let language = match node.attributes.get(CODE_LANGUAGE_ATTR) {
            Some(AttrValue::Str(language)) => display_text(language, 16),
            _ => String::new(),
        };
        let language = if language.is_empty() {
            "code"
        } else {
            &language
        };
        let preview = display_text(
            text.lines()
                .find(|line| !line.trim().is_empty())
                .unwrap_or(""),
            40,
        );
        let preview = if preview.is_empty() {
            "empty block"
        } else {
            &preview
        };
        let short_hash = node.content_hash().0 & u128::from(u32::MAX);
        let base = format!("{language}: {preview} [{short_hash:08x}]");
        // Count across the whole document before range filtering, so scrolling
        // does not rename duplicate blocks. Unrelated content never renumbers
        // these names; identical names get a document-order suffix.
        let occurrence = occurrences.entry(base.clone()).or_default();
        *occurrence += 1;
        let label = if *occurrence == 1 {
            base
        } else {
            format!("{base} #{}", *occurrence)
        };
        let position = byte_to_position(src, node.span.end());
        let point = (position.line, position.character);
        // A hint is an insertion point: include both boundary positions, even
        // when the closing delimiter is at EOF or the range is a single point.
        if start <= point && point <= end {
            hints.push(json!({
                "position": position,
                "label": label,
                "paddingLeft": true,
                "tooltip": "Generated code-block name. Add <label> after the closing delimiter to give this block a referenceable name.",
            }));
        }
    }
    hints
}

/// Keep user-authored language/body previews short and on one display line.
fn display_text(text: &str, limit: usize) -> String {
    let mut chars = text.trim().chars().filter(|c| !c.is_control());
    let mut display: String = chars.by_ref().take(limit).collect();
    if chars.next().is_some() {
        display.push('…');
    }
    display
}

#[cfg(test)]
mod tests;
