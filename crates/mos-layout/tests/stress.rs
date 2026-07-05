//! Stress test: lay out a large synthetic document and check that the
//! engine finishes in bounded time, spans many pages, and emits no
//! error-severity diagnostics.
//!
//! The document is built programmatically through the public `mos-core`
//! arena API (no parsing/lowering involved), so this exercises the
//! layout engine in isolation: style resolution, greedy text flow with
//! soft hyphens and NBSP, heading levels 1-3, and nested lists.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use mos_core::{AttrMap, AttrValue, Document, NodeId, NodeKind, NodeSpec, Severity, SourceSpan};
use mos_layout::LayoutEngine;

/// Chapters in the synthetic document. Each chapter contributes
/// 3 headings + 3 paragraphs + 2 lists carrying 7 items, so the arena
/// holds several thousand nodes in total (asserted below via
/// [`NODE_FLOOR`]). Sized so a debug run stays around a couple of
/// seconds: per-word shaping through embedded Noto Sans dominates, so
/// runtime scales with word count, not node count.
const CHAPTERS: usize = 120;

/// Lower bound on arena size so the "large document" claim is checked,
/// not assumed. 120 chapters currently allocate ~3.6k nodes.
const NODE_FLOOR: usize = 3_000;

/// Generous wall-clock ceiling for a debug build. Expected runtime is
/// around a second or two; the ceiling only guards against pathological
/// regressions (accidental quadratic flow, per-word re-shaping, etc.).
const DEBUG_CEILING: Duration = Duration::from_secs(10);

/// Sane lower bound on page count: 120 chapters of real text cannot fit
/// on fewer pages than this without the flow silently dropping content.
const PAGE_FLOOR: usize = 40;

fn span() -> SourceSpan {
    SourceSpan::placeholder(PathBuf::from("stress.mos"))
}

fn spec(kind: NodeKind) -> NodeSpec {
    NodeSpec::new(kind, span())
}

fn alloc_text(doc: &mut Document, parent: NodeId, text: &str) {
    let mut attrs = AttrMap::new();
    attrs.insert("text".to_owned(), AttrValue::Str(text.to_owned()));
    doc.alloc_child(parent, spec(NodeKind::Text).with_attributes(attrs));
}

fn alloc_section(doc: &mut Document, level: i64, text: &str) {
    let mut attrs = AttrMap::new();
    attrs.insert("level".to_owned(), AttrValue::Int(level));
    let id = doc.alloc_child(doc.root, spec(NodeKind::Section).with_attributes(attrs));
    alloc_text(doc, id, text);
}

fn alloc_paragraph(doc: &mut Document, text: &str) {
    let id = doc.alloc_child(doc.root, spec(NodeKind::Paragraph));
    alloc_text(doc, id, text);
}

fn alloc_list(doc: &mut Document, parent: NodeId, ordered: bool) -> NodeId {
    let mut attrs = AttrMap::new();
    attrs.insert("ordered".to_owned(), AttrValue::Bool(ordered));
    doc.alloc_child(parent, spec(NodeKind::List).with_attributes(attrs))
}

fn alloc_list_item(doc: &mut Document, parent: NodeId, text: &str) -> NodeId {
    let item = doc.alloc_child(parent, spec(NodeKind::ListItem));
    alloc_text(doc, item, text);
    item
}

/// Multi-sentence paragraph text with soft hyphens (U+00AD) and NBSP
/// (U+00A0) sprinkled in, varied per chapter/index so identical-string
/// shortcuts cannot mask per-paragraph cost.
fn paragraph_text(chapter: usize, index: usize) -> String {
    format!(
        "Paragraph {index} of chapter {chapter} stresses the greedy \
         breaker. Loads of 42\u{A0}kg stay glued across breaks. Every \
         extra\u{AD}ordinarily long dis\u{AD}cre\u{AD}tion\u{AD}ary word \
         offers break points on narrow measures."
    )
}

fn build_large_document() -> Document {
    let mut doc = Document::new(PathBuf::from("stress.mos"));
    for chapter in 0..CHAPTERS {
        alloc_section(&mut doc, 1, &format!("Chapter {chapter}"));
        alloc_paragraph(&mut doc, &paragraph_text(chapter, 0));
        alloc_section(&mut doc, 2, &format!("Section {chapter}.1"));
        alloc_paragraph(&mut doc, &paragraph_text(chapter, 1));
        alloc_section(&mut doc, 3, &format!("Subsection {chapter}.1.1"));
        alloc_paragraph(&mut doc, &paragraph_text(chapter, 2));

        // Alternate ordered/unordered lists; every list nests one level.
        let ordered = chapter % 2 == 0;
        let root = doc.root;
        let list = alloc_list(&mut doc, root, ordered);
        for item in 0..3 {
            alloc_list_item(&mut doc, list, &format!("Item {item} wraps in the column."));
        }
        let holder = alloc_list_item(&mut doc, list, "Item 3 nests a list.");
        let nested = alloc_list(&mut doc, holder, !ordered);
        for item in 0..3 {
            alloc_list_item(
                &mut doc,
                nested,
                &format!("Nested item {item} restores gutter."),
            );
        }
    }
    doc
}

#[test]
fn layout_of_large_document_stays_fast_and_correct() {
    let doc = build_large_document();
    let nodes = doc.len();
    assert!(
        nodes > NODE_FLOOR,
        "synthetic document shrank below the stress threshold: {nodes} nodes, floor {NODE_FLOOR}"
    );

    let start = Instant::now();
    let result = LayoutEngine::new().layout(&doc);
    let elapsed = start.elapsed();

    assert!(
        elapsed < DEBUG_CEILING,
        "layout of {CHAPTERS} chapters ({nodes} nodes) took {elapsed:.3?}, ceiling is {DEBUG_CEILING:?}"
    );

    let pages = result.graph.pages.len();
    assert!(
        pages > PAGE_FLOOR,
        "expected more than {PAGE_FLOOR} pages for {CHAPTERS} chapters, got {pages}"
    );

    let errors: Vec<String> = result
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.severity() == Severity::Error)
        .map(|diagnostic| diagnostic.message().to_owned())
        .collect();
    assert!(
        errors.is_empty(),
        "layout emitted error diagnostics: {errors:?}"
    );

    // Every page the engine emitted must carry content: an empty page in
    // the middle of a uniform text stream means the flow dropped runs.
    let empty_pages: Vec<u32> = result
        .graph
        .pages
        .iter()
        .filter(|page| page.runs.is_empty() && page.images.is_empty())
        .map(|page| page.number)
        .collect();
    assert!(
        empty_pages.is_empty(),
        "pages {empty_pages:?} came out empty in a document with uniform content"
    );
}
