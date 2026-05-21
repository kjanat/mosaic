//! PDF backend for Mosaic (manifest §21.1).
//!
//! Emits a fixed-A4 PDF declaring all 14 standard PDF base fonts
//! (Helvetica/Times/Courier × 4 + Symbol + `ZapfDingbats`). No font
//! data ships — every glyph outline is supplied by the PDF reader's
//! built-in Core 14 implementations.
//!
//! For each Latin Core 14 face actually used, the backend plans a
//! per-document `/Encoding` dict that layers a `/Differences` array
//! on top of `WinAnsiEncoding` to reach the 99 extended glyphs each
//! AFM carries beyond `WinAnsi` (`Ł`, `ł`, `Ě`, `ě`, `Ő`, `ő`, the
//! Romanian comma-below set, math operators `−≤≥≠√∂∑∆◊`, `fi`/`fl`).
//! A matching `/ToUnicode` `CMap` is emitted so the bytes we mint
//! decode back to real Unicode in copy/paste and search.
//!
//! See the private `encoding` module for the planner. PDF/A, tagged
//! PDF, hyperlinks, bookmarks, and full font embedding (issue #9)
//! are deferred.

mod embedded;
mod encoding;
mod images;

use std::collections::HashMap;
use std::path::Path;

use mosaic_core::{CoreError, Diagnostic, DiagnosticCode, DiagnosticNote, Result, Severity};
use mosaic_fonts::EmbeddedFontId;
use mosaic_layout::{Base14Font, Font, PageGraph, TextRun};
use pdf_writer::types::{SystemInfo, UnicodeCmap};
use pdf_writer::writers::Encoding;
use pdf_writer::{Content, Finish, Name, Pdf, Rect, Ref, Str, TextStr};

use crate::embedded::{EmbeddedFontPlan, EmbeddedRefs};
use crate::encoding::{DocEncoding, EncodingPlanner};

/// Document-level metadata that gets written to the PDF Info
/// dictionary. Populated by the lowerer from `#set document(...)`.
/// The `language` field is captured but not yet emitted (it belongs in
/// the catalog `/Lang` entry, which is the next slice).
#[derive(Debug, Clone, Default)]
pub struct PdfMetadata {
    pub title: Option<String>,
    pub author: Option<String>,
    pub language: Option<String>,
}

/// Emit `graph` as a PDF file at `out`. Creates `out`'s parent
/// directory if it doesn't already exist.
///
/// Returns any diagnostics raised during PDF emission — currently
/// only `W041` (per-font extended-glyph budget exhausted). Layout
/// diagnostics flow through [`mosaic_layout::LayoutResult::diagnostics`]
/// separately; callers (the CLI) typically render both.
///
/// # Errors
///
/// Returns a wrapped [`Diagnostic`] if writing the file (or creating
/// its parent directory) fails.
pub fn emit(graph: &PageGraph, metadata: &PdfMetadata, out: &Path) -> Result<Vec<Diagnostic>> {
    let (bytes, diagnostics) = build_pdf(graph, metadata)?;
    if let Some(parent) = out.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent).map_err(|err| {
            io_diagnostic(format!(
                "could not create output directory `{}`: {err}",
                parent.display()
            ))
        })?;
    }
    std::fs::write(out, bytes).map_err(|err| {
        io_diagnostic(format!("could not write PDF to `{}`: {err}", out.display()))
    })?;
    Ok(diagnostics)
}

fn io_diagnostic(message: String) -> CoreError {
    CoreError::Diagnostic(Box::new(Diagnostic {
        severity: Severity::Error,
        code: DiagnosticCode("E090"),
        message,
        span: None,
        notes: Vec::new(),
        suggestions: Vec::new(),
    }))
}

