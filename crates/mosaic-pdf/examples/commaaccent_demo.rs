//! One-off demo: emit a PDF with two Romanian lines side by side —
//! one using the comma-below glyphs (`Ș`/`ș`/`Ț`/`ț` via U+0218..U+021B
//! → `Scommaaccent`/`scommaaccent`/`Tcommaaccent`/`tcommaaccent` in
//! the Helvetica AFM, reachable through the per-document
//! `/Differences` planner) and one with the diacritics stripped to
//! ASCII (`S`/`s`/`T`/`t`) for visual comparison.

use mosaic_layout::{Base14Font, Font, Page, PageGraph, TextRun};
use mosaic_pdf::PdfMetadata;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let graph = PageGraph {
        pages: vec![Page {
            number: 1,
            width_pt: 595.276_f32, // A4 width
            height_pt: 841.89_f32, // A4 height
            runs: vec![
                TextRun {
                    x_pt: 68.0,
                    baseline_from_top_pt: 90.0,
                    size_pt: 16.0,
                    font: Font(Base14Font::HelveticaBold),
                    text: "Romanian comma-below demo".to_owned(),
                },
                TextRun {
                    x_pt: 68.0,
                    baseline_from_top_pt: 130.0,
                    size_pt: 11.0,
                    font: Font(Base14Font::Helvetica),
                    text: "With comma-below (correct Romanian):".to_owned(),
                },
                TextRun {
                    x_pt: 68.0,
                    baseline_from_top_pt: 152.0,
                    size_pt: 18.0,
                    font: Font(Base14Font::TimesRoman),
                    // Ș=U+0218, ș=U+0219, Ț=U+021A, ț=U+021B
                    text: "Șapte șopârle țipă în țară.".to_owned(),
                },
                TextRun {
                    x_pt: 68.0,
                    baseline_from_top_pt: 200.0,
                    size_pt: 11.0,
                    font: Font(Base14Font::Helvetica),
                    text: "Without (ASCII fallback, no diacritic):".to_owned(),
                },
                TextRun {
                    x_pt: 68.0,
                    baseline_from_top_pt: 222.0,
                    size_pt: 18.0,
                    font: Font(Base14Font::TimesRoman),
                    text: "Sapte sopirle tipa in tara.".to_owned(),
                },
                TextRun {
                    x_pt: 68.0,
                    baseline_from_top_pt: 280.0,
                    size_pt: 11.0,
                    font: Font(Base14Font::Helvetica),
                    text: "Bonus: historical cedilla codepoints rendered \
                           with the glyph each AFM ships."
                        .to_owned(),
                },
                TextRun {
                    x_pt: 68.0,
                    baseline_from_top_pt: 304.0,
                    size_pt: 18.0,
                    font: Font(Base14Font::TimesRoman),
                    // U+015E Ş (Scedilla, distinct cedilla glyph),
                    // U+0162 Ţ (Tcommaaccent, AFM has no Tcedilla so
                    //           routes here despite the codepoint name).
                    text: "Ş vs Ș    ş vs ș    Ţ vs Ț    ţ vs ț".to_owned(),
                },
                TextRun {
                    x_pt: 68.0,
                    baseline_from_top_pt: 360.0,
                    size_pt: 9.0,
                    font: Font(Base14Font::HelveticaOblique),
                    text: "Look at the bottom of each S/T: the right form has \
                           a comma hanging beneath (Scommaaccent/Tcommaaccent), \
                           the left has the cedilla hook (Scedilla)."
                        .to_owned(),
                },
            ],
        }],
    };

    let out = std::path::PathBuf::from("/tmp/commaaccent-demo.pdf");
    let diags = mosaic_pdf::emit(&graph, &PdfMetadata::default(), &out)?;
    // The example always succeeds quietly — readers care about the
    // produced `/tmp/commaaccent-demo.pdf`, not the console. Any
    // diagnostics surfaced here would indicate a bug in the layout
    // or planner, so escalate as an error to fail-fast.
    if !diags.is_empty() {
        return Err(format!("unexpected diagnostics: {diags:?}").into());
    }
    Ok(())
}
