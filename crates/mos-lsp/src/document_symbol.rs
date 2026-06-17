//! `textDocument/documentSymbol` support for heading outlines.

use mos_core::{AttrValue, Document, Node, NodeKind};
use serde_json::{Value, json};

use crate::diagnostics::{LspRange, byte_to_position, span_to_range};

#[derive(Clone, Debug)]
struct SectionSymbol {
    name: String,
    level: u8,
    range: LspRange,
    selection_range: LspRange,
}

/// Build nested LSP `DocumentSymbol`s from heading/section nodes.
#[must_use]
pub(crate) fn document_symbols(document: &Document, src: &str) -> Vec<Value> {
    let sections = flat_sections(document, src);
    let mut index = 0;
    build_children(&sections, &mut index, 0)
}

fn flat_sections(document: &Document, src: &str) -> Vec<SectionSymbol> {
    let mut sections: Vec<(&Node, u8)> = document
        .nodes()
        .filter(|node| node.kind == NodeKind::Section)
        .map(|node| (node, section_level(node)))
        .collect();
    sections.sort_by_key(|(node, _)| node.span.start());

    sections
        .iter()
        .enumerate()
        .map(|(index, (node, level))| {
            let end = sections[index + 1..]
                .iter()
                .find(|(_, next_level)| next_level <= level)
                .map_or(src.len(), |(next, _)| next.span.start());
            let range = LspRange {
                start: byte_to_position(src, node.span.start()),
                end: byte_to_position(src, end.max(node.span.end())),
            };
            SectionSymbol {
                name: section_title(document, node),
                level: *level,
                range,
                selection_range: span_to_range(src, &node.span),
            }
        })
        .collect()
}

fn section_level(node: &Node) -> u8 {
    match node.attributes.get("level") {
        Some(AttrValue::Int(level)) => u8::try_from(*level).unwrap_or(u8::MAX).max(1),
        _ => 1,
    }
}

fn section_title(document: &Document, node: &Node) -> String {
    let mut title = String::new();
    for child_id in &node.children {
        if let Some(child) = document.get(*child_id)
            && let Some(AttrValue::Str(text)) = child.attributes.get("text")
        {
            title.push_str(text);
        }
    }
    let title = title.trim();
    if title.is_empty() {
        "Untitled section".to_owned()
    } else {
        title.to_owned()
    }
}

fn build_children(sections: &[SectionSymbol], index: &mut usize, parent_level: u8) -> Vec<Value> {
    let mut children = Vec::new();
    while let Some(section) = sections.get(*index) {
        if section.level <= parent_level {
            break;
        }
        *index += 1;
        let nested = build_children(sections, index, section.level);
        children.push(symbol_json(section, nested));
    }
    children
}

fn symbol_json(section: &SectionSymbol, children: Vec<Value>) -> Value {
    json!({
        "name": section.name,
        "kind": 3,
        "range": section.range,
        "selectionRange": section.selection_range,
        "children": children,
    })
}
