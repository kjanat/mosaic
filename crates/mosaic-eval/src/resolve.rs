//! Cross-reference resolver (manifest §6 stage 3, MVP 1).
//!
//! Walks a lowered [`Document`] and, in two passes:
//!
//! 1. Assigns hierarchical `number` attributes to every [`NodeKind::Section`]
//!    (`"1"`, `"1.1"`, `"1.2"`, `"2"`), keyed off the existing `level`
//!    attribute.
//! 2. Builds a `label → NodeId` index from every block carrying a
//!    `label` attribute, then rewrites each [`NodeKind::Reference`]'s
//!    `text` attribute to its target's resolved string.
//!
//! Diagnostics:
//!
//! - `E041`: a label is declared more than once. The first occurrence
//!   wins; later occurrences keep their numbering but are not added to
//!   the index.
//! - `E042`: a `@label` reference targets a label that doesn't exist.
//!   The reference's text is left at its lowered placeholder
//!   (`?label?`) so it remains visible in the rendered output.
//!
//! Manifest §6 stage 3 calls for a fixpoint loop because later stages
//! (page references, TOC) can re-trigger resolution. MVP 1 only needs a
//! single pass — section numbering doesn't depend on layout — but the
//! driver shape mirrors the manifest's "internal fixpoint" anyway: the
//! loop runs until no rewrite changes the document, with a hard cap to
//! detect pathological cycles.

use std::collections::BTreeMap;

use mosaic_core::{
    AttrValue, Diagnostic, DiagnosticCode, Document, NodeId, NodeKind, Severity, SourceSpan,
};

/// Cap on resolver fixpoint iterations. MVP 1 always converges in one
/// pass; the cap is a safety net against forward-reference loops once
/// page numbering lands in MVP 3+.
const MAX_FIXPOINT_ITERATIONS: u32 = 8;

/// Run the resolver pass over `document` in place. Returns any
/// diagnostics produced; the document is modified regardless of whether
/// errors are present so partial output is still renderable.
pub fn resolve(document: &mut Document) -> Vec<Diagnostic> {
    let mut diagnostics: Vec<Diagnostic> = Vec::new();
    number_sections(document);
    let labels = build_label_index(document, &mut diagnostics);

    for _ in 0..MAX_FIXPOINT_ITERATIONS {
        let changed = rewrite_references(document, &labels, &mut diagnostics);
        if !changed {
            break;
        }
    }

    diagnostics
}

/// Walk the document depth-first and assign hierarchical numbers to
/// every section based on its `level` attribute. Sections without a
/// readable `level` default to depth 1.
fn number_sections(document: &mut Document) {
    let order = section_order(document);
    let mut counters: Vec<u32> = Vec::new();
    for (id, level) in order {
        let depth = usize::from(level.max(1));
        if depth > counters.len() {
            counters.resize(depth, 0);
        } else {
            counters.truncate(depth);
        }
        counters[depth - 1] += 1;
        let number = counters
            .iter()
            .map(u32::to_string)
            .collect::<Vec<_>>()
            .join(".");
        if let Some(node) = document.get_mut(id) {
            node.attributes
                .insert("number".to_owned(), AttrValue::Str(number));
        }
    }
}

fn section_order(document: &Document) -> Vec<(NodeId, u8)> {
    // MVP 1 only sees flat `Section` siblings under the document root,
    // but iterating via the children vector keeps this resilient if the
    // lowerer starts nesting sections later.
    let Some(root) = document.get(document.root) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for &child_id in &root.children {
        if let Some(child) = document.get(child_id)
            && child.kind == NodeKind::Section
        {
            let level = match child.attributes.get("level") {
                Some(AttrValue::Int(n)) => u8::try_from((*n).clamp(1, 255)).unwrap_or(1),
                _ => 1,
            };
            out.push((child_id, level));
        }
    }
    out
}

