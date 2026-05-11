//! PDF backend for Mosaic (manifest §21.1).
//!
//! MVP 0 emits a fixed-A4 PDF declaring all 14 standard PDF base fonts (Helvetica/Times/Courier
//! families + Symbol + ZapfDingbats) so we don't need font subsetting or embedding.
//! Tagged PDF, PDF/A, hyperlinks, bookmarks, and font embedding are deferred.

use std::path::Path;

use mosaic_core::{CoreError, Diagnostic, DiagnosticCode, Result, Severity};
use mosaic_layout::{Font, PageGraph, TextRun};
use pdf_writer::{Content, Finish, Name, Pdf, Rect, Ref, Str, TextStr};

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
/// # Errors
///
/// Returns a wrapped [`Diagnostic`] if writing the file (or creating
/// its parent directory) fails. Layout-level issues are surfaced
/// through [`mosaic_layout::LayoutResult::diagnostics`] instead, so
/// this function only fails on I/O problems.
pub fn emit(graph: &PageGraph, metadata: &PdfMetadata, out: &Path) -> Result<()> {
    let bytes = build_pdf(graph, metadata);
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
    Ok(())
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
/// can round-trip without touching the filesystem. Kept `pub(crate)`
/// until there's a real consumer — the public surface is [`emit`].
pub(crate) fn build_pdf(graph: &PageGraph, metadata: &PdfMetadata) -> Vec<u8> {
    let mut pdf = Pdf::new();
    let mut next_id: i32 = 1;
    let mut alloc = || {
        let id = Ref::new(next_id);
        next_id += 1;
        id
    };

    let catalog_id = alloc();
    let page_tree_id = alloc();
    // Allocate the info ref up front so we can reference it from the
    // trailer; the actual dictionary is written below.
    let info_id = alloc();

    // One indirect ref per font face, in the order published by
    // `Font::ALL`. We always emit all 14 entries so every page's
    // resource dictionary is identical, simplifying the writer.
    let font_refs: Vec<(Font, Ref)> = Font::ALL.iter().map(|f| (*f, alloc())).collect();

    // Per-page (page object, content stream) refs.
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

        let stream_bytes = build_content_stream(page.height_pt, &page.runs);
        pdf.stream(*content_id, &stream_bytes);
    }

    for (face, font_id) in &font_refs {
        pdf.type1_font(*font_id)
            .base_font(Name(face.pdf_base_name().as_bytes()));
    }

    // Info dictionary: only emit fields we actually have. `pdf-writer`
    // requires UTF-8-clean strings here; the lowerer already trims
    // surrounding whitespace.
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

    pdf.finish()
}

/// Build the per-page content stream. The layout engine measures
/// baselines from the **top** of the page; PDF's coordinate system is
/// bottom-origin, so we flip once here.
fn build_content_stream(page_height_pt: f32, runs: &[TextRun]) -> Vec<u8> {
    let mut content = Content::new();
    if runs.is_empty() {
        return content.finish().to_vec();
    }
    content.begin_text();
    for run in runs {
        content.set_font(Name(run.font.pdf_resource_name()), run.size_pt);
        // Identity rotation/scaling, translate to (x, page_height - baseline).
        let y_from_bottom = page_height_pt - run.baseline_from_top_pt;
        content.set_text_matrix([1.0, 0.0, 0.0, 1.0, run.x_pt, y_from_bottom]);
        content.show(Str(run.text.as_bytes()));
    }
    content.end_text();
    content.finish().to_vec()
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        reason = "tests panic loudly on setup failure; matches crate-wide test-module convention"
    )]
    use mosaic_layout::{Base14Font, Font, Page, PageGraph, TextRun};

    use super::*;

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
        let bytes = build_pdf(&sample_graph(), &PdfMetadata::default());
        assert!(bytes.starts_with(b"%PDF-"), "missing PDF header");
        assert!(
            bytes.windows(5).any(|w| w == b"%%EOF"),
            "missing %%EOF marker"
        );
    }

    #[test]
    fn build_pdf_embeds_text_runs_as_visible_strings() {
        let bytes = build_pdf(&sample_graph(), &PdfMetadata::default());
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
        let bytes = build_pdf(&PageGraph::default(), &PdfMetadata::default());
        assert!(bytes.starts_with(b"%PDF-"));
    }

    #[test]
    fn metadata_appears_in_info_dictionary() {
        let metadata = PdfMetadata {
            title: Some("My Doc".to_owned()),
            author: Some("A. Person".to_owned()),
            language: None,
        };
        let bytes = build_pdf(&sample_graph(), &metadata);
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

    fn unique_temp_path(label: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "mosaic-pdf-test-{label}-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos())
        ))
    }

    #[test]
    fn emit_writes_file() {
        let dir = unique_temp_path("write");
        let out = dir.join("out.pdf");
        emit(&sample_graph(), &PdfMetadata::default(), &out).expect("emit");
        let bytes = std::fs::read(&out).expect("read pdf");
        assert!(bytes.starts_with(b"%PDF-"));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn emit_fails_with_e090_when_target_is_a_directory() {
        // Writing a file whose path collides with an existing
        // directory must surface as an `E090` diagnostic, not a
        // panic or an `Unimplemented` error.
        let dir = unique_temp_path("conflict");
        std::fs::create_dir_all(&dir).expect("mkdir");
        // `dir` itself is the bogus output target; `fs::write` will
        // refuse to overwrite a directory.
        let result = emit(&sample_graph(), &PdfMetadata::default(), &dir);
        std::fs::remove_dir_all(&dir).ok();
        let err = result.expect_err("expected emit to fail");
        let CoreError::Diagnostic(d) = err else {
            panic!("expected Diagnostic, got Unimplemented");
        };
        assert_eq!(d.code.0, "E090");
        assert!(
            d.message.contains("could not write PDF"),
            "message={:?}",
            d.message
        );
    }
}
