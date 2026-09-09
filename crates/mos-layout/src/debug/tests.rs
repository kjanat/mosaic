#![allow(clippy::unwrap_used, clippy::expect_used, reason = "test assertions")]

use std::path::PathBuf;
use std::sync::Arc;

use mos_core::{AttrMap, AttrValue, Document, NodeId, NodeKind, NodeSpec, SourceSpan};

use super::Report;
use crate::{Base14Font, Font, LayoutEngine, MARGIN_PT, text_width};

fn alloc(
    doc: &mut Document,
    parent: NodeId,
    kind: NodeKind,
    start: usize,
    end: usize,
    attrs: AttrMap,
) -> NodeId {
    doc.alloc_child(
        parent,
        NodeSpec::new(
            kind,
            SourceSpan::new(PathBuf::from("fixture.mos"), start, end),
        )
        .with_attributes(attrs),
    )
}

fn document() -> Document {
    let mut doc = Document::new(PathBuf::from("fixture.mos"));
    let root = doc.root;
    alloc(
        &mut doc,
        root,
        NodeKind::Raw,
        0,
        0,
        AttrMap::from([
            ("set".into(), AttrValue::Str("text".into())),
            ("set.arg.font".into(), AttrValue::Str("Helvetica".into())),
        ]),
    );
    doc
}

fn paragraph(doc: &mut Document, parent: NodeId, text: &str) -> NodeId {
    let id = alloc(
        doc,
        parent,
        NodeKind::Paragraph,
        10,
        10 + text.len(),
        AttrMap::new(),
    );
    alloc(
        doc,
        id,
        NodeKind::Text,
        10,
        10 + text.len(),
        AttrMap::from([("text".into(), AttrValue::Str(text.into()))]),
    );
    id
}

fn trace(doc: &Document) -> Report {
    LayoutEngine::new().layout_with_debug(doc).debug.unwrap()
}

#[test]
fn tracing_is_opt_in_and_empty_documents_still_report_a_page() {
    let doc = document();
    assert!(LayoutEngine::new().layout(&doc).debug.is_none());
    let report = trace(&doc);
    assert_eq!(report.schema_version, 1);
    assert_eq!(report.units, "pt");
    assert_eq!(report.origin, "top-left");
    assert_eq!(report.pages.len(), 1);
    let page = &report.pages[0];
    assert_eq!(page.number, 1);
    assert_eq!(page.bounds.width_pt, crate::A4_WIDTH_PT);
    assert_eq!(page.content_bounds.x_pt, MARGIN_PT);
    assert!(page.lines.is_empty() && page.images.is_empty() && page.blocks.is_empty());
}

#[test]
fn line_geometry_and_source_mapping_match_emitted_runs() {
    let mut doc = document();
    let root = doc.root;
    let id = paragraph(&mut doc, root, "Hello world");
    let normal = LayoutEngine::new().layout(&doc);
    let debug = LayoutEngine::new().layout_with_debug(&doc);
    assert_eq!(
        normal.page_boundary_signatures(),
        debug.page_boundary_signatures()
    );
    assert_eq!(normal.label_pages, debug.label_pages);
    assert_eq!(normal.diagnostics.len(), debug.diagnostics.len());
    let page = &debug.debug.as_ref().unwrap().pages[0];
    assert_eq!(page.lines.len(), 1);
    let line = &page.lines[0];
    let source = line.source.as_ref().unwrap();
    assert_eq!(source.node_id, id.0);
    assert_eq!((source.byte_start, source.byte_end), (10, 21));
    assert_eq!(source.file, "fixture.mos");
    assert_eq!(source.kind, "paragraph");
    assert_eq!(line.runs.len(), debug.graph.pages[0].runs.len());
    for (index, run) in line.runs.iter().enumerate() {
        let placed = &debug.graph.pages[0].runs[index];
        assert_eq!(run.run_index, index);
        assert_eq!(run.text, placed.text);
        assert_eq!(run.bounds.x_pt, placed.x_pt);
        assert_eq!(line.baseline_from_top_pt, placed.baseline_from_top_pt);
        assert_eq!(run.font, "Helvetica");
        assert_eq!(
            run.bounds.width_pt,
            text_width(Font::Base14(Base14Font::Helvetica), 11.0, &run.text)
        );
        assert!(run.bounds.y_pt < line.baseline_from_top_pt);
        assert!(run.bounds.y_pt + run.bounds.height_pt > line.baseline_from_top_pt);
    }
    assert_eq!(page.blocks[0].source.node_id, id.0);
    assert_eq!(page.blocks[0].bounds, line.bounds);
}

#[test]
fn a_block_spanning_pages_has_source_bounds_on_every_page() {
    let mut doc = document();
    let root = doc.root;
    let id = paragraph(&mut doc, root, &"word ".repeat(1500));
    let normal = LayoutEngine::new().layout(&doc);
    let debug = LayoutEngine::new().layout_with_debug(&doc);
    assert_eq!(
        normal.page_boundary_signatures(),
        debug.page_boundary_signatures()
    );
    let report = debug.debug.as_ref().unwrap();
    assert!(report.pages.len() > 1);
    assert_eq!(report.pages.len(), debug.graph.pages.len());
    for (page, placed) in report.pages.iter().zip(&debug.graph.pages) {
        assert_eq!(page.number, placed.number);
        assert_eq!(page.blocks.len(), 1);
        assert_eq!(page.blocks[0].source.node_id, id.0);
        assert!(
            page.lines
                .iter()
                .all(|line| line.source.as_ref().unwrap().node_id == id.0)
        );
        assert_eq!(
            page.lines.iter().map(|line| line.runs.len()).sum::<usize>(),
            placed.runs.len()
        );
    }
}