fn build_label_index(
    document: &Document,
    diagnostics: &mut Vec<Diagnostic>,
) -> BTreeMap<String, NodeId> {
    let mut index: BTreeMap<String, NodeId> = BTreeMap::new();
    for node in document.nodes() {
        // References *consume* labels; only blocks declare them.
        // Treating a `@ref`'s `label` attribute as a declaration would
        // shadow the real target.
        if node.kind == NodeKind::Reference {
            continue;
        }
        let Some(AttrValue::Str(label)) = node.attributes.get("label") else {
            continue;
        };
        if let Some(&existing) = index.get(label) {
            let existing_span = document.get(existing).map_or_else(
                || SourceSpan::placeholder(document.file.clone()),
                |n| n.span.clone(),
            );
            diagnostics.push(Diagnostic {
                severity: Severity::Error,
                code: DiagnosticCode("E041"),
                message: format!("label `{label}` is declared more than once"),
                span: Some(node.span.clone()),
                notes: vec![mosaic_core::DiagnosticNote {
                    message: format!("first declaration of `{label}` is here"),
                    span: Some(existing_span),
                }],
                suggestions: Vec::new(),
            });
            continue;
        }
        index.insert(label.clone(), node.id);
    }
    index
}

/// Rewrite each `Reference` node's `text` attribute to point at its
/// target. Returns true if any node was mutated this iteration —
/// callers use that signal to drive the §6 stage 3 fixpoint loop.
fn rewrite_references(
    document: &mut Document,
    labels: &BTreeMap<String, NodeId>,
    diagnostics: &mut Vec<Diagnostic>,
) -> bool {
    let references: Vec<NodeId> = document
        .nodes()
        .filter(|n| n.kind == NodeKind::Reference)
        .map(|n| n.id)
        .collect();

    let mut changed = false;
    for ref_id in references {
        let Some(node) = document.get(ref_id) else {
            continue;
        };
        let Some(AttrValue::Str(label)) = node.attributes.get("label").cloned() else {
            continue;
        };
        let resolved_text = if let Some(target_id) = labels.get(&label) {
            let target_number = document.get(*target_id).and_then(|t| {
                if let Some(AttrValue::Str(n)) = t.attributes.get("number") {
                    Some(n.clone())
                } else {
                    None
                }
            });
            target_number.unwrap_or_else(|| label.clone())
        } else {
            let already_diagnosed = diagnostics
                .iter()
                .any(|d| d.code.0 == "E042" && d.span.as_ref() == Some(&node.span));
            if !already_diagnosed {
                diagnostics.push(Diagnostic {
                    severity: Severity::Error,
                    code: DiagnosticCode("E042"),
                    message: format!("unknown label `{label}` in `@` reference"),
                    span: Some(node.span.clone()),
                    notes: Vec::new(),
                    suggestions: Vec::new(),
                });
            }
            continue;
        };

        if let Some(node) = document.get_mut(ref_id) {
            let prior = node.attributes.get("text").cloned();
            let new = AttrValue::Str(resolved_text);
            if !matches!(&prior, Some(p) if attr_eq(p, &new)) {
                node.attributes.insert("text".to_owned(), new);
                changed = true;
            }
        }
    }
    changed
}

