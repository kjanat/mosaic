//! Cross-reference resolver (manifest §6 stage 3, MVP 1).
//!
//! Walks a lowered [`Document`] and, in three passes:
//!
//! 1. Assigns hierarchical `number` attributes to every [`NodeKind::Section`]
//!    (`"1"`, `"1.1"`, `"1.2"`, `"2"`), keyed off the existing `level`
//!    attribute.
//! 2. Assigns flat document-order `number` attributes to every numbered
//!    [`NodeKind::Figure`] (`"1"`, `"2"`, `"3"`) and stamps a visible
//!    `"{supplement} N: …"` label onto each captioned figure. Figures are
//!    not hierarchical, so the counter never resets. A figure can opt out
//!    with `numbered: false` (skipped: no number, no caption prefix, does
//!    not advance the counter) or swap its supplement word with
//!    `supplement: "…"` (issue #76).
//! 3. Builds a `label → LabelTarget` index from every block carrying a
//!    `label` attribute, then rewrites each [`NodeKind::Reference`]'s
//!    `text` attribute to its target's resolved string.
//!
//! The label index is *typed*: each entry records what kind of thing
//! the label points at (section, figure, or generic block). A section
//! reference renders as its bare hierarchical number (`"1.2"`); a figure
//! reference renders kind-aware as `"{supplement} {n}"` (`"Figure 1"` by
//! default) from the figure's flat document-order number. Generic targets
//! (paragraphs, raw blocks, images, skipped figures, …) carry no counter
//! and render as the bare label, matching prior behavior.
//!
//! Diagnostics:
//!
//! - `MOS0030`: a label is declared more than once. The first occurrence
//!   wins; later occurrences keep their numbering but are not added to
//!   the index. Each duplicate also carries a structured rename
//!   [`Suggestion`] — the next free `{label}-N` (`N >= 2`) that no other
//!   declaration or earlier suggestion already uses — over the duplicate
//!   label token span.
//! - `MOS0033`: a `@label` reference targets a label that doesn't exist.
//!   The reference's text is left at its lowered placeholder
//!   (`?label?`) so it remains visible in the rendered output.
//!
//! Manifest §6 stage 3 calls for a fixpoint loop because later stages
//! (page references, TOC) can re-trigger resolution. MVP 1 only needs a
//! single pass — section numbering doesn't depend on layout — but the
//! driver shape mirrors the manifest's "internal fixpoint" anyway: the
//! loop runs until no rewrite changes the document, with a hard cap to
//! detect pathological cycles.
//!
//! Every pass is **idempotent**: `resolve` is public and re-entrant, so
//! running it twice — inside the fixpoint above, or from a future
//! page-reference stage — must reproduce the same document rather than
//! compounding edits. Numbering overwrites attributes with the same
//! value; caption labelling re-derives from a preserved source instead
//! of re-reading the already-stamped text (which would nest the label
//! into `"Figure 1: Figure 1: …"`).

use std::collections::{BTreeMap, BTreeSet};

use mos_core::{
    AttrValue, Diagnostic, DiagnosticAnnotation, Document, NodeKind, SourceSpan, Suggestion, codes,
};

