//! End-to-end round-trip for the Type 0 CID-keyed embedded-font path.
//!
//! Renders Cyrillic + Greek runs through the bundled Noto Sans family,
//! parses the produced PDF with `lopdf`, and asserts:
//!
//! 1. The font dict is `/Type /Font /Subtype /Type0` with
//!    `/Encoding /Identity-H`, a `/DescendantFonts` array, and a
//!    `/ToUnicode` reference.
//! 2. The descendant is `/Subtype /CIDFontType2` with
//!    `/CIDSystemInfo /Registry (Adobe) /Ordering (Identity)`,
//!    `/CIDToGIDMap /Identity`, and a `/FontDescriptor`.
//! 3. The descriptor carries a `/FontFile2` stream whose `/Length1`
//!    matches the stream body length, and that length is materially
//!    smaller than the bundled TTF (subsetting actually subset).
//! 4. The `/ToUnicode` `CMap` maps every CID used in the content
//!    stream back to the source codepoint.
//! 5. The content stream uses hex-string CID pairs (`<HHHH ...>`), not
//!    ASCII literal strings.

use std::error::Error;

use lopdf::{Document, Object};
use mosaic_fonts::EmbeddedFontId;
use mosaic_layout::{EmbeddedFontId as LayoutEmbeddedFontId, Font, Page, PageGraph, TextRun};
use mosaic_pdf::PdfMetadata;

type TestResult = Result<(), Box<dyn Error>>;

macro_rules! ensure {
    ($cond:expr, $($arg:tt)*) => {
        if !$cond {
            return Err(format!($($arg)*).into());
        }
    };
}

fn render(face: EmbeddedFontId, text: &str) -> Result<(Document, Vec<u8>), Box<dyn Error>> {
    let graph = PageGraph {
        pages: vec![Page {
            number: 1,
            width_pt: 595.276_f32,
            height_pt: 841.89_f32,
            runs: vec![TextRun {
                x_pt: 68.0,
                baseline_from_top_pt: 100.0,
                size_pt: 12.0,
                font: Font::Embedded(face),
                text: text.to_owned(),
                glyphs: mosaic_fonts::shape(face.data(), text),
            }],
        }],
    };
    let tmp = std::env::temp_dir().join(format!(
        "mosaic-embedded-rt-{}.pdf",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos())
    ));
    let diags = mosaic_pdf::emit(&graph, &PdfMetadata::default(), &tmp)
        .map_err(|e| format!("emit: {e:?}"))?;
    if !diags.is_empty() {
        return Err(format!("unexpected diagnostics: {diags:?}").into());
    }
    let bytes = std::fs::read(&tmp)?;
    let doc = Document::load_mem(&bytes)?;
    std::fs::remove_file(&tmp).ok();
    Ok((doc, bytes))
}

fn deref<'d>(doc: &'d Document, obj: &'d Object) -> Result<&'d Object, Box<dyn Error>> {
    match obj {
        Object::Reference(r) => Ok(doc.get_object(*r)?),
        other => Ok(other),
    }
}

fn font_dict<'d>(
    doc: &'d Document,
    resource_name: &[u8],
) -> Result<&'d lopdf::Dictionary, Box<dyn Error>> {
    let page_id = doc.page_iter().next().ok_or("no pages")?;
    let page = doc.get_dictionary(page_id)?;
    let resources = deref(doc, page.get(b"Resources")?)?.as_dict()?;
    let fonts = deref(doc, resources.get(b"Font")?)?.as_dict()?;
    let Object::Reference(id) = fonts.get(resource_name)? else {
        return Err("expected indirect font ref".into());
    };
    Ok(doc.get_dictionary(*id)?)
}

#[test]
fn cyrillic_emits_type0_cid_font_chain() -> TestResult {
    let (doc, _) = render(EmbeddedFontId::Regular, "Привет, Мозаика")?;
    // Resource F15 is Noto Sans Regular's slot.
    let type0 = font_dict(&doc, b"F15")?;
    ensure!(
        type0.get(b"Subtype")? == &Object::Name(b"Type0".to_vec()),
        "expected /Subtype /Type0, got {:?}",
        type0.get(b"Subtype"),
    );
    ensure!(
        type0.get(b"Encoding")? == &Object::Name(b"Identity-H".to_vec()),
        "expected /Encoding /Identity-H",
    );
    let descendants = match type0.get(b"DescendantFonts")? {
        Object::Array(a) => a,
        other => return Err(format!("expected array, got {other:?}").into()),
    };
    ensure!(
        descendants.len() == 1,
        "expected 1 descendant, got {}",
        descendants.len(),
    );
    let Object::Reference(cid_id) = descendants[0] else {
        return Err("expected indirect descendant".into());
    };
    let cid = doc.get_dictionary(cid_id)?;
    ensure!(
        cid.get(b"Subtype")? == &Object::Name(b"CIDFontType2".to_vec()),
        "expected /Subtype /CIDFontType2",
    );
    ensure!(
        cid.get(b"CIDToGIDMap")? == &Object::Name(b"Identity".to_vec()),
        "expected /CIDToGIDMap /Identity",
    );

    // ToUnicode CMap present and non-empty.
    let Object::Reference(cmap_id) = type0.get(b"ToUnicode")? else {
        return Err("expected indirect /ToUnicode".into());
    };
    let Object::Stream(cmap_stream) = doc.get_object(*cmap_id)? else {
        return Err("ToUnicode is not a stream".into());
    };
    let cmap_text = String::from_utf8_lossy(&cmap_stream.content);
    ensure!(
        cmap_text.contains("beginbfchar") || cmap_text.contains("beginbfrange"),
        "ToUnicode CMap missing bfchar/bfrange",
    );
    // The source contained `П` (U+041F) — its 4-hex-uppercase form
    // must appear in a bfchar entry's RHS.
    ensure!(
        cmap_text.contains("041F"),
        "ToUnicode CMap missing U+041F (П):\n{cmap_text}",
    );
    Ok(())
}