/// Build the PDF bytes from `graph`. Pulled out of [`emit`] so tests
/// can round-trip without touching the filesystem. Returns the bytes
/// plus any encoding diagnostics (currently `W041` for Base14
/// `/Differences` overflow). Kept `pub(crate)` — the public surface
/// is [`emit`].
///
/// # Errors
///
/// Returns an error if font subsetting fails for any embedded face
/// (only with corrupted font data; the bundled cuts have been
/// verified).
pub(crate) fn build_pdf(
    graph: &PageGraph,
    metadata: &PdfMetadata,
) -> Result<(Vec<u8>, Vec<Diagnostic>)> {
    // Phase 1a: scan every run and plan per-face Base14 /Differences
    // encodings (embedded-font runs are skipped — they take the Type 0
    // CID path below).
    let mut planner = EncodingPlanner::new();
    for page in &graph.pages {
        planner.observe_runs(&page.runs);
    }
    let mut diagnostics: Vec<Diagnostic> = Vec::new();
    let encodings = planner.finalize(&mut diagnostics);

    // Phase 1b: subset every embedded face actually used. One plan
    // per face referenced; absent if the face never appears in `runs`.
    // Only embedded-font runs need cloning into the flat slice the
    // planner consumes — Base14 runs would be filtered out by
    // `plan_embedded` anyway, so cloning them up front is pure waste
    // for documents where Base14 dominates.
    let embedded_runs: Vec<TextRun> = graph
        .pages
        .iter()
        .flat_map(|p| p.runs.iter())
        .filter(|r| matches!(r.font, Font::Embedded(_)))
        .cloned()
        .collect();
    let embedded_plans: Vec<EmbeddedFontPlan> = embedded::plan_embedded(&embedded_runs)?;
    let embedded_by_id: HashMap<EmbeddedFontId, &EmbeddedFontPlan> =
        embedded_plans.iter().map(|p| (p.id, p)).collect();

    // Phase 2: emit. Refs allocated up front so the page tree, font
    // dicts, encoding dicts, FontFile2 streams, and ToUnicode streams
    // can cross-reference.
    let mut pdf = Pdf::new();
    let mut next_id: i32 = 1;
    let mut alloc = || {
        let id = Ref::new(next_id);
        next_id += 1;
        id
    };

    let catalog_id = alloc();
    let page_tree_id = alloc();
    let info_id = alloc();

    // One indirect ref per Base14 face, in the order published by
    // `Font::ALL_BASE14`. Always all 14 entries so every page's
    // resource dictionary is identical for Base14 — preserves byte
    // stability for Base14-only documents.
    let base14_refs: Vec<(Font, Ref)> = Font::ALL_BASE14.iter().map(|f| (*f, alloc())).collect();

    // For each Latin face that needs a `/Differences` map, pre-allocate
    // the indirect refs for the custom encoding dict and the
    // `/ToUnicode` CMap stream. Symbol/Dingbats and unused faces get
    // no extra refs. Iterate `Font::ALL_BASE14` (not `&encodings`) so the
    // `alloc()` order — and therefore the byte layout of the produced
    // PDF — is deterministic across runs.
    let mut encoding_refs: HashMap<Font, (Ref, Ref)> = HashMap::new();
    for font in Font::ALL_BASE14 {
        if let Some(enc) = encodings.get(&font)
            && enc.has_differences()
        {
            let enc_ref = alloc();
            let cmap_ref = alloc();
            encoding_refs.insert(font, (enc_ref, cmap_ref));
        }
    }

    // One set of 5 refs per embedded face actually referenced.
    let embedded_refs: HashMap<EmbeddedFontId, EmbeddedRefs> = embedded_plans
        .iter()
        .map(|plan| {
            (
                plan.id,
                EmbeddedRefs {
                    font: alloc(),
                    cid_font: alloc(),
                    descriptor: alloc(),
                    font_file: alloc(),
                    to_unicode: alloc(),
                },
            )
        })
        .collect();

    // Allocate one indirect ref per unique image. Compression itself
    // happens at emit time (see the loop below) so we don't hold every
    // compressed stream in memory simultaneously — `graph.images` is
    // already the deduped set, and an image-heavy document can blow
    // peak RAM if we buffer all compressed copies before writing them.
    let image_refs: Vec<Ref> = graph.images.iter().map(|_| alloc()).collect();

    let page_refs: Vec<(Ref, Ref)> = graph.pages.iter().map(|_| (alloc(), alloc())).collect();

    pdf.catalog(catalog_id).pages(page_tree_id);

    let page_count = i32::try_from(page_refs.len()).unwrap_or(i32::MAX);
    pdf.pages(page_tree_id)
        .kids(page_refs.iter().map(|(p, _)| *p))
        .count(page_count);

    for (page, (page_id, content_id)) in graph.pages.iter().zip(page_refs.iter()) {
        let mut page_obj = pdf.page(*page_id);
        page_obj.media_box(Rect::new(0.0, 0.0, page.width_pt, page.height_pt));
        page_obj.parent(page_tree_id);
        page_obj.contents(*content_id);
        {
            let mut resources = page_obj.resources();
            {
                let mut fonts = resources.fonts();
                for (face, font_id) in &base14_refs {
                    fonts.pair(Name(face.pdf_resource_name()), *font_id);
                }
                // Embedded faces actually referenced in this document.
                // Each page's resource dict lists every embedded face used
                // anywhere in the document, not just on this page, so
                // resource dicts stay identical across pages. Iterate
                // `EmbeddedFontId::ALL` for deterministic order.
                for id in EmbeddedFontId::ALL {
                    if let Some(refs) = embedded_refs.get(&id) {
                        fonts.pair(Name(id.pdf_resource_name()), refs.font);
                    }
                }
            }
            // Image XObjects. Every page lists every image referenced
            // anywhere in the document so resource dicts stay byte-
            // stable across pages — same pattern as the font dicts.
            if !graph.images.is_empty() {
                let mut x_objects = resources.x_objects();
                for (handle, image_id) in graph.images.iter().zip(image_refs.iter()) {
                    let name = images::resource_name(handle);
                    x_objects.pair(Name(name.as_bytes()), *image_id);
                }
            }
        }
        page_obj.finish();

        let stream_bytes = build_content_stream(page.height_pt, page, &encodings, &embedded_by_id)?;
        pdf.stream(*content_id, &stream_bytes);
    }

    // Emit each Image XObject. Order matches `graph.images` (and
    // therefore the `alloc()` order above), keeping byte output
    // deterministic. Each image is compressed in this loop and the
    // compressed buffer dropped at the end of the iteration, so peak
    // memory holds at most one compressed image at a time on top of
    // the (Arc-shared) decoded pixel buffer the handle already owns.
    for (handle, id) in graph.images.iter().zip(image_refs.iter()) {
        let compressed = images::flate_compress(&handle.rgb8);
        images::emit_image_xobject(&mut pdf, *id, handle, &compressed);
    }

    for (face, font_id) in &base14_refs {
        let Some(base14) = face.base14() else {
            continue;
        };
        let mut font_dict = pdf.type1_font(*font_id);
        font_dict.base_font(Name(face.pdf_base_name().as_bytes()));
        // Symbol and ZapfDingbats use their own PostScript encodings;
        // overriding to WinAnsi would be a category error (Symbol's
        // `A` is Alpha). Skip /Encoding entirely for those.
        if matches!(base14, Base14Font::Symbol | Base14Font::ZapfDingbats) {
            continue;
        }
        match encoding_refs.get(face) {
            Some(&(enc_ref, cmap_ref)) => {
                // Custom /Encoding dict + /ToUnicode CMap (emitted
                // below at top level so the refs resolve).
                font_dict.pair(Name(b"Encoding"), enc_ref);
                font_dict.to_unicode(cmap_ref);
            }
            None => {
                // No extended glyphs needed for this face — the
                // standard WinAnsi shortcut suffices. PDF readers
                // default Type1 dicts to the font's built-in
                // encoding (StandardEncoding for Helvetica), so
                // declaring WinAnsi is required for bytes ≥ 0x80
                // (Euro, smart quotes, accented Latin, …) to render
                // the right glyph.
                font_dict.encoding_predefined(Name(b"WinAnsiEncoding"));
            }
        }
    }

    // Emit each embedded face's 5-object cluster (Type 0 + CIDFont +
    // descriptor + FontFile2 stream + ToUnicode CMap).
    for plan in &embedded_plans {
        let refs = embedded_refs[&plan.id];
        embedded::emit_embedded(&mut pdf, plan, refs);
    }

    // Emit the custom /Encoding dicts and /ToUnicode CMap streams.
    // Same `Font::ALL_BASE14` walk as the allocation pass above keeps
    // emit order deterministic.
    for font in Font::ALL_BASE14 {
        let Some(enc) = encodings.get(&font) else {
            continue;
        };
        let Some(&(enc_ref, cmap_ref)) = encoding_refs.get(&font) else {
            continue;
        };
        emit_encoding_dict(&mut pdf, enc_ref, enc);
        emit_to_unicode_cmap(&mut pdf, cmap_ref, enc);
    }

    {
        let mut info = pdf.document_info(info_id);
        if let Some(title) = metadata.title.as_deref() {
            info.title(TextStr(title));
        }
        if let Some(author) = metadata.author.as_deref() {
            info.author(TextStr(author));
        }
        info.finish();
    }

    Ok((pdf.finish(), diagnostics))
}

