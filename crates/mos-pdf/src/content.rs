//! Per-page PDF content stream emission.

use std::collections::HashMap;

use mos_core::{CoreError, Diagnostic, DiagnosticCode, DiagnosticNote, Result, Severity};
use mos_fonts::EmbeddedFontId;
use mos_layout::{Font, TextRun};
use pdf_writer::{Content, Name, Str, TextStr};

use crate::embedded::{self, EmbeddedFontPlan};
use crate::encoding::DocEncoding;
use crate::images;

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
pub(crate) fn build_content_stream(
    page_height_pt: f32,
    page: &mos_layout::Page,
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
            .or_else(|| mos_fonts::winansi_byte(ch))
            .unwrap_or(b'?');
        out.push(byte);
    }
    out
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::error::Error;

    use mos_layout::{Page, TextRun};

    use super::*;

    type TestResult = std::result::Result<(), Box<dyn Error>>;

    macro_rules! ensure {
        ($cond:expr, $($arg:tt)*) => {
            if !$cond {
                return Err(format!($($arg)*).into());
            }
        };
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
                glyphs: mos_fonts::shape(face.data(), "Body"),
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
}