use crate::{LABEL_SPAN_END_ATTR, LABEL_SPAN_START_ATTR};

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
#[derive(Clone, Debug, Eq, PartialEq)]
enum LabelTargetKind {
    /// Heading target with its resolved hierarchical number (e.g.
    /// `"1.2"`).
    Section { number: String },
    /// Captioned figure with its resolved flat document-order number
    /// (e.g. `"3"`) and supplement word (`"Figure"` by default, or a
    /// custom `#figure(supplement: …)`). References render kind-aware as
    /// `"{supplement} {number}"` (e.g. `"Figure 3"`, `"Plate 3"`). A
    /// skipped (`numbered: false`) figure carries an empty number and
    /// renders as its bare label instead.
    Figure { number: String, supplement: String },
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
pub fn resolve(document: &mut Document, bib_keys: &BTreeSet<String>) -> Vec<Diagnostic> {
    let mut diagnostics: Vec<Diagnostic> = Vec::new();
    number_sections(document);
    number_figures(document);
    let labels = build_label_index(document, &mut diagnostics);
    validate_page_references(document, &labels, &mut diagnostics);

    for _ in 0..MAX_FIXPOINT_ITERATIONS {
        let changed = rewrite_references(document, &labels, bib_keys, &mut diagnostics);
        if !changed {
            break;
        }
    }

    diagnostics
}

/// Report an undeclared label in a `@page(label)` reference as `MOS0033`,
/// mirroring the `@label` cross-reference check. A page reference resolves to a
/// page *number* later, through the layout fixpoint (issue #72), but an unknown
/// *label* is a lower-time error exactly like a bad `@ref` — and catching it
/// here means `mos check` reports it without needing to lay the document out.
fn validate_page_references(
    document: &Document,
    labels: &BTreeMap<String, LabelTarget>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for node in document
        .nodes()
        .filter(|node| node.kind == NodeKind::PageReference)
    {
        let Some(AttrValue::Str(label)) = node.attributes.get("label") else {
            continue;
        };
        if labels.contains_key(label) {
            continue;
        }
        let mut diagnostic = Diagnostic::simple(
            &codes::MOS0033,
            None,
            format!("unknown label `{label}` in `@page` reference"),
        )
        .with_span(node.span.clone());
        if let Some(candidate) = nearest_label(label, labels) {
            diagnostic = diagnostic.with_suggestion(Suggestion::new(
                node.span.clone(),
                format!("@page({candidate})"),
            ));
        }
        diagnostics.push(diagnostic);
    }
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
    // Scan every `Section` in document order via the shared
    // `nodes_of_kind` collector (the same traversal figure numbering
    // uses). MVP 1 only emits flat sections under the root, but walking
    // the whole arena means nested sections would still be numbered in
    // order if the lowerer ever produced them.
    nodes_of_kind(document, NodeKind::Section)
        .into_iter()
        .map(|id| {
            let level = match document.get(id).and_then(|n| n.attributes.get("level")) {
                Some(AttrValue::Int(n)) => u8::try_from((*n).clamp(1, 255)).unwrap_or(1),
                _ => 1,
            };
            (id, level)
        })
        .collect()
}

/// Assign flat, document-order numbers to every figure (`"1"`, `"2"`,
/// `"3"`, …) and stamp a visible `"Figure N: …"` label onto each
/// captioned figure. Figures are not hierarchical, so the counter never
/// resets.
///
/// The label is baked into the caption text here — rather than rendered
/// by the layout engine the way section numbers are — so a numbered
/// figure shows its number with no backend changes; distinct label
/// *styling* is left to the future float/caption pass. The supplement
/// word comes from [`figure_supplement`] (the single localization seam)
/// and is joined to the number with a non-breaking space (U+00A0). That
/// space is *semantic generated text*, not layout policy in disguise: it
/// encodes `Figure` and its counter as one cohesive label token — the
/// same non-breaking space an author could type by hand — which the
/// layout engine merely honors. The resolver makes no wrapping decision
/// of its own; it just emits the token.
///
/// The pass is **idempotent**: the pre-label caption is preserved under a
/// `caption_source` attribute and the visible `text` is always re-derived
/// from it. Re-running the resolver — as the §6 stage 3 fixpoint and any
/// future page-reference pass do — therefore re-stamps the same label
/// instead of nesting `"Figure 1: Figure 1: …"`, and stays correct when a
/// figure is re-numbered, because the source never carries a stale counter.
fn number_figures(document: &mut Document) {
    // Counter advances only for numbered figures, so `#figure(numbered:
    // false)` figures neither consume a number nor leave a gap — the
    // numbered figures stay contiguous (1, 2, 3, …). This is the documented
    // skip rule (issue #76).
    let mut counter: usize = 0;
    for figure_id in nodes_of_kind(document, NodeKind::Figure) {
        // Read the per-figure controls before taking a `get_mut` borrow.
        let Some((numbered, supplement)) = document
            .get(figure_id)
            .map(|node| (figure_is_numbered(node), figure_supplement_attr(node)))
        else {
            continue;
        };
        // Resolve the caption's *source* text before mutating: `get`
        // borrows the document immutably, but the writes below need
        // `get_mut`. Prefer the preserved `caption_source`; fall back to
        // the live `text` only on the first pass, before any label has
        // been stamped. Re-deriving the label from this stable source —
        // never from the already-stamped `text` — is what keeps `resolve`
        // idempotent across reruns.
        let caption = figure_caption_text(document, figure_id).and_then(|text_id| {
            read_str_attr(document, text_id, "caption_source")
                .or_else(|| read_str_attr(document, text_id, "text"))
                .map(|source| (text_id, source))
        });

        if numbered {
            counter += 1;
            let number = counter.to_string();
            if let Some(node) = document.get_mut(figure_id) {
                node.attributes
                    .insert("number".to_owned(), AttrValue::Str(number.clone()));
            }
            if let Some((text_id, caption_source)) = caption {
                let labelled = format!(
                    "{}: {caption_source}",
                    figure_label_prefix(&supplement, &number)
                );
                if let Some(node) = document.get_mut(text_id) {
                    // Stash the pre-label caption so later passes re-derive
                    // the label from the original instead of the stamped text.
                    node.attributes
                        .insert("caption_source".to_owned(), AttrValue::Str(caption_source));
                    node.attributes
                        .insert("text".to_owned(), AttrValue::Str(labelled));
                }
            }
        } else {
            // Skipped figure: carry no number, and restore the caption to its
            // unprefixed source. Restoring (rather than just not stamping)
            // keeps the pass idempotent if a figure toggles numbered→skipped
            // across reruns, undoing any previously stamped `Figure N:`.
            if let Some(node) = document.get_mut(figure_id) {
                node.attributes.remove("number");
            }
            if let Some((text_id, caption_source)) = caption
                && let Some(node) = document.get_mut(text_id)
            {
                node.attributes.insert(
                    "caption_source".to_owned(),
                    AttrValue::Str(caption_source.clone()),
                );
                node.attributes
                    .insert("text".to_owned(), AttrValue::Str(caption_source));
            }
        }
    }
}

/// Collect the ids of every node of `kind` in document order. `nodes()`
/// iterates the arena by ascending [`mos_core::NodeId`] (allocation
/// order), and the lowerer allocates nodes in source order, so the
/// result is stable document order regardless of nesting depth. Shared
/// by figure numbering and [`section_order`] so both passes agree on
/// what "document order" means.
fn nodes_of_kind(document: &Document, kind: NodeKind) -> Vec<mos_core::NodeId> {
    document
        .nodes()
        .filter(|node| node.kind == kind)
        .map(|node| node.id)
        .collect()
}

/// Find the text node of a figure's caption, if it has one. The lowerer
/// tags the caption paragraph with `role = "caption"` and gives it a
/// single [`NodeKind::Text`] child carrying the caption string.
fn figure_caption_text(
    document: &Document,
    figure_id: mos_core::NodeId,
) -> Option<mos_core::NodeId> {
    let figure = document.get(figure_id)?;
    for &child_id in &figure.children {
        let Some(child) = document.get(child_id) else {
            continue;
        };
        let is_caption = child.kind == NodeKind::Paragraph
            && matches!(child.attributes.get("role"), Some(AttrValue::Str(role)) if role == "caption");
        if !is_caption {
            continue;
        }
        for &grandchild_id in &child.children {
            if document
                .get(grandchild_id)
                .is_some_and(|gc| gc.kind == NodeKind::Text)
            {
                return Some(grandchild_id);
            }
        }
    }
    None
}

/// Read a string attribute off a node by id, cloning it out. `None` if
/// the node is missing or the attribute is absent or non-string.
fn read_str_attr(document: &Document, id: mos_core::NodeId, key: &str) -> Option<String> {
    match document.get(id)?.attributes.get(key) {
        Some(AttrValue::Str(s)) => Some(s.clone()),
        _ => None,
    }
}

/// The human-facing *supplement* word prefixed to a figure's number in
/// generated reference and caption text — the "Figure" in "Figure 1".
///
/// This is the single localization seam for figure labels: LaTeX
/// localizes it through babel's `\figurename`, Typst through
/// `figure.supplement` under the document `text(lang: …)`. Mosaic
/// captures a document `language` in metadata but does not yet thread it
/// into the resolver, so this returns the English default; when that
/// plumbing lands, a language-keyed lookup replaces the constant here
/// without touching any call site. Sibling kinds (tables, equations,
/// theorems) grow their own supplements alongside their numbering.
fn figure_supplement() -> &'static str {
    "Figure"
}

/// Whether a figure participates in the auto `Figure N` counter. A figure
/// opts out with `#figure(numbered: false)` (issue #76), recorded by the
/// lowerer as a `numbered = false` attribute; absence means numbered.
fn figure_is_numbered(node: &mos_core::Node) -> bool {
    !matches!(
        node.attributes.get("numbered"),
        Some(AttrValue::Bool(false))
    )
}

/// The supplement word for a figure's caption and its references. An
/// explicit `#figure(supplement: …)` value wins — **including the empty
/// string** (`supplement: ""` / `supplement: none`), which means "number
/// only, no word" (the "no visible prefix" form). Only an *absent*
/// supplement falls back to the localized [`figure_supplement`] default
/// (`"Figure"`).
fn figure_supplement_attr(node: &mos_core::Node) -> String {
    match node.attributes.get("supplement") {
        Some(AttrValue::Str(s)) => s.clone(),
        _ => figure_supplement().to_owned(),
    }
}

/// Join a figure's supplement word and number into the cohesive label
/// token used in both captions and references — `"Figure\u{00A0}1"`,
/// non-breaking so the word never wraps off its number. An empty
/// supplement renders the number alone (`"1"`), with no word and no
/// leading space.
fn figure_label_prefix(supplement: &str, number: &str) -> String {
    if supplement.is_empty() {
        number.to_owned()
    } else {
        format!("{supplement}\u{00A0}{number}")
    }
}