/// Emits one PDF indirect object: a custom `/Encoding` dict with
/// `/BaseEncoding /WinAnsiEncoding` and a `/Differences` array.
/// `pdf-writer`'s `Differences::consecutive(start, names)` emits the
/// run-length form `[ start /n1 /n2 /n3 ]`. We use one group per
/// contiguous run for compactness; isolated slots get their own
/// single-element group.
fn emit_encoding_dict(pdf: &mut Pdf, id: Ref, enc: &DocEncoding) {
    let mut enc_dict: Encoding<'_> = pdf.indirect(id).start();
    enc_dict.base_encoding(Name(b"WinAnsiEncoding"));
    {
        let mut diffs = enc_dict.differences();
        let mut i = 0;
        while i < enc.differences.len() {
            let (start, _) = enc.differences[i];
            // Find the end of this contiguous run (slot[j] == slot[j-1] + 1).
            let mut j = i + 1;
            while j < enc.differences.len() && enc.differences[j].0 == enc.differences[j - 1].0 + 1
            {
                j += 1;
            }
            let names = enc.differences[i..j]
                .iter()
                .map(|(_, n)| Name(n.as_bytes()));
            diffs.consecutive(start, names);
            i = j;
        }
    }
    enc_dict.finish();
}

/// Emits a `/ToUnicode` `CMap` stream that round-trips every byte
/// used by `enc` back to its original Unicode codepoint, so
/// copy-paste and full-text search work for both `WinAnsi` natives
/// and `/Differences`-remapped slots.
fn emit_to_unicode_cmap(pdf: &mut Pdf, id: Ref, enc: &DocEncoding) {
    // The `SystemInfo` here is embedded inside the PostScript-y CMap
    // stream content (the `%%BeginResource: CMap …` header that
    // `UnicodeCmap::new` writes). The `/CMapName` and `/CIDSystemInfo`
    // entries set further down go on the stream dictionary itself —
    // both are required by PDF 1.7 §9.7.5.4 / §9.10.3 (pdf-writer
    // documents `.name()` and `.system_info()` as "Required"), even
    // though readers we've tested tolerate their absence because the
    // PS content carries the same info.
    let system_info = SystemInfo {
        registry: Str(b"Adobe"),
        ordering: Str(b"UCS"),
        supplement: 0,
    };
    let mut cmap: UnicodeCmap<u8> = UnicodeCmap::new(Name(b"Adobe-Identity-UCS"), system_info);
    for &(byte, ch) in &enc.to_unicode_entries {
        cmap.pair(byte, ch);
    }
    let cmap_bytes = cmap.finish();
    let mut cmap_writer = pdf.cmap(id, &cmap_bytes);
    cmap_writer.name(Name(b"Adobe-Identity-UCS"));
    cmap_writer.system_info(system_info);
}

