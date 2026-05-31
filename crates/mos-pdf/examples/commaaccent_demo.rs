//! One-off demo: emit a PDF with two Romanian lines side by side —
//! one using the comma-below glyphs (`Ș`/`ș`/`Ț`/`ț` via U+0218..U+021B
//! → `Scommaaccent`/`scommaaccent`/`Tcommaaccent`/`tcommaaccent` in
//! the Helvetica AFM, reachable through the per-document
//! `/Differences` planner) and one with the diacritics stripped to
//! ASCII (`S`/`s`/`T`/`t`) for visual comparison.

use mos_core::Severity;
use mos_layout::{Base14Font, Font, Page, PageGraph, TextRun};
use mos_pdf::PdfMetadata;

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
                    font: Font::Base14(Base14Font::HelveticaBold),
                    text: "Romanian comma-below demo".to_owned(),
                    actual_text: None,
                    glyphs: Vec::new(),
                },
                TextRun {
                    x_pt: 68.0,
                    baseline_from_top_pt: 130.0,
                    size_pt: 11.0,
                    font: Font::Base14(Base14Font::Helvetica),
                    text: "With comma-below (correct Romanian):".to_owned(),
                    actual_text: None,
                    glyphs: Vec::new(),
                },
                TextRun {
                    x_pt: 68.0,
                    baseline_from_top_pt: 152.0,
                    size_pt: 18.0,
                    font: Font::Base14(Base14Font::TimesRoman),
                    // Ș=U+0218, ș=U+0219, Ț=U+021A, ț=U+021B
                    text: "Șapte șopârle țipă în țară.".to_owned(),
                    actual_text: None,
                    glyphs: Vec::new(),
                },
                TextRun {
                    x_pt: 68.0,
                    baseline_from_top_pt: 200.0,
                    size_pt: 11.0,
                    font: Font::Base14(Base14Font::Helvetica),
                    text: "Without (ASCII fallback, no diacritic):".to_owned(),
                    actual_text: None,
                    glyphs: Vec::new(),
                },
                TextRun {
                    x_pt: 68.0,
                    baseline_from_top_pt: 222.0,
                    size_pt: 18.0,
                    font: Font::Base14(Base14Font::TimesRoman),
                    text: "Sapte sopirle tipa in tara.".to_owned(),
                    actual_text: None,
                    glyphs: Vec::new(),
                },
                TextRun {
                    x_pt: 68.0,
                    baseline_from_top_pt: 280.0,
                    size_pt: 11.0,
                    font: Font::Base14(Base14Font::Helvetica),
                    text: "Bonus: historical cedilla codepoints rendered \
                           with the glyph each AFM ships."
                        .to_owned(),
                    actual_text: None,
                    glyphs: Vec::new(),
                },
                TextRun {
                    x_pt: 68.0,
                    baseline_from_top_pt: 304.0,
                    size_pt: 18.0,
                    font: Font::Base14(Base14Font::TimesRoman),
                    // U+015E Ş (Scedilla, distinct cedilla glyph),
                    // U+0162 Ţ (Tcommaaccent, AFM has no Tcedilla so
                    //           routes here despite the codepoint name).
                    text: "Ş vs Ș    ş vs ș    Ţ vs Ț    ţ vs ț".to_owned(),
                    actual_text: None,
                    glyphs: Vec::new(),
                },
                TextRun {
                    x_pt: 68.0,
                    baseline_from_top_pt: 360.0,
                    size_pt: 9.0,
                    font: Font::Base14(Base14Font::HelveticaOblique),
                    text: "Look at the bottom of each S/T: the right form has \
                           a comma hanging beneath (Scommaaccent/Tcommaaccent), \
                           the left has the cedilla hook (Scedilla)."
                        .to_owned(),
                    actual_text: None,
                    glyphs: Vec::new(),
                },
            ],
            images: Vec::new(),
        }],
        images: Vec::new(),
    };

    let out = std::path::PathBuf::from("/tmp/commaaccent-demo.pdf");
    let diags = mos_pdf::emit(&graph, &PdfMetadata::default(), &out)?;
    // Mirror the CLI's severity gate (see
    // `crates/mos/src/main.rs` — exits non-zero only on
    // `Severity::Error`). Warnings (e.g. `MOS0032` glyph-budget exhaustion)
    // are non-fatal and pass through silently; the workspace's
    // `-D warnings` clippy rule blocks `println!`/`eprintln!` on
    // examples, so per-diagnostic logging isn't an option without
    // a lint-suppression annotation we're not allowed to add.
    let errors: Vec<_> = diags
        .iter()
        .filter(|d| d.severity() == Severity::Error)
        .collect();
    if !errors.is_empty() {
        return Err(format!("emit produced error diagnostics: {errors:?}").into());
    }
    Ok(())
}