/// Read a node's resolved `number` attribute, or an empty string if it
/// has none. Both section and figure numbering stash their counter
/// there before the label index is built; an empty result means the
/// numbering pass didn't reach the node (a resolver/lowerer bug).
fn captured_number(node: &mos_core::Node) -> String {
    match node.attributes.get("number") {
        Some(AttrValue::Str(s)) => s.clone(),
        _ => String::new(),
    }
}

/// Classify a labelled node into a [`LabelTargetKind`]. Only nodes
/// that actually declare a label reach this function — references are
/// filtered out by the caller.
fn classify_target(node: &mos_core::Node) -> LabelTargetKind {
    match node.kind {
        NodeKind::Section => LabelTargetKind::Section {
            number: captured_number(node),
        },
        NodeKind::Figure => LabelTargetKind::Figure {
            number: captured_number(node),
            supplement: figure_supplement_attr(node),
        },
        _ => LabelTargetKind::Generic,
    }
}

/// Collect every label declared anywhere in the document — any non-reference
/// block carrying a `label` attribute — regardless of document order or
/// duplication. The duplicate-rename suggestion consults this set so it never
/// proposes a name that some other declaration already uses.
fn declared_labels(document: &Document) -> BTreeSet<String> {
    document
        .nodes()
        .filter(|node| !matches!(node.kind, NodeKind::Reference | NodeKind::PageReference))
        .filter_map(|node| match node.attributes.get("label") {
            Some(AttrValue::Str(label)) => Some(label.clone()),
            _ => None,
        })
        .collect()
}

/// Pick a deterministic, collision-aware rename for a duplicated `label`: the
/// smallest integer suffix `N >= 2` whose `{label}-{N}` is not already in
/// `declared`. Boring and stable — no similarity ranking — but it steps over
/// existing labels so the suggested fix never re-creates the clash it
/// resolves. Among the first `declared.len() + 1` candidates at least one is
/// free (pigeonhole), so the bounded search always yields a name.
fn nonconflicting_rename(label: &str, declared: &BTreeSet<String>) -> String {
    let ceiling = declared.len().saturating_add(2);
    (2..=ceiling)
        .map(|n| format!("{label}-{n}"))
        .find(|candidate| !declared.contains(candidate))
        .unwrap_or_else(|| format!("{label}-{ceiling}"))
}

/// Build the `label -> LabelTarget` index from every label-declaring block,
/// reporting `MOS0030` for redeclarations. The first declaration of a label
/// wins; later occurrences keep their numbering but are not indexed, and each
/// carries a related note pointing at the first declaration plus a structured
/// rename [`Suggestion`] — the next free `{label}-N` — over the duplicate label
/// token span (see the module-level docs). Reads the document only, so
/// `resolve` stays idempotent.
fn build_label_index(
    document: &Document,
    diagnostics: &mut Vec<Diagnostic>,
) -> BTreeMap<String, LabelTarget> {
    let mut occupied_labels = declared_labels(document);
    let mut index: BTreeMap<String, LabelTarget> = BTreeMap::new();
    for node in document.nodes() {
        // References *consume* labels; only blocks declare them. Treating a
        // `@ref` or `@page(ref)`'s `label` attribute as a declaration would
        // shadow the real target (and falsely trip the duplicate-label check).
        if matches!(node.kind, NodeKind::Reference | NodeKind::PageReference) {
            continue;
        }
        let Some(AttrValue::Str(label)) = node.attributes.get("label") else {
            continue;
        };
        if let Some(existing) = index.get(label) {
            // Offer a deterministic, collision-aware rename for the duplicate:
            // the next free `{label}-N` that no declaration, or earlier
            // suggestion in this pass, already uses. Still a boring stable
            // rule, not a similarity-ranked guess. The fix targets only the
            // duplicate label token span so applying it preserves the
            // surrounding heading/directive syntax.
            let rename = nonconflicting_rename(label, &occupied_labels);
            occupied_labels.insert(rename.clone());
            let suggestion = label_span(node).map(|span| Suggestion::new(span, rename));
            let mut diagnostic = Diagnostic::simple(
                &codes::MOS0030,
                None,
                format!("label `{label}` is declared more than once"),
            )
            .with_span(node.span.clone())
            .with_annotation(DiagnosticAnnotation::Related {
                span: existing.span.clone(),
                message: format!("first declaration of `{label}` is here"),
            });
            if let Some(suggestion) = suggestion {
                diagnostic = diagnostic.with_suggestion(suggestion);
            }
            diagnostics.push(diagnostic);
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

fn label_span(node: &mos_core::Node) -> Option<SourceSpan> {
    let start = match node.attributes.get(LABEL_SPAN_START_ATTR) {
        Some(AttrValue::Int(value)) => usize::try_from(*value).ok()?,
        _ => return None,
    };
    let end = match node.attributes.get(LABEL_SPAN_END_ATTR) {
        Some(AttrValue::Int(value)) => usize::try_from(*value).ok()?,
        _ => return None,
    };
    if start > end {
        return None;
    }
    Some(SourceSpan::new(node.span.file.clone(), start, end))
}

/// Compute the display string for a reference to `target`.
///
/// Section targets render as their bare hierarchical counter (e.g.
/// `"1.2"`). Figure targets render kind-aware as `"Figure N"` — the
/// localized [`figure_supplement`] joined to the figure's flat
/// document-order counter with a non-breaking space (U+00A0): one
/// cohesive label token the layout engine honors, not a wrapping
/// decision made here (see [`number_figures`]). Generic targets
/// (paragraphs, images, raw blocks) have no counter and render as the
/// bare label.
fn render_target(target: &LabelTarget, label: &str) -> String {
    match &target.kind {
        LabelTargetKind::Section { number } if !number.is_empty() => number.clone(),
        LabelTargetKind::Figure { number, supplement } if !number.is_empty() => {
            figure_label_prefix(supplement, number)
        }
        // A numbered target carrying an empty number is a resolver/lowerer
        // bug; fall back to the label name so the output stays readable.
        LabelTargetKind::Section { .. }
        | LabelTargetKind::Figure { .. }
        | LabelTargetKind::Generic => label.to_owned(),
    }
}

/// Whether `label` can be spelled as an `@` reference — i.e. it is drawn
/// from the reference grammar's alphabet `[A-Za-z0-9_:.-]` (mirrors
/// `scan_label_chars` in `mos-parse`). `#figure(label: …)` and
/// `#image(label: …)` accept arbitrary strings, so the label index can hold
/// names — `"intro x"`, non-ASCII — that an `@…` reference can never name;
/// suggesting one would produce a fix that does not parse.
fn is_reference_label(label: &str) -> bool {
    !label.is_empty()
        && label
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b':' | b'.'))
}

/// Levenshtein edit distance between `a` and `b` over their bytes.
///
/// Callers only pass reference-alphabet labels (the parsed reference name and
/// [`is_reference_label`] candidates), all ASCII, so byte distance equals
/// character distance while staying allocation-light: one reusable row, where
/// `row[j]` holds the distance from the processed prefix of `a` to `b[..j]`.
fn edit_distance(a: &str, b: &str) -> usize {
    let b = b.as_bytes();
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, &ai) in a.as_bytes().iter().enumerate() {
        let mut diag = row[0];
        row[0] = i + 1;
        for (j, &bj) in b.iter().enumerate() {
            let cost = usize::from(ai != bj);
            let sub = diag + cost;
            diag = row[j + 1];
            row[j + 1] = sub.min(row[j + 1] + 1).min(row[j] + 1);
        }
    }
    row[b.len()]
}

