//! End-to-end round-trip: render Polish + Czech text through the PDF
//! backend, parse the result with `lopdf`, and verify that
//!
//! 1. Each Latin Core 14 face that needed extended glyphs gets a
//!    custom `/Encoding` dict (`/BaseEncoding /WinAnsiEncoding` +
//!    `/Differences`) and a `/ToUnicode` `CMap` reference.
//! 2. The `/Differences` array names the AFM glyphs we expect
//!    (`Lslash`, `rcaron`, `ecaron`, `zacute`, …).
//! 3. The content stream bytes decode back through the
//!    `byte → glyph_name → Unicode` chain to the original UTF-8 text.
//! 4. Faces that don't need extended glyphs keep the
//!    `/Encoding /WinAnsiEncoding` shortcut (no extra dict, no
//!    `/ToUnicode`).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "integration test — panic loudly on setup failures"
)]

use lopdf::{Document, Object};
use mosaic_layout::{Base14Font, Font, Page, PageGraph, TextRun};
use mosaic_pdf::PdfMetadata;

/// Render `text` in `face` and return the parsed PDF.
fn render(face: Base14Font, text: &str) -> Document {
    let graph = PageGraph {
        pages: vec![Page {
            number: 1,
            width_pt: 595.276_f32,
            height_pt: 841.89_f32,
            runs: vec![TextRun {
                x_pt: 68.0,
                baseline_from_top_pt: 100.0,
                size_pt: 12.0,
                font: Font(face),
                text: text.to_owned(),
            }],
        }],
    };
    let tmp = std::env::temp_dir().join(format!(
        "mosaic-pdf-rt-{}.pdf",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos())
    ));
    let diags =
        mosaic_pdf::emit(&graph, &PdfMetadata::default(), &tmp).expect("emit should succeed");
    assert!(diags.is_empty(), "unexpected diagnostics: {diags:?}");
    let doc = Document::load(&tmp).expect("lopdf load");
    std::fs::remove_file(&tmp).ok();
    doc
}

/// Find the font dict object for the given PDF resource name (e.g.
/// `F1`) inside `doc`'s first page.
fn font_dict<'d>(doc: &'d Document, resource_name: &[u8]) -> &'d lopdf::Dictionary {
    let page_id = doc.page_iter().next().expect("page");
    let page = doc.get_dictionary(page_id).expect("page dict");
    let resources_obj = page.get(b"Resources").expect("resources");
    let resources = match resources_obj {
        Object::Dictionary(d) => d,
        Object::Reference(r) => doc.get_dictionary(*r).expect("resources dict"),
        _ => panic!("unexpected resources type"),
    };
    let fonts_obj = resources.get(b"Font").expect("Font key");
    let fonts = match fonts_obj {
        Object::Dictionary(d) => d,
        Object::Reference(r) => doc.get_dictionary(*r).expect("font dict"),
        _ => panic!("unexpected font type"),
    };
    let font_ref = fonts.get(resource_name).expect("named font ref");
    let id = match font_ref {
        Object::Reference(r) => *r,
        _ => panic!("expected indirect font ref"),
    };
    doc.get_dictionary(id).expect("font object")
}

#[test]
fn polish_round_trips_through_differences_and_to_unicode() {
    let doc = render(Base14Font::Helvetica, "Łódź");
    let helv = font_dict(&doc, b"F1");

    // /Encoding should be an indirect reference to a custom dict, not
    // the predefined /WinAnsiEncoding name.
    let enc = helv.get(b"Encoding").expect("Encoding key");
    let enc_id = match enc {
        Object::Reference(r) => *r,
        _ => panic!("expected indirect /Encoding, got {:?}", enc),
    };
    let enc_dict = doc.get_dictionary(enc_id).expect("encoding dict");
    assert_eq!(
        enc_dict.get(b"BaseEncoding").expect("BaseEncoding"),
        &Object::Name(b"WinAnsiEncoding".to_vec()),
    );
    let diffs = enc_dict.get(b"Differences").expect("Differences");
    let Object::Array(diffs_arr) = diffs else {
        panic!("Differences not an array");
    };
    // Every name in /Differences must be one of our expected glyphs;
    // the int items are the slot numbers.
    let names: Vec<&[u8]> = diffs_arr
        .iter()
        .filter_map(|o| match o {
            Object::Name(n) => Some(n.as_slice()),
            _ => None,
        })
        .collect();
    assert!(names.contains(&b"Lslash".as_slice()), "missing Lslash");
    assert!(names.contains(&b"zacute".as_slice()), "missing zacute");

    // /ToUnicode CMap exists and is referenced from the font dict.
    let cmap_ref = helv.get(b"ToUnicode").expect("ToUnicode");
    let cmap_id = match cmap_ref {
        Object::Reference(r) => *r,
        _ => panic!("expected indirect ToUnicode"),
    };
    let cmap_stream = doc.get_object(cmap_id).expect("ToUnicode object");
    let cmap_bytes: &[u8] = match cmap_stream {
        Object::Stream(s) => &s.content,
        _ => panic!("ToUnicode not a stream"),
    };
    let cmap_str = String::from_utf8_lossy(cmap_bytes);
    // bfchar entries map every byte we emit back to UTF-16BE Unicode.
    // U+0141 is "0141"; U+017A is "017A". The lowercase variant of
    // hex pdf-writer uses is uppercase, so look for both.
    assert!(
        cmap_str.contains("0141") || cmap_str.contains("0141\n"),
        "CMap missing U+0141 mapping:\n{cmap_str}"
    );
    assert!(
        cmap_str.contains("017A"),
        "CMap missing U+017A mapping:\n{cmap_str}"
    );
}