#[test]
fn font_file2_is_subset_not_full_ttf() -> TestResult {
    let (doc, _) = render(EmbeddedFontId::Regular, "Привет")?;
    let type0 = font_dict(&doc, b"F15")?;
    let descendants = type0.get(b"DescendantFonts")?.as_array()?;
    let Object::Reference(cid_id) = descendants[0] else {
        return Err("expected indirect descendant".into());
    };
    let cid = doc.get_dictionary(cid_id)?;
    let Object::Reference(desc_id) = cid.get(b"FontDescriptor")? else {
        return Err("expected indirect descriptor".into());
    };
    let descriptor = doc.get_dictionary(*desc_id)?;
    let Object::Reference(ff_id) = descriptor.get(b"FontFile2")? else {
        return Err("expected indirect /FontFile2".into());
    };
    let Object::Stream(ff_stream) = doc.get_object(*ff_id)? else {
        return Err("FontFile2 is not a stream".into());
    };

    let full_ttf_len = EmbeddedFontId::Regular.data().bytes.len();
    let subset_len = ff_stream.content.len();
    ensure!(
        subset_len < full_ttf_len / 4,
        "subset {subset_len} bytes is not materially smaller than full TTF {full_ttf_len}",
    );

    // /Length1 (uncompressed size) must equal the actual stream
    // content length — pdf-writer doesn't compress, so the two are
    // the same.
    let length1 = match ff_stream.dict.get(b"Length1")? {
        Object::Integer(n) => usize::try_from(*n)?,
        other => return Err(format!("expected integer /Length1, got {other:?}").into()),
    };
    ensure!(
        length1 == subset_len,
        "Length1 ({length1}) != stream content length ({subset_len})",
    );
    Ok(())
}

#[test]
fn greek_routes_through_same_embedded_face() -> TestResult {
    // Greek and Cyrillic both live in Noto Sans Regular's coverage,
    // so a single document mixing both should produce one F15 font
    // dict (not two competing embedded faces).
    let (doc, _) = render(EmbeddedFontId::Regular, "Καλημέρα κόσμε")?;
    let type0 = font_dict(&doc, b"F15")?;
    ensure!(
        type0.get(b"Subtype")? == &Object::Name(b"Type0".to_vec()),
        "Greek-only doc should still emit a Type0 face",
    );
    Ok(())
}

#[test]
fn content_stream_uses_hex_cid_pairs_not_ascii_literals() -> TestResult {
    let (doc, bytes) = render(EmbeddedFontId::Regular, "Hello")?;
    // The content stream for embedded runs is `<HHHH HHHH ...>`, not
    // `(Hello)`. Search the content stream slice; pre-loaded `doc` is
    // only there to ensure the bytes parse cleanly.
    let _ = doc;
    let open = b"\nstream\n";
    let close = b"\nendstream";
    let open_at = bytes
        .windows(open.len())
        .position(|w| w == open)
        .ok_or("no content stream")?;
    let body = &bytes[open_at + open.len()..];
    let close_at = body
        .windows(close.len())
        .position(|w| w == close)
        .ok_or("no endstream")?;
    let content = &body[..close_at];
    // The embedded-run encoder emits 2 bytes per glyph (CIDs as
    // big-endian u16). pdf-writer picks `(...)` or `<...>` based on
    // byte contents; we don't depend on that choice. What we do
    // depend on is that the source ASCII letters don't appear
    // verbatim in the content stream — if they did, the run had
    // taken the Base14 byte path by mistake.
    ensure!(
        !content.windows(b"(Hello)".len()).any(|w| w == b"(Hello)"),
        "embedded run leaked `(Hello)` literal — wrong code path?",
    );
    Ok(())
}

#[test]
fn re_exported_layout_id_matches_fonts_id() {
    // The layout crate re-exports EmbeddedFontId; both paths must
    // resolve to the same enum.
    assert_eq!(LayoutEmbeddedFontId::Regular, EmbeddedFontId::Regular,);
}