/// The single nearest *resolvable* label to `unknown`, when one is a
/// reasonable near-miss rather than an unrelated string — the candidate for a
/// "did you mean `@intro`?" fix on an unknown reference.
///
/// "Reasonable" is deliberately conservative:
///
/// - references shorter than three bytes get no suggestion (a one-edit guess
///   on a one- or two-byte name is noise, not help);
/// - the edit distance must be within `unknown.len() / 3` — rustc's "did you
///   mean" heuristic. With the length floor that bound is always at least 1,
///   admitting `intrdo` → `intro` (distance 1, bound 2) while rejecting wholly
///   unrelated names.
///
/// Candidates are the label-index keys that [`is_reference_label`] accepts.
/// The index is the resolvable, first-occurrence-wins set, so any surviving
/// candidate both resolves and is spellable as `@candidate`. Ties break on
/// `(distance, label)`; the `BTreeMap` already yields labels in sorted order,
/// so the choice is identical on every run and every fixpoint pass.
fn nearest_label(unknown: &str, labels: &BTreeMap<String, LabelTarget>) -> Option<String> {
    if unknown.len() < 3 {
        return None;
    }
    let max_distance = unknown.len() / 3;
    labels
        .keys()
        .filter(|label| is_reference_label(label))
        .map(|label| (edit_distance(unknown, label), label))
        .filter(|&(distance, _)| distance <= max_distance)
        .min_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(b.1)))
        .map(|(_, label)| label.clone())
}

