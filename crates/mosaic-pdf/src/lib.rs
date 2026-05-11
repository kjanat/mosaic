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

mod encoding;

use std::collections::HashMap;
use std::path::Path;

use mosaic_core::{CoreError, Diagnostic, DiagnosticCode, Result, Severity};
use mosaic_layout::{Base14Font, Font, PageGraph, TextRun};
use pdf_writer::types::{SystemInfo, UnicodeCmap};
use pdf_writer::writers::Encoding;
use pdf_writer::{Content, Finish, Name, Pdf, Rect, Ref, Str, TextStr};

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
    let (bytes, diagnostics) = build_pdf(graph, metadata);
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
/// plus any encoding diagnostics (currently only `W041`). Kept
/// `pub(crate)` — the public surface is [`emit`].
pub(crate) fn build_pdf(graph: &PageGraph, metadata: &PdfMetadata) -> (Vec<u8>, Vec<Diagnostic>) {
    // Phase 1: scan every run and plan per-face encodings.
    let mut planner = EncodingPlanner::new();
    for page in &graph.pages {
        planner.observe_runs(&page.runs);
    }
    let mut diagnostics: Vec<Diagnostic> = Vec::new();
    let encodings = planner.finalize(&mut diagnostics);

    // Phase 2: emit. Refs allocated up front so the page tree, font
    // dicts, encoding dicts, and ToUnicode streams can cross-reference.
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

    // One indirect ref per font face, in the order published by
    // `Font::ALL`. We always emit all 14 entries so every page's
    // resource dictionary is identical, simplifying the writer.
    let font_refs: Vec<(Font, Ref)> = Font::ALL.iter().map(|f| (*f, alloc())).collect();

    // For each Latin face that needs a `/Differences` map, pre-allocate
    // the indirect refs for the custom encoding dict and the
    // `/ToUnicode` CMap stream. Symbol/Dingbats and unused faces get
    // no extra refs.
    let mut encoding_refs: HashMap<Font, (Ref, Ref)> = HashMap::new();
    for (font, enc) in &encodings {
        if enc.has_differences() {
            let enc_ref = alloc();
            let cmap_ref = alloc();
            encoding_refs.insert(*font, (enc_ref, cmap_ref));
        }
    }

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
            let mut fonts = resources.fonts();
            for (face, font_id) in &font_refs {
                fonts.pair(Name(face.pdf_resource_name()), *font_id);
            }
        }
        page_obj.finish();

        let stream_bytes = build_content_stream(page.height_pt, &page.runs, &encodings);
        pdf.stream(*content_id, &stream_bytes);
    }

    for (face, font_id) in &font_refs {
        let mut font_dict = pdf.type1_font(*font_id);
        font_dict.base_font(Name(face.pdf_base_name().as_bytes()));
        // Symbol and ZapfDingbats use their own PostScript encodings;
        // overriding to WinAnsi would be a category error (Symbol's
        // `A` is Alpha). Skip /Encoding entirely for those.
        if matches!(face.0, Base14Font::Symbol | Base14Font::ZapfDingbats) {
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

    // Emit the custom /Encoding dicts and /ToUnicode CMap streams.
    for (font, enc) in &encodings {
        let Some(&(enc_ref, cmap_ref)) = encoding_refs.get(font) else {
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

    (pdf.finish(), diagnostics)
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
    let mut cmap: UnicodeCmap<u8> = UnicodeCmap::new(
        Name(b"Adobe-Identity-UCS"),
        SystemInfo {
            registry: Str(b"Adobe"),
            ordering: Str(b"UCS"),
            supplement: 0,
        },
    );
    for &(byte, ch) in &enc.to_unicode_entries {
        cmap.pair(byte, ch);
    }
    pdf.cmap(id, &cmap.finish());
}

/// Build the per-page content stream. The layout engine measures
/// baselines from the **top** of the page; PDF's coordinate system is
/// bottom-origin, so we flip once here. Each run gets encoded against
/// the planner's per-face `DocEncoding`, so `WinAnsi` natives go out
/// as their native byte and extended chars go out as remapped slots.
fn build_content_stream(
    page_height_pt: f32,
    runs: &[TextRun],
    encodings: &HashMap<Font, DocEncoding>,
) -> Vec<u8> {
    let mut content = Content::new();
    if runs.is_empty() {
        return content.finish().to_vec();
    }
    content.begin_text();
    for run in runs {
        content.set_font(Name(run.font.pdf_resource_name()), run.size_pt);
        let y_from_bottom = page_height_pt - run.baseline_from_top_pt;
        content.set_text_matrix([1.0, 0.0, 0.0, 1.0, run.x_pt, y_from_bottom]);
        let bytes = encode_run(&run.text, run.font, encodings);
        content.show(Str(&bytes));
    }
    content.end_text();
    content.finish().to_vec()
}

/// Encode `text` against `font`'s `DocEncoding`. Every char is
/// guaranteed mappable by `mosaic-layout::sanitize_text` upstream,
/// which substitutes unmappable codepoints to `?`. The planner
/// further guarantees `byte_for_char` covers every `WinAnsi` native
/// and every extended char that fit into the 256-slot budget; any
/// char missing from the map is overflow that we render as `?`
/// (already reported via W041).
fn encode_run(text: &str, font: Font, encodings: &HashMap<Font, DocEncoding>) -> Vec<u8> {
    let map = encodings.get(&font).map(|e| &e.byte_for_char);
    let mut out = Vec::with_capacity(text.len());
    for ch in text.chars() {
        // Symbol/Dingbats don't go through the planner; fall back to
        // a direct WinAnsi byte for any char that happens to round-trip
        // (mostly ASCII), and to `?` otherwise. In practice the layout
        // engine never routes text into those faces today.
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
                        font: Font(Base14Font::HelveticaBold),
                        text: "Title".to_owned(),
                    },
                    TextRun {
                        x_pt: 68.0,
                        baseline_from_top_pt: 130.0,
                        size_pt: 11.0,
                        font: Font(Base14Font::Helvetica),
                        text: "Body".to_owned(),
                    },
                ],
            }],
        }
    }

    #[test]
    fn build_pdf_starts_with_pdf_header_and_ends_with_eof() {
        let (bytes, diags) = build_pdf(&sample_graph(), &PdfMetadata::default());
        assert!(bytes.starts_with(b"%PDF-"), "missing PDF header");
        assert!(
            bytes.windows(5).any(|w| w == b"%%EOF"),
            "missing %%EOF marker"
        );
        assert!(diags.is_empty(), "unexpected diagnostics: {diags:?}");
    }

    #[test]
    fn build_pdf_embeds_text_runs_as_visible_strings() {
        let (bytes, _) = build_pdf(&sample_graph(), &PdfMetadata::default());
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
        let (bytes, _) = build_pdf(&PageGraph::default(), &PdfMetadata::default());
        assert!(bytes.starts_with(b"%PDF-"));
    }

    #[test]
    fn metadata_appears_in_info_dictionary() {
        let metadata = PdfMetadata {
            title: Some("My Doc".to_owned()),
            author: Some("A. Person".to_owned()),
            language: None,
        };
        let (bytes, _) = build_pdf(&sample_graph(), &metadata);
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
                    font: Font(Base14Font::Helvetica),
                    text: "Łódź Příliš ě".to_owned(),
                }],
            }],
        }
    }

    #[test]
    fn extended_latin_emits_differences_and_to_unicode() {
        let (bytes, diags) = build_pdf(&extended_latin_graph(), &PdfMetadata::default());
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
        let (bytes, _) = build_pdf(&sample_graph(), &PdfMetadata::default());
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
        let (bytes, _) = build_pdf(&extended_latin_graph(), &PdfMetadata::default());
        // Look for the hex digraph `7F` inside any hex string in the
        // file. We don't try to be surgical about which hex string;
        // a false positive across object boundaries is acceptable
        // for a smoke test of this size.
        let needle = b"7F";
        assert!(
            bytes.windows(needle.len()).any(|w| w == needle),
            "content stream should reference remapped slot 0x7F"
        );
        // And the run must NOT have been all-substituted to `?`s
        // (0x3F repeated). If `?` count >= run length, something
        // collapsed.
        let qmark_count = bytes.iter().filter(|&&b| b == b'?').count();
        assert!(
            qmark_count < 5,
            "too many `?` in PDF ({qmark_count}); did Ł/ř/ě/ź get substituted?"
        );
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
