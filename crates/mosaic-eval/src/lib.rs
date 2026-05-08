//! Expression and scripting evaluator (manifest §4, §25).
//!
//! In MVP 0 the "evaluator" is really a *lowerer*: it walks a
//! [`SyntaxTree`] from `mosaic-parse` and builds the typed semantic
//! [`Document`] graph from `mosaic-core` (manifest §6 stage 2).
//! Section hierarchy resolution, counters, and label binding (§6
//! stage 3) are deferred to MVP 1.

use std::collections::BTreeMap;

use mosaic_core::{
    AttrMap, AttrValue, Diagnostic, Document, Node, NodeId, NodeKind, Severity, StyleId,
};
use mosaic_parse::{Inline, InlineKind, Item, SyntaxTree};

/// Result of lowering a [`SyntaxTree`] into a [`Document`].
#[derive(Debug)]
pub struct LowerResult {
    pub document: Document,
    pub diagnostics: Vec<Diagnostic>,
}

impl LowerResult {
    #[must_use]
    pub fn has_errors(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|d| d.severity == Severity::Error)
    }
}

#[derive(Default, Debug)]
pub struct Evaluator;

impl Evaluator {
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// Lower `tree` into a semantic [`Document`].
    pub fn evaluate(&self, tree: &SyntaxTree) -> LowerResult {
        let mut document = Document::new(tree.file.clone());
        let diagnostics: Vec<Diagnostic> = Vec::new();
        let root = document.root;

        for item in &tree.items {
            match item {
                Item::Heading {
                    level,
                    inlines,
                    span,
                } => {
                    let mut attributes: AttrMap = BTreeMap::new();
                    attributes.insert("level".to_owned(), AttrValue::Int(i64::from(*level)));
                    let heading = document.alloc_child(
                        root,
                        Node {
                            id: NodeId::default(),
                            kind: NodeKind::Section,
                            span: span.clone(),
                            content_hash: Default::default(),
                            style_id: StyleId::default(),
                            children: Vec::new(),
                            attributes,
                        },
                    );
                    lower_inlines(&mut document, heading, inlines);
                }
                Item::Paragraph { inlines, span } => {
                    let para = document.alloc_child(
                        root,
                        Node {
                            id: NodeId::default(),
                            kind: NodeKind::Paragraph,
                            span: span.clone(),
                            content_hash: Default::default(),
                            style_id: StyleId::default(),
                            children: Vec::new(),
                            attributes: AttrMap::new(),
                        },
                    );
                    lower_inlines(&mut document, para, inlines);
                }
                Item::Set { name, span } => {
                    // `#set` is recorded but not interpreted at MVP 0.
                    // Stash it as a `Raw` node off the document root so
                    // later passes (MVP 1+) can pick it up.
                    let mut attributes: AttrMap = BTreeMap::new();
                    attributes.insert("set".to_owned(), AttrValue::Str(name.clone()));
                    document.alloc_child(
                        root,
                        Node {
                            id: NodeId::default(),
                            kind: NodeKind::Raw,
                            span: span.clone(),
                            content_hash: Default::default(),
                            style_id: StyleId::default(),
                            children: Vec::new(),
                            attributes,
                        },
                    );
                }
            }
        }

        LowerResult {
            document,
            diagnostics,
        }
    }
}

fn lower_inlines(doc: &mut Document, parent: NodeId, inlines: &[Inline]) {
    for inline in inlines {
        let kind = match inline.kind {
            InlineKind::Text => NodeKind::Text,
            InlineKind::Emphasis => NodeKind::Emphasis,
            InlineKind::Strong => NodeKind::Strong,
            InlineKind::Code => NodeKind::Raw,
        };
        let mut attributes: AttrMap = BTreeMap::new();
        attributes.insert("text".to_owned(), AttrValue::Str(inline.text.clone()));
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

/// Convenience: parse + lower in one step. Concatenates parse diagnostics
/// with lowering diagnostics so callers can render them uniformly.
pub fn lower(src: &str, file: &std::path::Path) -> LowerResult {
    let parse_result = mosaic_parse::parse(src, file);
    let mut diagnostics = parse_result.diagnostics;
    let lower = Evaluator::new().evaluate(&parse_result.tree);
    diagnostics.extend(lower.diagnostics);
    LowerResult {
        document: lower.document,
        diagnostics,
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use mosaic_core::NodeKind;

    use super::*;

    #[test]
    fn lowers_heading_and_paragraph() {
        let r = lower(
            "= Hello\n\nbody *italic* text\n",
            &PathBuf::from("test.mos"),
        );
        assert!(!r.has_errors());
        // Document root + Section + Paragraph + 1 Text inside Section
        // + 3 inline children of Paragraph (text/emphasis/text).
        assert_eq!(r.document.len(), 1 + 2 + 1 + 3);

        let kinds: Vec<NodeKind> = r.document.nodes().map(|n| n.kind).collect();
        assert_eq!(kinds[0], NodeKind::Document);
        assert!(kinds.contains(&NodeKind::Section));
        assert!(kinds.contains(&NodeKind::Paragraph));
        assert!(kinds.contains(&NodeKind::Emphasis));
    }

    #[test]
    fn lowers_set_block_as_raw() {
        let r = lower(
            "#set page(paper: \"A4\")\n\n= After\n",
            &PathBuf::from("test.mos"),
        );
        assert!(!r.has_errors());
        let kinds: Vec<NodeKind> = r.document.nodes().map(|n| n.kind).collect();
        assert!(kinds.contains(&NodeKind::Raw));
        assert!(kinds.contains(&NodeKind::Section));
    }

    #[test]
    fn root_owns_top_level_items() {
        let r = lower("= A\n\n= B\n\npara\n", &PathBuf::from("test.mos"));
        let root = r.document.get(r.document.root).unwrap();
        assert_eq!(root.children.len(), 3);
    }
}