/// Build the per-page content stream. The layout engine measures
/// baselines from the **top** of the page; PDF's coordinate system is
/// bottom-origin, so we flip once here.
///
/// Base14 runs encode through the planner's per-face `DocEncoding`
/// (`WinAnsi` byte + `/Differences` remap; characters outside both
/// tiers silently render as `?`). Embedded-font runs encode the
/// shaped glyph stream as big-endian `u16` CIDs.
///
/// Image placements (raster `XObject`s) emit *outside* the text object
/// — `BT/ET` brackets only permit text operators, so each image
/// placement is wrapped in its own `q ... Q` save/restore pair before
/// the text block starts. Putting images first means subsequent text
/// can overlay (e.g. a caption beneath the image is unaffected, but
/// in-line annotations atop a figure would land on top).
fn build_content_stream(
    page_height_pt: f32,
    page: &mosaic_layout::Page,
    encodings: &HashMap<Font, DocEncoding>,
    embedded_by_id: &HashMap<EmbeddedFontId, &EmbeddedFontPlan>,
) -> Result<Vec<u8>> {
    let mut content = Content::new();
    for placement in &page.images {
        images::emit_placement(&mut content, page_height_pt, placement);
    }
    if page.runs.is_empty() {
        return Ok(content.finish().to_vec());
    }
    content.begin_text();
    let mut i = 0;
    while i < page.runs.len() {
        let run = &page.runs[i];
        if let Some(actual_text) = run.actual_text.as_deref() {
            {
                let mut marked = content.begin_marked_content_with_properties(Name(b"Span"));
                marked.properties().actual_text(TextStr(actual_text));
            }
            while i < page.runs.len() && page.runs[i].actual_text.as_deref() == Some(actual_text) {
                emit_text_run(
                    &mut content,
                    page_height_pt,
                    &page.runs[i],
                    encodings,
                    embedded_by_id,
                )?;
                i += 1;
            }
            content.end_marked_content();
        } else {
            emit_text_run(&mut content, page_height_pt, run, encodings, embedded_by_id)?;
            i += 1;
        }
    }
    content.end_text();
    Ok(content.finish().to_vec())
}

fn emit_text_run(
    content: &mut Content,
    page_height_pt: f32,
    run: &TextRun,
    encodings: &HashMap<Font, DocEncoding>,
    embedded_by_id: &HashMap<EmbeddedFontId, &EmbeddedFontPlan>,
) -> Result<()> {
    content.set_font(Name(run.font.pdf_resource_name()), run.size_pt);
    let y_from_bottom = page_height_pt - run.baseline_from_top_pt;
    content.set_text_matrix([1.0, 0.0, 0.0, 1.0, run.x_pt, y_from_bottom]);
    let bytes = match run.font {
        Font::Base14(_) => encode_base14_run(&run.text, run.font, encodings),
        Font::Embedded(id) => {
            let plan = embedded_by_id.get(&id).ok_or_else(|| {
                CoreError::Diagnostic(Box::new(Diagnostic {
                    severity: Severity::Error,
                    code: DiagnosticCode("E092"),
                    message: format!("missing embedded font plan for {:?} (id {id:?})", run.font),
                    span: None,
                    notes: vec![DiagnosticNote {
                        message:
                            "PDF emission expected an embedded plan for every embedded text run"
                                .to_owned(),
                        span: None,
                    }],
                    suggestions: Vec::new(),
                }))
            })?;
            embedded::encode_glyph_run(plan, &run.glyphs)
        }
    };
    content.show(Str(&bytes));
    Ok(())
}

/// Encode `text` against a Base14 face's `DocEncoding`. The planner
/// guarantees `byte_for_char` covers every `WinAnsi` native and every
/// extended Latin char that fit into the 256-slot budget; any char
/// outside both — Cyrillic, CJK, emoji — renders as `?`. Documents
/// that need real coverage should pick the bundled Noto Sans family
/// (the default; users hit this Base14 path only by explicitly asking
/// for `Helvetica`/`Times`/`Courier` via `#set text(font: ...)`).
fn encode_base14_run(text: &str, font: Font, encodings: &HashMap<Font, DocEncoding>) -> Vec<u8> {
    let map = encodings.get(&font).map(|e| &e.byte_for_char);
    let mut out = Vec::with_capacity(text.len());
    for ch in text.chars() {
        let byte = map
            .and_then(|m| m.get(&ch).copied())
            .or_else(|| mosaic_fonts::winansi_byte(ch))
            .unwrap_or(b'?');
        out.push(byte);
    }
    out
}

#[cfg(test)]
mod tests {
    // No `#![allow]` here. The two filesystem-touching tests
    // (`emit_writes_file`, `emit_fails_with_e090_when_target_is_a_directory`)
    // return `TestResult` and surface failures via `?` / `ensure!`
    // instead of `unwrap`/`expect`/`panic!`. The rest return `()`
    // and use plain `assert!`, which is not covered by
    // `clippy::panic`.
    use std::error::Error;