fn attr_eq(a: &AttrValue, b: &AttrValue) -> bool {
    match (a, b) {
        (AttrValue::Str(x), AttrValue::Str(y)) => x == y,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::expect_used,
        reason = "tests panic loudly on setup failure"
    )]

    use std::path::PathBuf;

    use super::*;

    fn lower(src: &str) -> (Document, Vec<Diagnostic>) {
        let r = crate::lower(src, &PathBuf::from("test.mos"));
        (r.document, r.diagnostics)
    }

    fn section_numbers(doc: &Document) -> Vec<(String, String)> {
        doc.nodes()
            .filter(|n| n.kind == NodeKind::Section)
            .map(|n| {
                let title = n
                    .children
                    .iter()
                    .filter_map(|c| doc.get(*c))
                    .find_map(|c| match c.attributes.get("text") {
                        Some(AttrValue::Str(s)) => Some(s.clone()),
                        _ => None,
                    })
                    .unwrap_or_default();
                let number = match n.attributes.get("number") {
                    Some(AttrValue::Str(s)) => s.clone(),
                    _ => String::new(),
                };
                (title, number)
            })
            .collect()
    }

    #[test]
    fn assigns_hierarchical_section_numbers() {
        let (doc, diags) = lower("= Intro\n\n== Background\n\n== Aims\n\n= Methods\n\n== Sample\n");
        assert!(diags.is_empty(), "{diags:?}");
        let nums = section_numbers(&doc);
        let pairs: Vec<(&str, &str)> = nums.iter().map(|(t, n)| (t.as_str(), n.as_str())).collect();
        assert_eq!(
            pairs,
            vec![
                ("Intro", "1"),
                ("Background", "1.1"),
                ("Aims", "1.2"),
                ("Methods", "2"),
                ("Sample", "2.1"),
            ]
        );
    }

    #[test]
    fn duplicate_label_emits_e041_and_keeps_first() {
        let (doc, diags) = lower("= A <dup>\n\n= B <dup>\n\nsee @dup\n");
        assert!(
            diags.iter().any(|d| d.code.0 == "E041"),
            "expected E041, got {diags:?}"
        );
        // Reference still resolves to the first declaration's number.
        let r = doc
            .nodes()
            .find(|n| n.kind == NodeKind::Reference)
            .expect("reference");
        assert_eq!(
            r.attributes.get("text"),
            Some(&AttrValue::Str("1".to_owned()))
        );
    }

    #[test]
    fn unknown_label_emits_e042() {
        let (doc, diags) = lower("see @no:such\n");
        assert!(
            diags.iter().any(|d| d.code.0 == "E042"),
            "expected E042, got {diags:?}"
        );
        let r = doc
            .nodes()
            .find(|n| n.kind == NodeKind::Reference)
            .expect("reference");
        // Placeholder text is preserved so the diagnostic location is
        // visible in the rendered output.
        assert_eq!(
            r.attributes.get("text"),
            Some(&AttrValue::Str("?no:such?".to_owned()))
        );
    }

    #[test]
    fn reference_resolves_to_section_number() {
        let (doc, diags) =
            lower("= Intro <intro>\n\n= Methods <methods>\n\nsee @methods and @intro\n");
        assert!(diags.is_empty(), "{diags:?}");
        let refs: Vec<String> = doc
            .nodes()
            .filter(|n| n.kind == NodeKind::Reference)
            .filter_map(|n| match n.attributes.get("text") {
                Some(AttrValue::Str(s)) => Some(s.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(refs, vec!["2".to_owned(), "1".to_owned()]);
    }

    #[test]
    fn paragraph_label_indexes_paragraph() {
        // A paragraph-attached label has no section number, so the
        // resolver falls back to using the bare label as the rewritten
        // text. No E042 is emitted because the target exists.
        let (doc, diags) = lower("<note> a side note here\n\nsee @note\n");
        assert!(diags.is_empty(), "{diags:?}");
        let r = doc.nodes().find(|n| n.kind == NodeKind::Reference).unwrap();
        assert_eq!(
            r.attributes.get("text"),
            Some(&AttrValue::Str("note".to_owned()))
        );
    }

    #[test]
    fn level_three_numbers_correctly() {
        let (doc, diags) = lower("= A\n\n== B\n\n=== C\n\n== D\n\n= E\n");
        assert!(diags.is_empty(), "{diags:?}");
        let nums: Vec<String> = doc
            .nodes()
            .filter(|n| n.kind == NodeKind::Section)
            .filter_map(|n| match n.attributes.get("number") {
                Some(AttrValue::Str(s)) => Some(s.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(nums, vec!["1", "1.1", "1.1.1", "1.2", "2"]);
    }
}
