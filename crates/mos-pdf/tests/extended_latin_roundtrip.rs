//! End-to-end round-trip: render Polish + Czech text through the PDF
//! backend, parse the result with `lopdf`, and verify that
//!
//! 1. Each Latin Core 14 face that needed extended glyphs gets a
//!    custom `/Encoding` dict (`/BaseEncoding /WinAnsiEncoding` +
//!    `/Differences`) and a `/ToUnicode` `CMap` reference.
//! 2. The `/Differences` array names the AFM glyphs we expect
//!    (`Lslash`, `rcaron`, `ecaron`, `zacute`, …).
//! 3. The `/ToUnicode` `CMap` actually maps the remapped bytes back to
//!    the original Unicode codepoints (smoke test for copy-paste).
//! 4. Faces that don't need extended glyphs keep the
//!    `/Encoding /WinAnsiEncoding` shortcut (no extra dict, no
//!    `/ToUnicode`).

use std::error::Error;
use std::sync::atomic::{AtomicU64, Ordering};

use lopdf::{Document, Object};
use mos_layout::{Base14Font, Font, Page, PageGraph, TextRun};
use mos_pdf::PdfMetadata;

type TestResult = Result<(), Box<dyn Error>>;

/// Monotonic counter giving each [`render`] call a unique temp-file name.
/// See the note in `render` for why a clock-based name is not enough.
static RENDER_SEQ: AtomicU64 = AtomicU64::new(0);

/// Tiny `assert!`-shaped helper that returns `Err` instead of
/// panicking, so the surrounding `-> TestResult` test bodies stay
/// clippy-clean under `clippy::panic_in_result_fn`. Mirrors the
/// `Vec<String>`-of-diffs precedent in
/// `pdf-base14-metrics/tests/winansi_vendor.rs`.
macro_rules! ensure {
    ($cond:expr, $($arg:tt)*) => {
        if !$cond {
            return Err(format!($($arg)*).into());
        }
    };
}

/// Render `text` in `face` and return the parsed PDF.
fn render(face: Base14Font, text: &str) -> Result<Document, Box<dyn Error>> {
    let graph = PageGraph {
        pages: vec![Page {
            number: 1,
            width_pt: 595.276_f32,
            height_pt: 841.89_f32,
            runs: vec![TextRun {
                x_pt: 68.0,
                baseline_from_top_pt: 100.0,
                size_pt: 12.0,
                font: Font::Base14(face),
                text: text.to_owned(),
                actual_text: None,
                glyphs: Vec::new(),
            }],
            images: Vec::new(),
        }],
        images: Vec::new(),
    };
    // Unique per call. `SystemTime::now()` is too coarse on some platforms
    // (notably Windows, ~15 ms granularity) to disambiguate tests that render
    // concurrently, so two threads could collide on one temp path and clobber
    // each other's PDF mid-write: an intermittent failure. A process id plus a
    // monotonic counter is collision-free without depending on clock resolution.
    let tmp = std::env::temp_dir().join(format!(
        "mos-pdf-rt-{}-{}.pdf",
        std::process::id(),
        RENDER_SEQ.fetch_add(1, Ordering::Relaxed)
    ));
    let diags =
        mos_pdf::emit(&graph, &PdfMetadata::default(), &tmp).map_err(|e| format!("emit: {e:?}"))?;
    if !diags.is_empty() {
        return Err(format!("unexpected diagnostics: {diags:?}").into());
    }
    let doc = Document::load(&tmp)?;
    std::fs::remove_file(&tmp).ok();
    Ok(doc)
}

/// Resolve `Object::Reference` indirections; otherwise return as-is.
fn deref<'d>(doc: &'d Document, obj: &'d Object) -> Result<&'d Object, Box<dyn Error>> {
    match obj {
        Object::Reference(r) => Ok(doc.get_object(*r)?),
        other => Ok(other),
    }
}

/// Find the font dict object for the given PDF resource name (e.g.
/// `F1`) inside `doc`'s first page.
fn font_dict<'d>(
    doc: &'d Document,
    resource_name: &[u8],
) -> Result<&'d lopdf::Dictionary, Box<dyn Error>> {
    let page_id = doc.page_iter().next().ok_or("no pages")?;
    let page = doc.get_dictionary(page_id)?;
    let resources = deref(doc, page.get(b"Resources")?)?.as_dict()?;
    let fonts = deref(doc, resources.get(b"Font")?)?.as_dict()?;
    let font_ref = fonts.get(resource_name)?;
    let Object::Reference(id) = font_ref else {
        return Err("expected indirect font ref".into());
    };
    Ok(doc.get_dictionary(*id)?)
}

/// Extract the glyph-name list from a font dict's `/Differences`
/// array. Slot numbers (integers) are filtered out so callers can
/// assert on names alone.
fn differences_names<'d>(
    doc: &'d Document,
    font: &'d lopdf::Dictionary,
) -> Result<Vec<&'d [u8]>, Box<dyn Error>> {
    let enc_id = match font.get(b"Encoding")? {
        Object::Reference(r) => *r,
        other => return Err(format!("expected indirect /Encoding, got {other:?}").into()),
    };
    let enc_dict = doc.get_dictionary(enc_id)?;
    let Object::Array(diffs_arr) = enc_dict.get(b"Differences")? else {
        return Err("Differences not an array".into());
    };
    Ok(diffs_arr
        .iter()
        .filter_map(|o| match o {
            Object::Name(n) => Some(n.as_slice()),
            _ => None,
        })
        .collect())
}