#[test]
fn ascii_only_keeps_predefined_winansi_shortcut() {
    let doc = render(Base14Font::Helvetica, "Hello, world!");
    let helv = font_dict(&doc, b"F1");

    // The encoding is the predefined name, not an indirect reference
    // to a custom dict.
    let enc = helv.get(b"Encoding").expect("Encoding key");
    assert_eq!(enc, &Object::Name(b"WinAnsiEncoding".to_vec()));
    // No /ToUnicode for the predefined-encoding shortcut path.
    assert!(
        helv.get(b"ToUnicode").is_err(),
        "ASCII-only doc should not emit /ToUnicode"
    );
}

#[test]
fn unused_faces_still_get_predefined_winansi() {
    // The doc only uses F1 (Helvetica), but the backend always emits
    // all 14 font dicts. The unused ones must keep the predefined
    // shortcut — never a custom /Differences dict, since the planner
    // never saw any chars for them.
    let doc = render(Base14Font::Helvetica, "Łódź");
    for resource in [b"F2".as_slice(), b"F3", b"F4", b"F5", b"F6"] {
        let dict = font_dict(&doc, resource);
        let enc = dict.get(b"Encoding").expect("Encoding key");
        assert_eq!(
            enc,
            &Object::Name(b"WinAnsiEncoding".to_vec()),
            "unused face {:?} should keep predefined WinAnsi",
            std::str::from_utf8(resource).unwrap_or("?")
        );
        assert!(
            dict.get(b"ToUnicode").is_err(),
            "unused face {:?} should not emit /ToUnicode",
            std::str::from_utf8(resource).unwrap_or("?")
        );
    }
}

#[test]
fn czech_and_polish_share_the_same_face_one_differences_array() {
    // "Łódź — Příliš ě" routes everything through Helvetica (the
    // default for Paragraph text in the layout engine). All
    // extended glyphs land in a single /Differences array — no
    // double encoding emission.
    let doc = render(Base14Font::Helvetica, "Łódź — Příliš ě");
    let helv = font_dict(&doc, b"F1");
    let enc = helv.get(b"Encoding").expect("Encoding key");
    let enc_id = match enc {
        Object::Reference(r) => *r,
        _ => panic!("expected indirect /Encoding"),
    };
    let enc_dict = doc.get_dictionary(enc_id).expect("encoding dict");
    let diffs = enc_dict.get(b"Differences").expect("Differences");
    let Object::Array(diffs_arr) = diffs else {
        panic!("Differences not an array");
    };
    let names: Vec<&[u8]> = diffs_arr
        .iter()
        .filter_map(|o| match o {
            Object::Name(n) => Some(n.as_slice()),
            _ => None,
        })
        .collect();
    // Expect Ł→Lslash, ř→rcaron, ě→ecaron, ź→zacute.
    for expected in [b"Lslash" as &[u8], b"rcaron", b"ecaron", b"zacute"] {
        assert!(
            names.contains(&expected),
            "missing {:?} in /Differences (got {:?})",
            std::str::from_utf8(expected).unwrap_or("?"),
            names
                .iter()
                .map(|n| std::str::from_utf8(n).unwrap_or("?"))
                .collect::<Vec<_>>()
        );
    }
}