/// Rewrite each `Reference` node's `text` attribute to point at its
/// target. Returns true if any node was mutated this iteration —
/// callers use that signal to drive the §6 stage 3 fixpoint loop.
fn rewrite_references(
    document: &mut Document,
    labels: &BTreeMap<String, LabelTarget>,
    bib_keys: &BTreeSet<String>,
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
                .any(|d| d.def().code() == codes::MOS0033.code() && d.span() == Some(&node.span));
            if !already_diagnosed {
                let mut diagnostic = Diagnostic::simple(
                    &codes::MOS0033,
                    None,
                    format!("unknown label `{label}` in `@` reference"),
                )
                .with_span(node.span.clone());
                // An `@key` that misses every label but exactly matches a
                // bibliography key is a citation written with the wrong
                // syntax (`@key` instead of `[@key]`). That exact match is a
                // stronger signal than any label near-miss, so it wins: offer
                // the citation form and say why. `node.span` covers the whole
                // `@label` token (sigil included), so the replacement supplies
                // the full `[@key]`.
                if bib_keys.contains(&label) {
                    diagnostic = diagnostic
                        .with_annotation(DiagnosticAnnotation::Hint(format!(
                            "`{label}` is a bibliography key; cite it as `[@{label}]`"
                        )))
                        .with_suggestion(Suggestion::new(node.span.clone(), format!("[@{label}]")));
                } else if let Some(candidate) = nearest_label(&label, labels) {
                    // Offer the nearest existing label as a machine-applicable
                    // fix (`@intrdo` -> `@intro`) when a reasonable near-miss
                    // exists. The replacement carries its own `@`.
                    diagnostic = diagnostic.with_suggestion(Suggestion::new(
                        node.span.clone(),
                        format!("@{candidate}"),
                    ));
                }
                diagnostics.push(diagnostic);
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

    fn apply_suggestion(src: &str, suggestion: &Suggestion) -> String {
        let mut out = String::new();
        out.push_str(&src[..suggestion.span.start()]);
        out.push_str(&suggestion.replacement);
        out.push_str(&src[suggestion.span.end()..]);
        out
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
    fn duplicate_label_emits_mos0030_and_keeps_first() {
        let src = "= A <dup>\n\n= B <dup>\n\nsee @dup\n";
        let (doc, diags) = lower(src);
        let mos0030: Vec<&Diagnostic> = diags
            .iter()
            .filter(|d| d.def().code() == codes::MOS0030.code())
            .collect();
        assert_eq!(
            mos0030.len(),
            1,
            "expected exactly one MOS0030, got {diags:?}"
        );
        let d = mos0030[0];
        assert_eq!(d.def().code(), codes::MOS0030.code());
        assert_eq!(d.severity(), Severity::Error);
        assert!(
            d.message().contains("`dup`"),
            "MOS0030 message should name the duplicated label, got {:?}",
            d.message()
        );
        // The duplicate diagnostic must point at the *second* occurrence
        // and carry a Related annotation back to the first declaration.
        // Editor UIs rely on both spans to render the redeclaration jump.
        assert_eq!(
            d.span().map(|span| &src[span.start()..span.end()]),
            Some("= B <dup>"),
            "MOS0030 span should cover the second heading exactly"
        );
        assert_eq!(
            d.annotations().len(),
            1,
            "MOS0030 should reference the first decl"
        );
        let related = d.annotations().iter().find_map(|a| match a {
            DiagnosticAnnotation::Related { span, message } => Some((span, message)),
            _ => None,
        });
        assert!(related.is_some(), "MOS0030 carries a Related annotation");
        if let Some((note_span, note_message)) = related {
            assert_eq!(
                &src[note_span.start()..note_span.end()],
                "= A <dup>",
                "MOS0030 note should point at the original declaration exactly"
            );
            assert!(
                note_message.contains("`dup`"),
                "first-decl note should name the label, got {note_message:?}"
            );
        }
        // The duplicate carries exactly one structured rename suggestion:
        // replace only the duplicate label token with the smallest free
        // `dup-2` candidate (nothing else here claims it). Editors apply this
        // as a fix-it, so the payload — span + replacement — must preserve the
        // surrounding heading syntax.
        let suggestions = d.suggestions();
        assert_eq!(
            suggestions.len(),
            1,
            "MOS0030 should carry exactly one rename suggestion, got {suggestions:?}"
        );
        if let Some(suggestion) = suggestions.first() {
            assert_eq!(
                &src[suggestion.span.start()..suggestion.span.end()],
                "dup",
                "suggestion span should cover only the duplicate label token"
            );
            assert_eq!(
                suggestion.replacement, "dup-2",
                "suggestion should rename the duplicate label deterministically"
            );
            assert_eq!(
                apply_suggestion(src, suggestion),
                "= A <dup>\n\n= B <dup-2>\n\nsee @dup\n",
                "applying the fix must preserve the heading and label delimiters"
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
    fn triple_duplicate_label_emits_one_mos0030_per_redeclaration() {
        // Three sections share `dup`. The first wins; the second and
        // third each get their own MOS0030 pointing back at the first.
        // The reference still resolves to section number `1`.
        let src = "= A <dup>\n\n= B <dup>\n\n= C <dup>\n\nsee @dup\n";
        let (doc, diags) = lower(src);
        let mos0030: Vec<&Diagnostic> = diags
            .iter()
            .filter(|d| d.def().code() == codes::MOS0030.code())
            .collect();
        assert_eq!(
            mos0030.len(),
            2,
            "expected two MOS0030 (one per redeclaration), got {diags:?}"
        );
        let spans: Vec<&str> = mos0030
            .iter()
            .filter_map(|d| d.span().map(|s| &src[s.start()..s.end()]))
            .collect();
        assert_eq!(
            spans.len(),
            mos0030.len(),
            "every MOS0030 must carry a primary span"
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
        for d in &mos0030 {
            let related = d.annotations().iter().find_map(|a| match a {
                DiagnosticAnnotation::Related { span, message } => Some((span, message)),
                _ => None,
            });
            assert!(related.is_some(), "MOS0030 carries a Related annotation");
            if let Some((ns, _)) = related {
                assert_eq!(
                    &src[ns.start()..ns.end()],
                    "= A <dup>",
                    "every redeclaration must link back to the first decl"
                );
            }
            // Each redeclaration carries its own deterministic rename
            // suggestion over its own label-token span. Generated suggestions
            // are reserved during this resolver pass, so bulk-applying both
            // fixes does not create a fresh duplicate.
            let suggestions = d.suggestions();
            assert_eq!(
                suggestions.len(),
                1,
                "each MOS0030 carries exactly one rename suggestion, got {suggestions:?}"
            );
            if let Some(suggestion) = suggestions.first() {
                assert_eq!(&src[suggestion.span.start()..suggestion.span.end()], "dup");
            }
        }
        let replacements: Vec<&str> = mos0030
            .iter()
            .filter_map(|d| d.suggestions().first())
            .map(|suggestion| suggestion.replacement.as_str())
            .collect();
        assert_eq!(replacements, vec!["dup-2", "dup-3"]);
        let reference_text = doc
            .nodes()
            .find(|n| n.kind == NodeKind::Reference)
            .and_then(|n| n.attributes.get("text"));
        assert_eq!(reference_text, Some(&AttrValue::Str("1".to_owned())));
    }

    #[test]
    fn duplicate_suggestion_skips_existing_label() {
        // `dup-2` already names another block, so the collision-aware rename
        // for the duplicate `dup` must step over it to `dup-3` rather than
        // propose a name that would just re-collide. Only `dup` is
        // duplicated; `dup-2` is a distinct, valid label (hyphens are legal
        // label chars).
        let src = "= A <dup>\n\n= B <dup-2>\n\n= C <dup>\n";
        let (_doc, diags) = lower(src);
        let mos0030: Vec<&Diagnostic> = diags
            .iter()
            .filter(|d| d.def().code() == codes::MOS0030.code())
            .collect();
        assert_eq!(mos0030.len(), 1, "only `dup` is duplicated, got {diags:?}");
        let d = mos0030[0];
        let suggestions = d.suggestions();
        assert_eq!(
            suggestions.len(),
            1,
            "the duplicate carries one rename suggestion, got {suggestions:?}"
        );
        if let Some(suggestion) = suggestions.first() {
            assert_eq!(
                suggestion.replacement, "dup-3",
                "rename must skip the existing `dup-2` and land on the next free suffix"
            );
            assert_eq!(
                &src[suggestion.span.start()..suggestion.span.end()],
                "dup",
                "suggestion targets the duplicate label token"
            );
            assert_eq!(
                apply_suggestion(src, suggestion),
                "= A <dup>\n\n= B <dup-2>\n\n= C <dup-3>\n",
                "applying the fix must preserve the duplicate declaration syntax"
            );
        }
    }

    #[test]
    fn unknown_label_emits_mos0033() {
        let (doc, diags) = lower("see @no:such\n");
        let mos0033: Vec<&Diagnostic> = diags
            .iter()
            .filter(|d| d.def().code() == codes::MOS0033.code())
            .collect();
        assert_eq!(
            mos0033.len(),
            1,
            "expected exactly one MOS0033 even with the fixpoint loop, got {diags:?}"
        );
        let d = mos0033[0];
        assert_eq!(d.def().code(), codes::MOS0033.code());
        assert_eq!(d.severity(), Severity::Error);
        assert!(
            d.message().contains("`no:such`"),
            "MOS0033 message should name the missing label, got {:?}",
            d.message()
        );
        assert!(
            d.span().is_some(),
            "MOS0033 must carry a span so editors can jump to the bad reference"
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
    fn multiple_unknown_references_each_emit_one_mos0033() {
        // Three distinct unknown labels in a single paragraph produce one
        // diagnostic apiece in a single resolver pass.
        let src = "see @alpha and @beta and @gamma\n";
        let (_doc, diags) = lower(src);
        let mos0033: Vec<&Diagnostic> = diags
            .iter()
            .filter(|d| d.def().code() == codes::MOS0033.code())
            .collect();
        assert_eq!(
            mos0033.len(),
            3,
            "expected one MOS0033 per unknown label, got {diags:?}"
        );
        let labels: BTreeSet<&str> = mos0033
            .iter()
            .filter_map(|d| {
                // Each MOS0033's message is `unknown label `<name>` in `@` reference`.
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
    fn unknown_reference_suggestion_is_not_duplicated_after_fixpoint_rerun() {
        // The resolved `@intro` changes reference text on the first pass, so
        // the fixpoint runs again. The unknown `@intrdo` must still get one
        // MOS0033 with one structured suggestion, not one per iteration.
        let src = "= Intro <intro>\n\nsee @intro and @intrdo\n";
        let (doc, diags) = lower(src);
        let mos0033: Vec<&Diagnostic> = diags
            .iter()
            .filter(|d| d.def().code() == codes::MOS0033.code())
            .collect();
        assert_eq!(
            mos0033.len(),
            1,
            "expected one MOS0033 after fixpoint rerun, got {diags:?}"
        );
        let d = mos0033[0];
        let suggestions = d.suggestions();
        assert_eq!(
            suggestions.len(),
            1,
            "expected one suggestion after fixpoint rerun, got {suggestions:?}"
        );
        if let Some(suggestion) = suggestions.first() {
            assert_eq!(suggestion.replacement, "@intro");
            assert_eq!(
                apply_suggestion(src, suggestion),
                "= Intro <intro>\n\nsee @intro and @intro\n",
                "fix should replace only the unknown reference token"
            );
        }
        let reference_texts: Vec<&str> = doc
            .nodes()
            .filter(|n| n.kind == NodeKind::Reference)
            .filter_map(|n| match n.attributes.get("text") {
                Some(AttrValue::Str(s)) => Some(s.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(
            reference_texts,
            vec!["1", "?intrdo?"],
            "resolved refs rewrite while unknown refs keep visible placeholders"
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
        // text. No MOS0033 is emitted because the target exists.
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
            mos_core::NodeSpec::new(kind, SourceSpan::placeholder(doc.file.clone()))
                .with_attributes(attrs),
        )
    }

    /// Build a synthetic `Text` node under `parent` carrying `text`,
    /// returning its id. The lowerer's caption text nodes have exactly
    /// this shape.
    fn make_text(doc: &mut Document, parent: mos_core::NodeId, text: &str) -> mos_core::NodeId {
        let mut attrs = mos_core::AttrMap::new();
        attrs.insert("text".to_owned(), AttrValue::Str(text.to_owned()));
        doc.alloc_child(
            parent,
            mos_core::NodeSpec::new(NodeKind::Text, SourceSpan::placeholder(doc.file.clone()))
                .with_attributes(attrs),
        )
    }

    /// Build a `Figure` (optionally labelled) carrying a `role = "caption"`
    /// paragraph whose single `Text` child holds `caption`. Returns the
    /// figure id and the caption text-node id so tests can assert on the
    /// stamped label. Mirrors the shape the lowerer produces for a
    /// captioned `#figure`.
    fn make_captioned_figure(
        doc: &mut Document,
        label: Option<&str>,
        caption: &str,
    ) -> (mos_core::NodeId, mos_core::NodeId) {
        let figure = make_node(doc, NodeKind::Figure, label, None);
        let mut caption_attrs = mos_core::AttrMap::new();
        caption_attrs.insert("role".to_owned(), AttrValue::Str("caption".to_owned()));
        let caption_para = doc.alloc_child(
            figure,
            mos_core::NodeSpec::new(
                NodeKind::Paragraph,
                SourceSpan::placeholder(doc.file.clone()),
            )
            .with_attributes(caption_attrs),
        );
        let caption_text = make_text(doc, caption_para, caption);
        (figure, caption_text)
    }

    /// Read a node's resolved `number` attribute as an owned string, or
    /// the empty string if the node is missing or unnumbered. Test-only
    /// convenience wrapping [`captured_number`] for the numbering
    /// assertions below.
    fn node_number(doc: &Document, id: mos_core::NodeId) -> String {
        doc.get(id).map(captured_number).unwrap_or_default()
    }

    #[test]
    fn classify_target_distinguishes_kinds() {
        let mut doc = Document::new(PathBuf::from("test.mos"));
        let section_id = make_node(&mut doc, NodeKind::Section, Some("sec"), Some("1.2"));
        let figure_id = make_node(&mut doc, NodeKind::Figure, Some("fig"), Some("3"));
        let paragraph_id = make_node(&mut doc, NodeKind::Paragraph, Some("p"), None);

        assert_eq!(
            doc.get(section_id).map(classify_target),
            Some(LabelTargetKind::Section {
                number: "1.2".to_owned()
            })
        );

        assert_eq!(
            doc.get(figure_id).map(classify_target),
            Some(LabelTargetKind::Figure {
                number: "3".to_owned(),
                supplement: "Figure".to_owned(),
            })
        );

        assert_eq!(
            doc.get(paragraph_id).map(classify_target),
            Some(LabelTargetKind::Generic)
        );
    }

    #[test]
    fn figure_reference_renders_kind_aware_text() {
        // Constructs a Figure node with a label and a Reference to it,
        // then runs the full resolver. Verifies:
        //   - the figure receives document-order number "1",
        //   - the figure label is found (no MOS0033),
        //   - the label index records the target as a numbered `Figure`,
        //   - the reference's rewritten text is kind-aware `"Figure 1"`,
        //     not the bare label name.
        let mut doc = Document::new(PathBuf::from("test.mos"));
        let figure_id = make_node(&mut doc, NodeKind::Figure, Some("fig:one"), None);
        let ref_id = doc.alloc_child(
            doc.root,
            mos_core::NodeSpec::new(
                NodeKind::Reference,
                SourceSpan::placeholder(doc.file.clone()),
            )
            .with_attributes({
                let mut a = mos_core::AttrMap::new();
                a.insert("label".to_owned(), AttrValue::Str("fig:one".to_owned()));
                a.insert("text".to_owned(), AttrValue::Str("?fig:one?".to_owned()));
                a
            }),
        );

        let diags = resolve(&mut doc, &BTreeSet::new());
        assert!(diags.is_empty(), "{diags:?}");

        // The figure carries its resolved document-order number.
        assert_eq!(
            doc.get(figure_id).and_then(|f| f.attributes.get("number")),
            Some(&AttrValue::Str("1".to_owned()))
        );

        let mut sink: Vec<Diagnostic> = Vec::new();
        let index = build_label_index(&doc, &mut sink);
        assert!(sink.is_empty(), "{sink:?}");
        assert_eq!(
            index.get("fig:one").map(|target| &target.kind),
            Some(&LabelTargetKind::Figure {
                number: "1".to_owned(),
                supplement: "Figure".to_owned(),
            })
        );

        assert_eq!(
            doc.get(ref_id).and_then(|r| r.attributes.get("text")),
            Some(&AttrValue::Str("Figure\u{00A0}1".to_owned())),
            "a figure reference resolves to kind-aware `Figure N` text, joined by a non-breaking space"
        );
    }

    #[test]
    fn captioned_figure_gets_supplement_label_stamped() {
        // A figure with a `role = "caption"` paragraph gets its caption
        // text prefixed with the non-breaking `Figure N: ` label so the
        // number is visible; the figure itself is still numbered "1".
        let mut doc = Document::new(PathBuf::from("test.mos"));
        let (figure, caption_text) = make_captioned_figure(&mut doc, Some("fig:a"), "A plot.");

        let diags = resolve(&mut doc, &BTreeSet::new());
        assert!(diags.is_empty(), "{diags:?}");

        assert_eq!(node_number(&doc, figure), "1");
        assert_eq!(
            read_str_attr(&doc, caption_text, "text"),
            Some("Figure\u{00A0}1: A plot.".to_owned()),
            "the caption is prefixed with the non-breaking `Figure N: ` label"
        );
    }

    #[test]
    fn skipped_figure_omits_label_and_does_not_advance_counter() {
        // `#figure(numbered: false)` opts out of numbering (issue #76): no
        // `number` attribute, no `Figure N:` caption prefix, and — the
        // documented counter rule — the skip does not advance the counter,
        // so a later numbered figure is still "Figure 1", not "Figure 2".
        let mut doc = Document::new(PathBuf::from("test.mos"));
        let (skipped, skipped_caption) =
            make_captioned_figure(&mut doc, Some("fig:skip"), "Decorative.");
        if let Some(node) = doc.get_mut(skipped) {
            node.attributes
                .insert("numbered".to_owned(), AttrValue::Bool(false));
        }
        let (numbered, numbered_caption) =
            make_captioned_figure(&mut doc, Some("fig:num"), "A plot.");

        let diags = resolve(&mut doc, &BTreeSet::new());
        assert!(diags.is_empty(), "{diags:?}");

        assert_eq!(
            node_number(&doc, skipped),
            "",
            "a skipped figure carries no number"
        );
        assert_eq!(
            read_str_attr(&doc, skipped_caption, "text"),
            Some("Decorative.".to_owned()),
            "a skipped figure's caption keeps no `Figure N:` prefix"
        );
        assert_eq!(
            node_number(&doc, numbered),
            "1",
            "the skipped figure must not consume or gap the counter"
        );
        assert_eq!(
            read_str_attr(&doc, numbered_caption, "text"),
            Some("Figure\u{00A0}1: A plot.".to_owned())
        );
    }

    #[test]
    fn custom_supplement_renders_in_caption_and_reference() {
        // `#figure(supplement: "Plate")` swaps the supplement word in both
        // the stamped caption and any reference to the figure (issue #76).
        let mut doc = Document::new(PathBuf::from("test.mos"));
        let (figure, caption_text) = make_captioned_figure(&mut doc, Some("fig:plate"), "A map.");
        if let Some(node) = doc.get_mut(figure) {
            node.attributes
                .insert("supplement".to_owned(), AttrValue::Str("Plate".to_owned()));
        }
        let ref_id = doc.alloc_child(
            doc.root,
            mos_core::NodeSpec::new(
                NodeKind::Reference,
                SourceSpan::placeholder(doc.file.clone()),
            )
            .with_attributes({
                let mut a = mos_core::AttrMap::new();
                a.insert("label".to_owned(), AttrValue::Str("fig:plate".to_owned()));
                a.insert("text".to_owned(), AttrValue::Str("?fig:plate?".to_owned()));
                a
            }),
        );

        let diags = resolve(&mut doc, &BTreeSet::new());
        assert!(diags.is_empty(), "{diags:?}");

        assert_eq!(
            read_str_attr(&doc, caption_text, "text"),
            Some("Plate\u{00A0}1: A map.".to_owned()),
            "the caption uses the custom supplement word"
        );
        assert_eq!(
            doc.get(ref_id).and_then(|r| r.attributes.get("text")),
            Some(&AttrValue::Str("Plate\u{00A0}1".to_owned())),
            "a reference renders the custom supplement, not `Figure`"
        );
    }

    #[test]
    fn empty_supplement_renders_number_only() {
        // `#figure(supplement: "")` / `supplement: none` keeps the figure
        // numbered but drops the supplement word: the caption and any
        // reference show the number alone — the "no visible prefix" form
        // (issue #76). Distinct from `numbered: false`, which drops the
        // number entirely.
        let mut doc = Document::new(PathBuf::from("test.mos"));
        let (figure, caption_text) = make_captioned_figure(&mut doc, Some("fig:plain"), "A chart.");
        if let Some(node) = doc.get_mut(figure) {
            node.attributes
                .insert("supplement".to_owned(), AttrValue::Str(String::new()));
        }
        let ref_id = doc.alloc_child(
            doc.root,
            mos_core::NodeSpec::new(
                NodeKind::Reference,
                SourceSpan::placeholder(doc.file.clone()),
            )
            .with_attributes({
                let mut a = mos_core::AttrMap::new();
                a.insert("label".to_owned(), AttrValue::Str("fig:plain".to_owned()));
                a.insert("text".to_owned(), AttrValue::Str("?fig:plain?".to_owned()));
                a
            }),
        );

        let diags = resolve(&mut doc, &BTreeSet::new());
        assert!(diags.is_empty(), "{diags:?}");

        assert_eq!(
            read_str_attr(&doc, caption_text, "text"),
            Some("1: A chart.".to_owned()),
            "an empty supplement renders the number with no word and no leading space"
        );
        assert_eq!(
            doc.get(ref_id).and_then(|r| r.attributes.get("text")),
            Some(&AttrValue::Str("1".to_owned())),
            "a reference to a number-only figure renders just the number"
        );
    }

    #[test]
    fn reference_to_skipped_figure_renders_bare_label() {
        // A reference to a `numbered: false` figure has no number to show,
        // so it falls back to the bare label name — like an image reference.
        let mut doc = Document::new(PathBuf::from("test.mos"));
        let figure = make_node(&mut doc, NodeKind::Figure, Some("fig:skip"), None);
        if let Some(node) = doc.get_mut(figure) {
            node.attributes
                .insert("numbered".to_owned(), AttrValue::Bool(false));
        }
        let ref_id = doc.alloc_child(
            doc.root,
            mos_core::NodeSpec::new(
                NodeKind::Reference,
                SourceSpan::placeholder(doc.file.clone()),
            )
            .with_attributes({
                let mut a = mos_core::AttrMap::new();
                a.insert("label".to_owned(), AttrValue::Str("fig:skip".to_owned()));
                a.insert("text".to_owned(), AttrValue::Str("?fig:skip?".to_owned()));
                a
            }),
        );

        let diags = resolve(&mut doc, &BTreeSet::new());
        assert!(diags.is_empty(), "{diags:?}");

        assert_eq!(
            doc.get(ref_id).and_then(|r| r.attributes.get("text")),
            Some(&AttrValue::Str("fig:skip".to_owned())),
            "a reference to a skipped figure renders the bare label"
        );
    }

    #[test]
    fn resolve_is_idempotent_for_captioned_figures() {
        // `resolve` is public and re-entrant: the §6 stage 3 fixpoint and
        // future page-reference passes rerun it. Stamping the caption
        // label must therefore be idempotent — the second pass has to
        // reproduce `"Figure 1: A plot."` byte-for-byte instead of
        // re-reading the stamped text and nesting the label into
        // `"Figure 1: Figure 1: A plot."`.
        let mut doc = Document::new(PathBuf::from("test.mos"));
        let (_figure, caption_text) = make_captioned_figure(&mut doc, Some("fig:a"), "A plot.");

        let first = resolve(&mut doc, &BTreeSet::new());
        assert!(first.is_empty(), "{first:?}");
        let after_first = read_str_attr(&doc, caption_text, "text");
        assert_eq!(after_first, Some("Figure\u{00A0}1: A plot.".to_owned()));

        let second = resolve(&mut doc, &BTreeSet::new());
        assert!(second.is_empty(), "{second:?}");
        assert_eq!(
            read_str_attr(&doc, caption_text, "text"),
            after_first,
            "a second resolve pass must not re-stamp the figure label"
        );
    }

    #[test]
    fn figures_get_sequential_document_order_numbers() {
        // Three figures, one without a label, get flat document-order
        // numbers. Numbering is unconditional: the unlabelled middle
        // figure still advances the counter.
        let mut doc = Document::new(PathBuf::from("test.mos"));
        let first = make_node(&mut doc, NodeKind::Figure, Some("fig:a"), None);
        let middle = make_node(&mut doc, NodeKind::Figure, None, None);
        let last = make_node(&mut doc, NodeKind::Figure, Some("fig:c"), None);

        let diags = resolve(&mut doc, &BTreeSet::new());
        assert!(diags.is_empty(), "{diags:?}");

        assert_eq!(node_number(&doc, first), "1");
        assert_eq!(
            node_number(&doc, middle),
            "2",
            "unlabelled figures are still numbered"
        );
        assert_eq!(node_number(&doc, last), "3");
    }

    #[test]
    fn figures_and_sections_use_independent_counters() {
        // Sections and figures count independently: a figure sandwiched
        // between two sections is still figure "1", and the sections are
        // "1"/"2" regardless of the figures interleaved with them.
        let mut doc = Document::new(PathBuf::from("test.mos"));
        let sec_one = make_node(&mut doc, NodeKind::Section, Some("sec:a"), None);
        let fig_one = make_node(&mut doc, NodeKind::Figure, Some("fig:a"), None);
        let sec_two = make_node(&mut doc, NodeKind::Section, Some("sec:b"), None);
        let fig_two = make_node(&mut doc, NodeKind::Figure, Some("fig:b"), None);

        let diags = resolve(&mut doc, &BTreeSet::new());
        assert!(diags.is_empty(), "{diags:?}");

        assert_eq!(node_number(&doc, sec_one), "1");
        assert_eq!(node_number(&doc, sec_two), "2");
        assert_eq!(node_number(&doc, fig_one), "1");
        assert_eq!(node_number(&doc, fig_two), "2");
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

    #[test]
    fn unknown_reference_suggests_nearest_label() {
        // A near-miss typo gets a machine-applicable "did you mean" fix:
        // replace the whole `@intrdo` token (sigil included) with `@intro`.
        let src = "= Intro <intro>\n\nsee @intrdo\n";
        let (doc, diags) = lower(src);
        let mos0033: Vec<&Diagnostic> = diags
            .iter()
            .filter(|d| d.def().code() == codes::MOS0033.code())
            .collect();
        assert_eq!(
            mos0033.len(),
            1,
            "expected exactly one MOS0033, got {diags:?}"
        );
        let d = mos0033[0];
        // Message and span are unchanged from the no-suggestion path.
        assert!(
            d.message().contains("`intrdo`"),
            "message should still name the missing label, got {:?}",
            d.message()
        );
        assert_eq!(
            d.span().map(|span| &src[span.start()..span.end()]),
            Some("@intrdo"),
            "MOS0033 span should still cover the bad reference exactly"
        );
        // Exactly one structured suggestion, replacing the full reference.
        let suggestions = d.suggestions();
        assert_eq!(
            suggestions.len(),
            1,
            "expected one nearest-label suggestion, got {suggestions:?}"
        );
        if let Some(suggestion) = suggestions.first() {
            assert_eq!(
                &src[suggestion.span.start()..suggestion.span.end()],
                "@intrdo",
                "suggestion should replace the whole `@` reference token"
            );
            assert_eq!(suggestion.replacement, "@intro");
            assert_eq!(
                apply_suggestion(src, suggestion),
                "= Intro <intro>\n\nsee @intro\n",
                "applying the fix should rewrite `@intrdo` to `@intro`"
            );
        }
        // The unresolved placeholder stays visible in the meantime.
        let reference_text = doc
            .nodes()
            .find(|n| n.kind == NodeKind::Reference)
            .and_then(|n| n.attributes.get("text"));
        assert_eq!(reference_text, Some(&AttrValue::Str("?intrdo?".to_owned())));
    }

    #[test]
    fn unknown_reference_suggestion_breaks_ties_deterministically() {
        // `@intrx` sits one edit from both `intra` and `intro`. The tie
        // breaks on `(distance, label)`, so the single suggestion is always
        // the lexicographically smaller `@intra`.
        let src = "= A <intra>\n\n= B <intro>\n\nsee @intrx\n";
        let (_doc, diags) = lower(src);
        let mos0033: Vec<&Diagnostic> = diags
            .iter()
            .filter(|d| d.def().code() == codes::MOS0033.code())
            .collect();
        assert_eq!(mos0033.len(), 1, "got {diags:?}");
        if let Some(d) = mos0033.first() {
            let suggestions = d.suggestions();
            assert_eq!(
                suggestions.len(),
                1,
                "exactly one nearest-label suggestion, got {suggestions:?}"
            );
            if let Some(suggestion) = suggestions.first() {
                assert_eq!(
                    suggestion.replacement, "@intra",
                    "ties must resolve to the lexicographically smaller label"
                );
            }
        }
    }

    #[test]
    fn unknown_reference_without_close_match_has_no_suggestion() {
        // An unrelated reference name is left without a guess.
        let src = "= Intro <intro>\n\nsee @conclusion\n";
        let (_doc, diags) = lower(src);
        let mos0033: Vec<&Diagnostic> = diags
            .iter()
            .filter(|d| d.def().code() == codes::MOS0033.code())
            .collect();
        assert_eq!(mos0033.len(), 1, "got {diags:?}");
        if let Some(d) = mos0033.first() {
            assert!(
                d.suggestions().is_empty(),
                "an unrelated label must not be suggested, got {:?}",
                d.suggestions()
            );
        }
    }

    #[test]
    fn short_unknown_reference_has_no_suggestion() {
        // Conservative floor: references shorter than three bytes never get a
        // suggestion, even when a one-edit neighbour (`ax`) exists.
        let src = "= A <ax>\n\nsee @ab\n";
        let (_doc, diags) = lower(src);
        let mos0033: Vec<&Diagnostic> = diags
            .iter()
            .filter(|d| d.def().code() == codes::MOS0033.code())
            .collect();
        assert_eq!(mos0033.len(), 1, "got {diags:?}");
        if let Some(d) = mos0033.first() {
            assert!(
                d.suggestions().is_empty(),
                "short references must not be guessed, got {:?}",
                d.suggestions()
            );
        }
    }

    #[test]
    fn unreferenceable_label_is_not_suggested() {
        // `#figure(label: "...")` / `#image(label: "...")` accept arbitrary
        // strings, so the index can hold a label the `@`-reference grammar
        // cannot spell. `@intro x` would not parse, so even this one-edit
        // match must be filtered out and produce no suggestion.
        let mut doc = Document::new(PathBuf::from("test.mos"));
        let _figure = make_node(&mut doc, NodeKind::Figure, Some("intro x"), None);
        let _reference = make_node(&mut doc, NodeKind::Reference, Some("introx"), None);

        let mut diagnostics: Vec<Diagnostic> = Vec::new();
        let index = build_label_index(&doc, &mut diagnostics);
        let changed = rewrite_references(&mut doc, &index, &BTreeSet::new(), &mut diagnostics);
        assert!(!changed, "an unknown reference rewrites no text");

        let mos0033: Vec<&Diagnostic> = diagnostics
            .iter()
            .filter(|d| d.def().code() == codes::MOS0033.code())
            .collect();
        assert_eq!(mos0033.len(), 1, "got {diagnostics:?}");
        if let Some(d) = mos0033.first() {
            assert!(
                d.suggestions().is_empty(),
                "an unreferenceable label must not be suggested, got {:?}",
                d.suggestions()
            );
        }
    }
}