/// Return the (parsed-string-view of the) `/ToUnicode` `CMap` stream
/// referenced by `font`'s font dict.
fn to_unicode_cmap(doc: &Document, font: &lopdf::Dictionary) -> Result<String, Box<dyn Error>> {
    let cmap_id = match font.get(b"ToUnicode")? {
        Object::Reference(r) => *r,
        _ => return Err("expected indirect ToUnicode".into()),
    };
    let Object::Stream(stream) = doc.get_object(cmap_id)? else {
        return Err("ToUnicode is not a stream".into());
    };
    Ok(String::from_utf8_lossy(&stream.content).into_owned())
}

#[test]
fn polish_round_trips_through_differences_and_to_unicode() -> TestResult {
    let doc = render(Base14Font::Helvetica, "Łódź")?;
    let helv = font_dict(&doc, b"F1")?;

    // /Encoding should be an indirect reference to a custom dict, not
    // the predefined /WinAnsiEncoding name.
    let Object::Reference(enc_id) = helv.get(b"Encoding")? else {
        return Err("expected indirect /Encoding".into());
    };
    let enc_dict = doc.get_dictionary(*enc_id)?;
    ensure!(
        enc_dict.get(b"BaseEncoding")? == &Object::Name(b"WinAnsiEncoding".to_vec()),
        "BaseEncoding is not /WinAnsiEncoding",
    );
    let names = differences_names(&doc, helv)?;
    ensure!(
        names.contains(&b"Lslash".as_slice()),
        "missing Lslash in /Differences",
    );
    ensure!(
        names.contains(&b"zacute".as_slice()),
        "missing zacute in /Differences",
    );
    Ok(())
}

#[test]
fn to_unicode_round_trips_remapped_bytes_to_original_unicode() -> TestResult {
    // Łódź carries two extended chars: Ł (U+0141) and ź (U+017A).
    // The deterministic allocator hands Ł the first gap slot
    // (0x7F) and ź the second (0x81); pdf-writer's UnicodeCmap emits
    // `<7F> <0141>` / `<81> <017A>` (uppercase hex via `push_hex`).
    // ó and d are WinAnsi natives at 0xF3 and 0x64 respectively, and
    // the CMap covers them too (every byte the content stream emits
    // gets a mapping, not just remapped slots).
    let doc = render(Base14Font::Helvetica, "Łódź")?;
    let helv = font_dict(&doc, b"F1")?;
    let cmap = to_unicode_cmap(&doc, helv)?;

    // The bfchar block uses literal angle-bracketed hex pairs.
    ensure!(
        cmap.contains("<7F> <0141>"),
        "CMap missing remap mapping <7F> -> U+0141 (Ł):\n{cmap}",
    );
    ensure!(
        cmap.contains("<81> <017A>"),
        "CMap missing remap mapping <81> -> U+017A (ź):\n{cmap}",
    );
    // WinAnsi natives also covered.
    ensure!(
        cmap.contains("<F3> <00F3>"),
        "CMap missing native mapping <F3> -> U+00F3 (ó):\n{cmap}",
    );
    ensure!(
        cmap.contains("<64> <0064>"),
        "CMap missing native mapping <64> -> U+0064 (d):\n{cmap}",
    );
    Ok(())
}

#[test]
fn ascii_only_keeps_predefined_winansi_shortcut() -> TestResult {
    let doc = render(Base14Font::Helvetica, "Hello, world!")?;
    let helv = font_dict(&doc, b"F1")?;

    // The encoding is the predefined name, not an indirect reference
    // to a custom dict.
    ensure!(
        helv.get(b"Encoding")? == &Object::Name(b"WinAnsiEncoding".to_vec()),
        "expected predefined /WinAnsiEncoding on ASCII-only doc",
    );
    // No /ToUnicode for the predefined-encoding shortcut path.
    ensure!(
        helv.get(b"ToUnicode").is_err(),
        "ASCII-only doc should not emit /ToUnicode",
    );
    Ok(())
}

#[test]
fn unused_faces_still_get_predefined_winansi() -> TestResult {
    // The doc only uses F1 (Helvetica), but the backend always emits
    // all 14 font dicts. The unused ones must keep the predefined
    // shortcut; never a custom /Differences dict, since the planner
    // never saw any chars for them.
    let doc = render(Base14Font::Helvetica, "Łódź")?;
    for resource in [b"F2".as_slice(), b"F3", b"F4", b"F5", b"F6"] {
        let dict = font_dict(&doc, resource)?;
        let label = std::str::from_utf8(resource).unwrap_or("?");
        ensure!(
            dict.get(b"Encoding")? == &Object::Name(b"WinAnsiEncoding".to_vec()),
            "unused face {label} should keep predefined WinAnsi",
        );
        ensure!(
            dict.get(b"ToUnicode").is_err(),
            "unused face {label} should not emit /ToUnicode",
        );
    }
    Ok(())
}

#[test]
fn czech_and_polish_share_the_same_face_one_differences_array() -> TestResult {
    // "Łódź — Příliš ě" routes everything through Helvetica (the
    // default for Paragraph text in the layout engine). All
    // extended glyphs land in a single /Differences array: no
    // double encoding emission.
    let doc = render(Base14Font::Helvetica, "Łódź — Příliš ě")?;
    let helv = font_dict(&doc, b"F1")?;
    let names = differences_names(&doc, helv)?;
    // Expect Ł→Lslash, ř→rcaron, ě→ecaron, ź→zacute.
    for expected in [b"Lslash" as &[u8], b"rcaron", b"ecaron", b"zacute"] {
        ensure!(
            names.contains(&expected),
            "missing {:?} in /Differences (got {:?})",
            std::str::from_utf8(expected).unwrap_or("?"),
            names
                .iter()
                .map(|n| std::str::from_utf8(n).unwrap_or("?"))
                .collect::<Vec<_>>(),
        );
    }
    Ok(())
}