    use mosaic_layout::{Base14Font, Font, Page, PageGraph, TextRun};

    use super::*;

    // Explicit `std::result::Result` because the parent module
    // imports `mosaic_core::Result` which only takes one type
    // parameter.
    type TestResult = std::result::Result<(), Box<dyn Error>>;

    /// `assert!`-shaped helper that returns `Err` instead of
    /// panicking, so `-> TestResult` bodies stay clippy-clean under
    /// `clippy::panic_in_result_fn`. Mirrors the precedent in
    /// `pdf-base14-metrics/tests/winansi_vendor.rs` and the
    /// integration test at `tests/extended_latin_roundtrip.rs`.
    macro_rules! ensure {
        ($cond:expr, $($arg:tt)*) => {
            if !$cond {
                return Err(format!($($arg)*).into());
            }
        };
    }

    fn count_bytes(haystack: &[u8], needle: &[u8]) -> usize {
        haystack
            .windows(needle.len())
            .filter(|w| *w == needle)
            .count()
    }

    fn sample_graph() -> PageGraph {
        PageGraph {
            pages: vec![Page {
                number: 1,
                width_pt: 595.276_f32,
                height_pt: 841.89_f32,
                runs: vec![
                    TextRun {
                        x_pt: 68.0,
                        baseline_from_top_pt: 100.0,
                        size_pt: 20.0,
                        font: Font::Base14(Base14Font::HelveticaBold),
                        text: "Title".to_owned(),
                        actual_text: None,
                        glyphs: Vec::new(),
                    },
                    TextRun {
                        x_pt: 68.0,
                        baseline_from_top_pt: 130.0,
                        size_pt: 11.0,
                        font: Font::Base14(Base14Font::Helvetica),
                        text: "Body".to_owned(),
                        actual_text: None,
                        glyphs: Vec::new(),
                    },
                ],
                images: Vec::new(),
            }],
            images: Vec::new(),
        }
    }

    #[test]
    fn missing_embedded_plan_returns_diagnostic() -> TestResult {
        let face = EmbeddedFontId::Regular;
        let page = Page {
            number: 1,
            width_pt: 595.276_f32,
            height_pt: 841.89_f32,
            runs: vec![TextRun {
                x_pt: 68.0,
                baseline_from_top_pt: 100.0,
                size_pt: 12.0,
                font: Font::Embedded(face),
                text: "Body".to_owned(),
                actual_text: None,
                glyphs: mosaic_fonts::shape(face.data(), "Body"),
            }],
            images: Vec::new(),
        };

        let err = build_content_stream(
            page.height_pt,
            &page,
            &HashMap::new(),
            &HashMap::<EmbeddedFontId, &EmbeddedFontPlan>::new(),
        )
        .err()
        .ok_or("missing embedded plan unexpectedly succeeded")?;
        let diagnostic = match err {
            CoreError::Diagnostic(diagnostic) => diagnostic,
            other => return Err(format!("expected diagnostic error, got {other:?}").into()),
        };
        ensure!(
            diagnostic.code.0 == "E092",
            "wrong code: {:?}",
            diagnostic.code.0
        );
        ensure!(
            diagnostic.message.contains("Embedded(Regular)")
                && diagnostic.message.contains("Regular"),
            "missing context in message: {:?}",
            diagnostic.message
        );
        Ok(())
    }

    #[test]
    fn build_pdf_starts_with_pdf_header_and_ends_with_eof() {
        let (bytes, diags) = build_pdf(&sample_graph(), &PdfMetadata::default()).unwrap();
        assert!(bytes.starts_with(b"%PDF-"), "missing PDF header");
        assert!(
            bytes.windows(5).any(|w| w == b"%%EOF"),
            "missing %%EOF marker"
        );
        assert!(diags.is_empty(), "unexpected diagnostics: {diags:?}");
    }

    #[test]
    fn build_pdf_embeds_text_runs_as_visible_strings() {
        let (bytes, _) = build_pdf(&sample_graph(), &PdfMetadata::default()).unwrap();
        // The Str writer emits ASCII inside `(...)` so we can grep
        // the raw bytes for the visible payload.
        assert!(
            bytes.windows(b"(Title)".len()).any(|w| w == b"(Title)"),
            "Title not found in stream"
        );
        assert!(
            bytes.windows(b"(Body)".len()).any(|w| w == b"(Body)"),
            "Body not found in stream"
        );
    }

    #[test]
    fn empty_graph_still_produces_valid_pdf() {
        let (bytes, _) = build_pdf(&PageGraph::default(), &PdfMetadata::default()).unwrap();
        assert!(bytes.starts_with(b"%PDF-"));
    }

    #[test]
    fn metadata_appears_in_info_dictionary() {
        let metadata = PdfMetadata {
            title: Some("My Doc".to_owned()),
            author: Some("A. Person".to_owned()),
            language: None,
        };
        let (bytes, _) = build_pdf(&sample_graph(), &metadata).unwrap();
        assert!(
            bytes.windows(b"(My Doc)".len()).any(|w| w == b"(My Doc)"),
            "title not found in PDF"
        );
        assert!(
            bytes
                .windows(b"(A. Person)".len())
                .any(|w| w == b"(A. Person)"),
            "author not found in PDF"
        );
    }