#[test]
fn figure_bounds_enclose_its_image_and_caption() {
    let mut doc = document();
    let root = doc.root;
    let figure = alloc(
        &mut doc,
        root,
        NodeKind::Figure,
        3,
        50,
        AttrMap::from([("label".into(), AttrValue::Str("fig:one".into()))]),
    );
    let image = alloc(
        &mut doc,
        figure,
        NodeKind::Image,
        3,
        50,
        AttrMap::from([
            (
                "resolved_path".into(),
                AttrValue::Str("/virtual/image.png".into()),
            ),
            ("pixel_width".into(), AttrValue::Int(2)),
            ("pixel_height".into(), AttrValue::Int(1)),
            (
                "pixels".into(),
                AttrValue::Bytes(Arc::from([255, 0, 0, 255, 0, 0])),
            ),
            ("width".into(), AttrValue::Length(40.0)),
            ("height".into(), AttrValue::Length(20.0)),
        ]),
    );
    let caption = paragraph(&mut doc, figure, "A caption");
    let after = paragraph(&mut doc, root, "After");
    let report = trace(&doc);
    let page = &report.pages[0];
    assert_eq!(page.images.len(), 1);
    let placed = &page.images[0];
    assert_eq!(placed.source.as_ref().unwrap().node_id, image.0);
    assert_eq!(placed.bounds.width_pt, 40.0);
    assert_eq!(placed.bounds.height_pt, 20.0);
    assert_eq!(placed.image_index, 0);
    let block = page
        .blocks
        .iter()
        .find(|block| block.source.node_id == figure.0)
        .unwrap();
    assert_eq!(block.source.kind, "figure");
    assert_eq!(block.source.label.as_deref(), Some("fig:one"));
    let caption_line = page
        .lines
        .iter()
        .find(|line| line.source.as_ref().unwrap().node_id == caption.0)
        .unwrap();
    assert!(block.bounds.y_pt <= placed.bounds.y_pt);
    assert!(
        block.bounds.y_pt + block.bounds.height_pt
            >= caption_line.bounds.y_pt + caption_line.bounds.height_pt
    );
    assert_eq!(
        page.lines.last().unwrap().source.as_ref().unwrap().node_id,
        after.0
    );
}

#[test]
fn nested_list_sources_and_container_bounds_do_not_leak() {
    let mut doc = document();
    let root = doc.root;
    let list = alloc(&mut doc, root, NodeKind::List, 0, 30, AttrMap::new());
    let item = alloc(&mut doc, list, NodeKind::ListItem, 0, 30, AttrMap::new());
    let outer = paragraph(&mut doc, item, "Outer");
    let nested = alloc(&mut doc, item, NodeKind::List, 15, 30, AttrMap::new());
    let nested_item = alloc(&mut doc, nested, NodeKind::ListItem, 15, 30, AttrMap::new());
    let inner = paragraph(&mut doc, nested_item, "Inner");
    let after = paragraph(&mut doc, root, "After");
    let report = trace(&doc);
    let page = &report.pages[0];
    assert_eq!(
        page.lines
            .iter()
            .map(|line| line.source.as_ref().unwrap().node_id)
            .collect::<Vec<_>>(),
        vec![outer.0, inner.0, after.0]
    );
    for id in [list, item, nested, nested_item, outer, inner, after] {
        assert!(page.blocks.iter().any(|block| block.source.node_id == id.0));
    }
    assert!(
        page.lines[0].runs.len() > 1,
        "list marker is included in line geometry"
    );
}

#[test]
fn raw_copy_text_and_bibliography_entry_sources_are_preserved() {
    let mut doc = document();
    let root = doc.root;
    let raw = alloc(
        &mut doc,
        root,
        NodeKind::Raw,
        0,
        20,
        AttrMap::from([
            ("raw.kind".into(), AttrValue::Str("code".into())),
            ("text".into(), AttrValue::Str("a\tb\n\nlast".into())),
        ]),
    );
    let bib = alloc(
        &mut doc,
        root,
        NodeKind::Bibliography,
        30,
        40,
        AttrMap::new(),
    );
    let entry = paragraph(&mut doc, bib, "Book title");
    doc.get_mut(entry)
        .unwrap()
        .attributes
        .insert("entry_number".into(), AttrValue::Int(1));
    let report = trace(&doc);
    let page = &report.pages[0];
    assert_eq!(page.lines[0].source.as_ref().unwrap().node_id, raw.0);
    assert_eq!(page.lines[0].runs[0].actual_text.as_deref(), Some("a\tb"));
    let line = page.lines.last().unwrap();
    assert_eq!(line.source.as_ref().unwrap().node_id, entry.0);
    assert_eq!(line.runs[0].text, "[1]");
    assert!(
        page.blocks
            .iter()
            .any(|block| block.source.node_id == bib.0)
    );
}
