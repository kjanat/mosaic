//! Cross-reference resolver (manifest §6 stage 3, MVP 1).
//!
//! Walks a lowered [`Document`] and, in two passes:
//!
//! 1. Assigns hierarchical `number` attributes to every [`NodeKind::Section`]
//!    (`"1"`, `"1.1"`, `"1.2"`, `"2"`), keyed off the existing `level`
//!    attribute.
//! 2. Builds a `label → LabelTarget` index from every block carrying a
//!    `label` attribute, then rewrites each [`NodeKind::Reference`]'s
//!    `text` attribute to its target's resolved string.
//!
//! The label index is *typed*: each entry records what kind of thing
//! the label points at (section, figure, or generic block). Section
//! references render from the section's captured counter; figure
//! targets are recognised distinctly so future figure-numbering work
//! (issue #46) has a hook, but figure display text still falls back to
//! the bare label name for now. Generic targets (paragraphs, raw
//! blocks, …) also render as the bare label, matching prior behavior.
//!
//! Diagnostics:
//!
//! - `MOS0026`: a label is declared more than once. The first occurrence
//!   wins; later occurrences keep their numbering but are not added to
//!   the index.
//! - `MOS0027`: a `@label` reference targets a label that doesn't exist.
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

use mos_core::{
    AttrValue, Diagnostic, DiagnosticAnnotation, Document, NodeKind, SourceSpan, codes,
};

/// Cap on resolver fixpoint iterations. MVP 1 always converges in one
/// pass; the cap is a safety net against forward-reference loops once
/// page numbering lands in MVP 3+.
const MAX_FIXPOINT_ITERATIONS: u32 = 8;

/// What a label points at, captured at index-build time.
///
/// Each variant carries only the data needed to render the reference's
/// display text — references never re-traverse the document via the
/// target [`mos_core::NodeId`] once the index is built, so the resolver can stay
/// kind-aware without exposing a node-typed handle to callers.
///
/// Figure targets carry no extra data yet: figure numbering is pending
/// on issue #46. The variant exists so the resolver can plumb figure
/// labels through as a distinct kind today and the renderer can light
/// up once counters arrive.
#[derive(Clone, Debug, Eq, PartialEq)]
enum LabelTargetKind {
    /// Heading target with its resolved hierarchical number (e.g.
    /// `"1.2"`).
    Section { number: String },
    /// Captioned figure. Numbering is deferred to issue #46; until
    /// then references render as the bare label name.
    Figure,
    /// Anything else carrying a label (paragraph, raw block, image, …).
    Generic,
}

/// An entry in the label → target index.
///
/// `span` is the declaration site, retained so duplicate-label
/// diagnostics can still point a "first declared here" note at the
/// original occurrence without re-looking-up the node by id.
#[derive(Clone, Debug)]
struct LabelTarget {
    kind: LabelTargetKind,
    span: SourceSpan,
}

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

fn section_order(document: &Document) -> Vec<(mos_core::NodeId, u8)> {
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

/// Classify a labelled node into a [`LabelTargetKind`]. Only nodes
/// that actually declare a label reach this function — references are
/// filtered out by the caller.
fn classify_target(node: &mos_core::Node) -> LabelTargetKind {
    match node.kind {
        NodeKind::Section => {
            let number = match node.attributes.get("number") {
                Some(AttrValue::Str(s)) => s.clone(),
                _ => String::new(),
            };
            LabelTargetKind::Section { number }
        }
        NodeKind::Figure => LabelTargetKind::Figure,
        _ => LabelTargetKind::Generic,
    }
}

fn build_label_index(
    document: &Document,
    diagnostics: &mut Vec<Diagnostic>,
) -> BTreeMap<String, LabelTarget> {
    let mut index: BTreeMap<String, LabelTarget> = BTreeMap::new();
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
        if let Some(existing) = index.get(label) {
            diagnostics.push(
                Diagnostic::simple(
                    &codes::MOS0026,
                    None,
                    format!("label `{label}` is declared more than once"),
                )
                .with_span(node.span.clone())
                .with_annotation(DiagnosticAnnotation::Related {
                    span: existing.span.clone(),
                    message: format!("first declaration of `{label}` is here"),
                }),
            );
            continue;
        }
        index.insert(
            label.clone(),
            LabelTarget {
                kind: classify_target(node),
                span: node.span.clone(),
            },
        );
    }
    index
}