    #[test]
    fn actual_text_is_emitted_for_replacement_runs() {
        let graph = PageGraph {
            pages: vec![Page {
                number: 1,
                width_pt: 595.276_f32,
                height_pt: 841.89_f32,
                runs: vec![TextRun {
                    x_pt: 68.0,
                    baseline_from_top_pt: 100.0,
                    size_pt: 12.0,
                    font: Font::Base14(Base14Font::Courier),
                    text: "    println".to_owned(),
                    actual_text: Some("\tprintln".to_owned()),
                    glyphs: Vec::new(),
                }],
                images: Vec::new(),
            }],
            images: Vec::new(),
        };

        let (bytes, diags) = build_pdf(&graph, &PdfMetadata::default()).unwrap();

        assert!(diags.is_empty(), "unexpected diagnostics: {diags:?}");
        assert!(
            bytes
                .windows(b"/ActualText".len())
                .any(|w| w == b"/ActualText"),
            "missing /ActualText"
        );
        assert!(
            bytes.windows(b"println".len()).any(|w| w == b"println"),
            "actual text payload missing"
        );
    }

    #[test]
    fn actual_text_wraps_adjacent_fragments_once() {
        let graph = PageGraph {
            pages: vec![Page {
                number: 1,
                width_pt: 595.276_f32,
                height_pt: 841.89_f32,
                runs: vec![
                    TextRun {
                        x_pt: 68.0,
                        baseline_from_top_pt: 100.0,
                        size_pt: 12.0,
                        font: Font::Base14(Base14Font::Courier),
                        text: "    ".to_owned(),
                        actual_text: Some("\tprintln".to_owned()),
                        glyphs: Vec::new(),
                    },
                    TextRun {
                        x_pt: 92.0,
                        baseline_from_top_pt: 100.0,
                        size_pt: 12.0,
                        font: Font::Base14(Base14Font::CourierBold),
                        text: "println".to_owned(),
                        actual_text: Some("\tprintln".to_owned()),
                        glyphs: Vec::new(),
                    },
                ],
                images: Vec::new(),
            }],
            images: Vec::new(),
        };

        let (bytes, diags) = build_pdf(&graph, &PdfMetadata::default()).unwrap();

        assert!(diags.is_empty(), "unexpected diagnostics: {diags:?}");
        assert_eq!(count_bytes(&bytes, b"/ActualText"), 1);
        assert_eq!(count_bytes(&bytes, b"println"), 1);
    }

    /// A graph containing Polish + Czech text — exercises the
    /// `/Differences` and `/ToUnicode` emit paths end to end.
    fn extended_latin_graph() -> PageGraph {
        PageGraph {
            pages: vec![Page {
                number: 1,
                width_pt: 595.276_f32,
                height_pt: 841.89_f32,
                runs: vec![TextRun {
                    x_pt: 68.0,
                    baseline_from_top_pt: 100.0,
                    size_pt: 12.0,
                    font: Font::Base14(Base14Font::Helvetica),
                    text: "Łódź Příliš ě".to_owned(),
                    actual_text: None,
                    glyphs: Vec::new(),
                }],
                images: Vec::new(),
            }],
            images: Vec::new(),
        }
    }

    #[test]
    fn extended_latin_emits_differences_and_to_unicode() {
        let (bytes, diags) = build_pdf(&extended_latin_graph(), &PdfMetadata::default()).unwrap();
        assert!(diags.is_empty(), "unexpected diagnostics: {diags:?}");
        // The /Encoding dict carries /BaseEncoding /WinAnsiEncoding.
        assert!(
            bytes
                .windows(b"/BaseEncoding /WinAnsiEncoding".len())
                .any(|w| w == b"/BaseEncoding /WinAnsiEncoding"),
            "missing /BaseEncoding"
        );
        // The /Differences array contains the AFM glyph names for the
        // non-WinAnsi codepoints in the sample: Ł→Lslash, ř→rcaron,
        // ě→ecaron, ź→zacute. (ó/d/í/l/i/š are WinAnsi natives, so
        // they don't show up in /Differences.)
        for name in [b"/Lslash" as &[u8], b"/rcaron", b"/ecaron", b"/zacute"] {
            assert!(
                bytes.windows(name.len()).any(|w| w == name),
                "missing {:?} in /Differences",
                std::str::from_utf8(name).unwrap_or("?")
            );
        }
        // A /ToUnicode CMap was emitted.
        assert!(
            bytes
                .windows(b"/ToUnicode".len())
                .any(|w| w == b"/ToUnicode"),
            "missing /ToUnicode reference"
        );
        assert!(
            bytes
                .windows(b"beginbfchar".len())
                .any(|w| w == b"beginbfchar"),
            "missing beginbfchar in CMap"
        );
    }

    #[test]
    fn pure_ascii_graph_keeps_predefined_winansi_shortcut() {
        // Existing sample_graph() is pure ASCII — no /Differences
        // should be emitted, the predefined WinAnsi shortcut path is
        // exercised. This guards against accidental "always emit a
        // custom encoding" regressions that would balloon every PDF.
        let (bytes, _) = build_pdf(&sample_graph(), &PdfMetadata::default()).unwrap();
        assert!(
            bytes
                .windows(b"/Encoding /WinAnsiEncoding".len())
                .any(|w| w == b"/Encoding /WinAnsiEncoding"),
            "expected predefined WinAnsi shortcut on ASCII-only doc"
        );
        assert!(
            !bytes
                .windows(b"/BaseEncoding".len())
                .any(|w| w == b"/BaseEncoding"),
            "no custom /Encoding dict expected for ASCII-only doc"
        );
    }

    #[test]
    fn extended_latin_content_stream_uses_remapped_bytes() {
        // Polish "Ł" lands in the first gap slot (0x7F) by the
        // allocator's deterministic order. The run also contains
        // Latin-1 bytes ≥ 0x80 (`ó`, `í`, …) so pdf-writer switches
        // the string from literal `(...)` to hex `<...>` form;
        // 0x7F therefore appears in the document as the ASCII pair
        // `7F`. This is a smoke check that the encoder routed Ł to
        // a remapped slot rather than substituting `?` (0x3F).
        //
        // Both assertions operate on the page content stream slice
        // only — scanning the whole PDF would let the `/ToUnicode`
        // CMap (`<7F> <0141>`) satisfy the `7F` needle even if the
        // content stream had silently substituted to `?`. Surgical
        // slicing keeps the smoke test honest.
        let (bytes, _) = build_pdf(&extended_latin_graph(), &PdfMetadata::default()).unwrap();
        let content_stream = first_content_stream(&bytes).expect("content stream not found");
        let needle = b"7F";
        assert!(
            content_stream.windows(needle.len()).any(|w| w == needle),
            "content stream should reference remapped slot 0x7F"
        );
        let qmark_count = content_stream.iter().filter(|&&b| b == b'?').count();
        assert!(
            qmark_count < 5,
            "too many `?` in PDF ({qmark_count}); did Ł/ř/ě/ź get substituted?"
        );
    }

    #[test]
    fn build_pdf_is_byte_for_byte_deterministic() {
        // Regression guard for the HashMap-iteration-order bug that
        // shuffled indirect IDs between builds. Two `build_pdf` calls
        // on the same graph must produce identical bytes; otherwise
        // golden tests and reproducible CI artifacts break.
        let (a, _) = build_pdf(&extended_latin_graph(), &PdfMetadata::default()).unwrap();
        let (b, _) = build_pdf(&extended_latin_graph(), &PdfMetadata::default()).unwrap();
        assert_eq!(
            a,
            b,
            "build_pdf is non-deterministic: byte lengths {} vs {}",
            a.len(),
            b.len()
        );
    }

    /// Locate the first `stream` ... `endstream` body in a PDF byte
    /// blob and return the bytes between them. `build_pdf` emits the
    /// page content stream before any `/ToUnicode` `CMap` stream
    /// (see the object-order comment in [`build_pdf`]), so the first
    /// match is always the page content. Markers anchor on the
    /// surrounding `\n` so the substring inside `endstream` doesn't
    /// false-match the opener.
    fn first_content_stream(bytes: &[u8]) -> Option<&[u8]> {
        let open = b"\nstream\n";
        let close = b"\nendstream";
        let open_at = bytes.windows(open.len()).position(|w| w == open)?;
        let body = &bytes[open_at + open.len()..];
        let close_at = body.windows(close.len()).position(|w| w == close)?;
        Some(&body[..close_at])
    }