/// Compute the display string for a reference to `target`.
///
/// Section targets render as their captured counter (e.g. `"1.2"`).
/// Figure targets are recognised but have no counter yet (#46) and
/// fall back to the bare label name, matching the generic fallback
/// used for paragraphs and other unnumbered blocks.
fn render_target(target: &LabelTarget, label: &str) -> String {
    match &target.kind {
        LabelTargetKind::Section { number } if !number.is_empty() => number.clone(),
        // Section without a number is a lowerer bug, but fall back to
        // the label name so the rendered output stays readable.
        LabelTargetKind::Section { .. } | LabelTargetKind::Figure | LabelTargetKind::Generic => {
            label.to_owned()
        }
    }
}

/// Rewrite each `Reference` node's `text` attribute to point at its
/// target. Returns true if any node was mutated this iteration —
/// callers use that signal to drive the §6 stage 3 fixpoint loop.
fn rewrite_references(
    document: &mut Document,
    labels: &BTreeMap<String, LabelTarget>,
    diagnostics: &mut Vec<Diagnostic>,
) -> bool {
    let references: Vec<mos_core::NodeId> = document
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
        let resolved_text = if let Some(target) = labels.get(&label) {
            render_target(target, &label)
        } else {
            let already_diagnosed = diagnostics
                .iter()
                .any(|d| d.def().code() == codes::MOS0027.code() && d.span() == Some(&node.span));
            if !already_diagnosed {
                diagnostics.push(
                    Diagnostic::simple(
                        &codes::MOS0027,
                        None,
                        format!("unknown label `{label}` in `@` reference"),
                    )
                    .with_span(node.span.clone()),
                );
            }
            continue;
        };

        if let Some(node) = document.get_mut(ref_id) {
            let new = AttrValue::Str(resolved_text);
            if node.attributes.get("text") != Some(&new) {
                node.attributes.insert("text".to_owned(), new);
                changed = true;
            }
        }
    }
    changed
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use mos_core::Severity;

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
        let src = "= A <dup>\n\n= B <dup>\n\nsee @dup\n";
        let (doc, diags) = lower(src);
        let e041: Vec<&Diagnostic> = diags
            .iter()
            .filter(|d| d.def().code() == codes::MOS0026.code())
            .collect();
        assert_eq!(e041.len(), 1, "expected exactly one MOS0026, got {diags:?}");
        let d = e041[0];
        assert_eq!(d.def().code(), codes::MOS0026.code());
        assert_eq!(d.severity(), Severity::Error);
        assert!(
            d.message().contains("`dup`"),
            "MOS0026 message should name the duplicated label, got {:?}",
            d.message()
        );
        // The duplicate diagnostic must point at the *second* occurrence
        // and carry a Related annotation back to the first declaration.
        // Editor UIs rely on both spans to render the redeclaration jump.
        assert_eq!(
            d.span().map(|span| &src[span.start..span.end]),
            Some("= B <dup>"),
            "MOS0026 span should cover the second heading exactly"
        );
        assert_eq!(
            d.annotations().len(),
            1,
            "MOS0026 should reference the first decl"
        );
        let related = d.annotations().iter().find_map(|a| match a {
            DiagnosticAnnotation::Related { span, message } => Some((span, message)),
            _ => None,
        });
        assert!(related.is_some(), "MOS0026 carries a Related annotation");
        if let Some((note_span, note_message)) = related {
            assert_eq!(
                &src[note_span.start..note_span.end],
                "= A <dup>",
                "MOS0026 note should point at the original declaration exactly"
            );
            assert!(
                note_message.contains("`dup`"),
                "first-decl note should name the label, got {note_message:?}"
            );
        }
        // Reference still resolves to the first declaration's number.
        let reference_text = doc
            .nodes()
            .find(|n| n.kind == NodeKind::Reference)
            .and_then(|n| n.attributes.get("text"));
        assert_eq!(reference_text, Some(&AttrValue::Str("1".to_owned())));
    }

    #[test]
    fn triple_duplicate_label_emits_one_e041_per_redeclaration() {
        // Three sections share `dup`. The first wins; the second and
        // third each get their own MOS0026 pointing back at the first.
        // The reference still resolves to section number `1`.
        let src = "= A <dup>\n\n= B <dup>\n\n= C <dup>\n\nsee @dup\n";
        let (doc, diags) = lower(src);
        let e041: Vec<&Diagnostic> = diags
            .iter()
            .filter(|d| d.def().code() == codes::MOS0026.code())
            .collect();
        assert_eq!(
            e041.len(),
            2,
            "expected two MOS0026 (one per redeclaration), got {diags:?}"
        );
        let spans: Vec<&str> = e041
            .iter()
            .filter_map(|d| d.span().map(|s| &src[s.start..s.end]))
            .collect();
        assert_eq!(
            spans.len(),
            e041.len(),
            "every MOS0026 must carry a primary span"
        );
        assert!(
            spans.contains(&"= B <dup>"),
            "missing span for second decl, got {spans:?}"
        );
        assert!(
            spans.contains(&"= C <dup>"),
            "missing span for third decl, got {spans:?}"
        );
        // Every duplicate diagnostic must reference the same first decl.
        for d in &e041 {
            let related = d.annotations().iter().find_map(|a| match a {
                DiagnosticAnnotation::Related { span, message } => Some((span, message)),
                _ => None,
            });
            assert!(related.is_some(), "MOS0026 carries a Related annotation");
            if let Some((ns, _)) = related {
                assert_eq!(
                    &src[ns.start..ns.end],
                    "= A <dup>",
                    "every redeclaration must link back to the first decl"
                );
            }
        }
        let reference_text = doc
            .nodes()
            .find(|n| n.kind == NodeKind::Reference)
            .and_then(|n| n.attributes.get("text"));
        assert_eq!(reference_text, Some(&AttrValue::Str("1".to_owned())));
    }

    #[test]
    fn unknown_label_emits_e042() {
        let (doc, diags) = lower("see @no:such\n");
        let e042: Vec<&Diagnostic> = diags
            .iter()
            .filter(|d| d.def().code() == codes::MOS0027.code())
            .collect();
        assert_eq!(
            e042.len(),
            1,
            "expected exactly one MOS0027 even with the fixpoint loop, got {diags:?}"
        );
        let d = e042[0];
        assert_eq!(d.def().code(), codes::MOS0027.code());
        assert_eq!(d.severity(), Severity::Error);
        assert!(
            d.message().contains("`no:such`"),
            "MOS0027 message should name the missing label, got {:?}",
            d.message()
        );
        assert!(
            d.span().is_some(),
            "MOS0027 must carry a span so editors can jump to the bad reference"
        );
        let reference_text = doc
            .nodes()
            .find(|n| n.kind == NodeKind::Reference)
            .and_then(|n| n.attributes.get("text"));
        // Placeholder text is preserved so the diagnostic location is
        // visible in the rendered output.
        assert_eq!(
            reference_text,
            Some(&AttrValue::Str("?no:such?".to_owned()))
        );
    }

    #[test]
    fn multiple_unknown_references_each_emit_one_e042() {
        // Three distinct unknown labels in a single paragraph. The
        // fixpoint loop runs more than once (each iteration sees the
        // unresolved placeholders again), but every bad reference must
        // produce exactly one diagnostic — not one per iteration.
        let src = "see @alpha and @beta and @gamma\n";
        let (_doc, diags) = lower(src);
        let e042: Vec<&Diagnostic> = diags
            .iter()
            .filter(|d| d.def().code() == codes::MOS0027.code())
            .collect();
        assert_eq!(
            e042.len(),
            3,
            "expected one MOS0027 per unknown label, got {diags:?}"
        );
        let labels: std::collections::BTreeSet<&str> = e042
            .iter()
            .filter_map(|d| {
                // Each MOS0027's message is `unknown label `<name>` in `@` reference`.
                let msg = &d.message();
                let start = msg.find('`')? + 1;
                let end = start + msg[start..].find('`')?;
                Some(&msg[start..end])
            })
            .collect();
        assert_eq!(
            labels,
            ["alpha", "beta", "gamma"].into_iter().collect(),
            "each unknown label should appear exactly once"
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
        // text. No MOS0027 is emitted because the target exists.
        let (doc, diags) = lower("<note> a side note here\n\nsee @note\n");
        assert!(diags.is_empty(), "{diags:?}");
        let reference_text = doc
            .nodes()
            .find(|n| n.kind == NodeKind::Reference)
            .and_then(|n| n.attributes.get("text"));
        assert_eq!(reference_text, Some(&AttrValue::Str("note".to_owned())));
    }

    /// Build a synthetic node with `kind`, `label`, and (optionally) a
    /// section `number`. Used by classifier tests to exercise typed
    /// targets without dragging in image/file I/O.
    fn make_node(
        doc: &mut Document,
        kind: NodeKind,
        label: Option<&str>,
        number: Option<&str>,
    ) -> mos_core::NodeId {
        let mut attrs = mos_core::AttrMap::new();
        if let Some(l) = label {
            attrs.insert("label".to_owned(), AttrValue::Str(l.to_owned()));
        }
        if let Some(n) = number {
            attrs.insert("number".to_owned(), AttrValue::Str(n.to_owned()));
        }
        doc.alloc_child(
            doc.root,
            mos_core::Node {
                id: mos_core::NodeId::default(),
                kind,
                span: SourceSpan::placeholder(doc.file.clone()),
                content_hash: mos_core::ContentHash::default(),
                style_id: mos_core::StyleId::default(),
                children: Vec::new(),
                attributes: attrs,
            },
        )
    }

    #[test]
    fn classify_target_distinguishes_kinds() {
        let mut doc = Document::new(PathBuf::from("test.mos"));
        let section_id = make_node(&mut doc, NodeKind::Section, Some("sec"), Some("1.2"));
        let figure_id = make_node(&mut doc, NodeKind::Figure, Some("fig"), None);
        let paragraph_id = make_node(&mut doc, NodeKind::Paragraph, Some("p"), None);

        assert_eq!(
            doc.get(section_id).map(classify_target),
            Some(LabelTargetKind::Section {
                number: "1.2".to_owned()
            })
        );

        assert_eq!(
            doc.get(figure_id).map(classify_target),
            Some(LabelTargetKind::Figure)
        );

        assert_eq!(
            doc.get(paragraph_id).map(classify_target),
            Some(LabelTargetKind::Generic)
        );
    }

    #[test]
    fn figure_label_is_recognised_as_figure_target() {
        // Constructs a Figure node with a label and a Reference to it,
        // then runs the resolver directly. Verifies:
        //   - the figure label is found (no E042),
        //   - the reference's rewritten text falls back to the bare
        //     label name, matching the "figure numbering is pending
        //     #46" contract,
        //   - the label index records the target as `Figure`, not
        //     `Generic`.
        let mut doc = Document::new(PathBuf::from("test.mos"));
        let _figure = make_node(&mut doc, NodeKind::Figure, Some("fig:one"), None);
        let ref_id = doc.alloc_child(
            doc.root,
            mos_core::Node {
                id: mos_core::NodeId::default(),
                kind: NodeKind::Reference,
                span: SourceSpan::placeholder(doc.file.clone()),
                content_hash: mos_core::ContentHash::default(),
                style_id: mos_core::StyleId::default(),
                children: Vec::new(),
                attributes: {
                    let mut a = mos_core::AttrMap::new();
                    a.insert("label".to_owned(), AttrValue::Str("fig:one".to_owned()));
                    a.insert("text".to_owned(), AttrValue::Str("?fig:one?".to_owned()));
                    a
                },
            },
        );

        let diags = resolve(&mut doc);
        assert!(diags.is_empty(), "{diags:?}");

        let mut sink: Vec<Diagnostic> = Vec::new();
        let index = build_label_index(&doc, &mut sink);
        assert!(sink.is_empty(), "{sink:?}");
        assert_eq!(
            index.get("fig:one").map(|target| &target.kind),
            Some(&LabelTargetKind::Figure)
        );

        assert_eq!(
            doc.get(ref_id).and_then(|r| r.attributes.get("text")),
            Some(&AttrValue::Str("fig:one".to_owned())),
            "figure references render as the bare label until figure numbering lands (#46)"
        );
    }

    #[test]
    fn section_target_index_carries_resolved_number() {
        let (doc, diags) = lower("= Intro <intro>\n\n== Methods <methods>\n");
        assert!(diags.is_empty(), "{diags:?}");

        let mut sink: Vec<Diagnostic> = Vec::new();
        let index = build_label_index(&doc, &mut sink);
        assert!(sink.is_empty(), "{sink:?}");

        assert_eq!(
            index.get("intro").map(|t| &t.kind),
            Some(&LabelTargetKind::Section {
                number: "1".to_owned()
            })
        );
        assert_eq!(
            index.get("methods").map(|t| &t.kind),
            Some(&LabelTargetKind::Section {
                number: "1.1".to_owned()
            })
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