    fn unique_temp_path(label: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "mosaic-pdf-test-{label}-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos())
        ))
    }

    #[test]
    fn emit_writes_file() -> TestResult {
        let dir = unique_temp_path("write");
        let out = dir.join("out.pdf");
        let diags = emit(&sample_graph(), &PdfMetadata::default(), &out)
            .map_err(|e| format!("emit: {e:?}"))?;
        ensure!(diags.is_empty(), "unexpected diagnostics: {diags:?}");
        let bytes = std::fs::read(&out)?;
        ensure!(bytes.starts_with(b"%PDF-"), "missing PDF header");
        std::fs::remove_dir_all(&dir).ok();
        Ok(())
    }

    /// Build a graph with one image: a 4×2 red-and-blue checker
    /// flattened to RGB8, sized at 40×20 pt. Reused across multiple
    /// emit tests below.
    fn image_graph() -> PageGraph {
        use mosaic_layout::{ImageHandle, ImagePlacement};
        use std::sync::Arc;
        // 4 columns × 2 rows; alternating red/blue cells.
        let mut rgb8 = Vec::with_capacity(4 * 2 * 3);
        for y in 0..2 {
            for x in 0..4 {
                if (x + y) % 2 == 0 {
                    rgb8.extend_from_slice(&[255, 0, 0]);
                } else {
                    rgb8.extend_from_slice(&[0, 0, 255]);
                }
            }
        }
        let handle = ImageHandle {
            id: 0,
            resolved_path: "/tmp/checker.png".to_owned(),
            pixel_width: 4,
            pixel_height: 2,
            rgb8: Arc::from(rgb8),
        };
        PageGraph {
            pages: vec![Page {
                number: 1,
                width_pt: 595.276_f32,
                height_pt: 841.89_f32,
                runs: Vec::new(),
                images: vec![ImagePlacement {
                    handle: handle.clone(),
                    x_pt: 68.0,
                    top_from_top_pt: 100.0,
                    width_pt: 40.0,
                    height_pt: 20.0,
                }],
            }],
            images: vec![handle],
        }
    }

    #[test]
    fn image_xobject_carries_width_height_and_devicergb() {
        let (bytes, diags) = build_pdf(&image_graph(), &PdfMetadata::default()).unwrap();
        assert!(diags.is_empty(), "unexpected diagnostics: {diags:?}");
        // The Image XObject must declare /Subtype /Image, /Width 4,
        // /Height 2, /ColorSpace /DeviceRGB, /BitsPerComponent 8, and
        // /Filter /FlateDecode.
        for needle in [
            b"/Subtype /Image" as &[u8],
            b"/Width 4",
            b"/Height 2",
            b"/ColorSpace /DeviceRGB",
            b"/BitsPerComponent 8",
            b"/Filter /FlateDecode",
        ] {
            assert!(
                bytes.windows(needle.len()).any(|w| w == needle),
                "missing {:?} in PDF",
                std::str::from_utf8(needle).unwrap_or("?")
            );
        }
    }

    #[test]
    fn image_placement_emits_do_operator_referencing_xobject() {
        let (bytes, _) = build_pdf(&image_graph(), &PdfMetadata::default()).unwrap();
        // The page's resource dict must list /Im0; the content stream
        // must reference /Im0 via the Do operator.
        assert!(
            bytes.windows(b"/Im0 ".len()).any(|w| w == b"/Im0 "),
            "/Im0 resource name not found"
        );
        assert!(
            bytes.windows(b"/Im0 Do".len()).any(|w| w == b"/Im0 Do"),
            "/Im0 Do operator not found in content stream"
        );
    }

    #[test]
    fn duplicate_image_emits_one_xobject() {
        // Two placements of the same image should still produce one
        // shared XObject — the layout pass already dedup'd them, so the
        // PDF backend never sees two ImageHandle entries.
        use mosaic_layout::{ImageHandle, ImagePlacement};
        use std::sync::Arc;
        let handle = ImageHandle {
            id: 0,
            resolved_path: "/tmp/shared.png".to_owned(),
            pixel_width: 1,
            pixel_height: 1,
            rgb8: Arc::from(vec![10_u8, 20, 30]),
        };
        let graph = PageGraph {
            pages: vec![Page {
                number: 1,
                width_pt: 595.276_f32,
                height_pt: 841.89_f32,
                runs: Vec::new(),
                images: vec![
                    ImagePlacement {
                        handle: handle.clone(),
                        x_pt: 10.0,
                        top_from_top_pt: 50.0,
                        width_pt: 5.0,
                        height_pt: 5.0,
                    },
                    ImagePlacement {
                        handle: handle.clone(),
                        x_pt: 100.0,
                        top_from_top_pt: 50.0,
                        width_pt: 5.0,
                        height_pt: 5.0,
                    },
                ],
            }],
            images: vec![handle],
        };
        let (bytes, _) = build_pdf(&graph, &PdfMetadata::default()).unwrap();
        let xobject_marker = b"/Subtype /Image";
        let count = bytes
            .windows(xobject_marker.len())
            .filter(|w| *w == xobject_marker)
            .count();
        assert_eq!(count, 1, "expected exactly one Image XObject, got {count}");
        // Both placements show up as /Im0 Do.
        let do_count = bytes
            .windows(b"/Im0 Do".len())
            .filter(|w| *w == b"/Im0 Do")
            .count();
        assert_eq!(
            do_count, 2,
            "expected two /Im0 Do operators, got {do_count}"
        );
    }

    #[test]
    fn image_only_pdf_remains_byte_deterministic() {
        let (a, _) = build_pdf(&image_graph(), &PdfMetadata::default()).unwrap();
        let (b, _) = build_pdf(&image_graph(), &PdfMetadata::default()).unwrap();
        assert_eq!(a, b, "image emit must be byte-stable across runs");
    }

    #[test]
    fn emit_fails_with_e090_when_target_is_a_directory() -> TestResult {
        // Writing a file whose path collides with an existing
        // directory must surface as an `E090` diagnostic, not a
        // panic or an `Unimplemented` error.
        let dir = unique_temp_path("conflict");
        std::fs::create_dir_all(&dir)?;
        // `dir` itself is the bogus output target; `fs::write` will
        // refuse to overwrite a directory.
        let result = emit(&sample_graph(), &PdfMetadata::default(), &dir);
        std::fs::remove_dir_all(&dir).ok();
        let Err(err) = result else {
            return Err("expected emit to fail when target is a directory".into());
        };
        let CoreError::Diagnostic(d) = err else {
            return Err("expected Diagnostic, got Unimplemented".into());
        };
        ensure!(d.code.0 == "E090", "wrong code: {:?}", d.code.0);
        ensure!(
            d.message.contains("could not write PDF"),
            "message={:?}",
            d.message
        );
        Ok(())
    }
}
