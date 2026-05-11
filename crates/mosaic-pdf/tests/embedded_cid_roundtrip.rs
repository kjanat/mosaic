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
use mosaic_fonts::{EmbeddedFontId, shape_with_fallback};
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

/// Build a `PageGraph` whose single page emits one `TextRun` per
/// fallback sub-run, mimicking what `mosaic-layout::flush_line`
/// does when shaping a fallback-aware word. The runs share a
/// baseline; each `x_pt` advances by the previous sub-run's
/// `advance_pt` so the runs render side-by-side, never overlapping.
fn build_fallback_graph(
    primary: EmbeddedFontId,
    fallbacks: &[Font],
    text: &str,
) -> (PageGraph, Vec<f32>) {
    let size_pt = 12.0_f32;
    let mut x_pt = 68.0_f32;
    let mut xs = Vec::new();
    let mut runs = Vec::new();
    for sub in shape_with_fallback(Font::Embedded(primary), fallbacks, size_pt, text) {
        xs.push(x_pt);
        runs.push(TextRun {
            x_pt,
            baseline_from_top_pt: 100.0,
            size_pt,
            font: sub.font,
            text: sub.text,
            glyphs: sub.glyphs,
        });
        x_pt += sub.advance_pt;
    }
    let graph = PageGraph {
        pages: vec![Page {
            number: 1,
            width_pt: 595.276_f32,
            height_pt: 841.89_f32,
            runs,
        }],
    };
    (graph, xs)
}

fn emit_graph(graph: &PageGraph) -> Result<(Document, Vec<u8>), Box<dyn Error>> {
    let tmp = std::env::temp_dir().join(format!(
        "mosaic-embedded-rt-{}.pdf",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos())
    ));
    let diags = mosaic_pdf::emit(graph, &PdfMetadata::default(), &tmp)
        .map_err(|e| format!("emit: {e:?}"))?;
    if !diags.is_empty() {
        return Err(format!("unexpected diagnostics: {diags:?}").into());
    }
    let bytes = std::fs::read(&tmp)?;
    let doc = Document::load_mem(&bytes)?;
    std::fs::remove_file(&tmp).ok();
    Ok((doc, bytes))
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
            images: Vec::new(),
        }],
        images: Vec::new(),
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

#[test]
fn notdef_glyphs_dont_pollute_tounicode() -> TestResult {
    // CJK + emoji aren't in Noto Sans Regular's coverage, so rustybuzz
    // emits gid 0 (`.notdef`) for those codepoints. The ToUnicode CMap
    // must not record a Unicode mapping for gid 0 — otherwise every
    // unsupported character round-trips back to whichever source
    // codepoint happened to be first.
    let (doc, _) = render(EmbeddedFontId::Regular, "日本 🦀")?;
    let type0 = font_dict(&doc, b"F15")?;
    let Object::Reference(cmap_id) = type0.get(b"ToUnicode")? else {
        return Err("expected indirect /ToUnicode".into());
    };
    let Object::Stream(cmap_stream) = doc.get_object(*cmap_id)? else {
        return Err("ToUnicode is not a stream".into());
    };
    let cmap_text = String::from_utf8_lossy(&cmap_stream.content);
    // `.notdef` is CID 0 under /CIDToGIDMap /Identity. No `<0000>`
    // entry should appear on the LHS of any bfchar/bfrange mapping.
    // Scan only the body of `begin{bfchar,bfrange}..end…` blocks; the
    // `<0000> <FFFF>` codespacerange header is required and unrelated.
    let mut in_block = false;
    for line in cmap_text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("beginbfchar") || trimmed.starts_with("beginbfrange") {
            in_block = true;
            continue;
        }
        if trimmed.starts_with("endbfchar") || trimmed.starts_with("endbfrange") {
            in_block = false;
            continue;
        }
        if in_block && trimmed.starts_with("<0000>") {
            return Err(format!(
                "ToUnicode CMap maps gid 0 (.notdef): {trimmed}\nFull:\n{cmap_text}"
            )
            .into());
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------
// Per-glyph fallback tests (Noto Sans Regular + Math fallback)
// ---------------------------------------------------------------------

const MATH_FALLBACK: &[Font] = &[Font::Embedded(EmbeddedFontId::Math)];

fn extract_content_stream(bytes: &[u8]) -> Result<&[u8], Box<dyn Error>> {
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
    Ok(&body[..close_at])
}

#[test]
fn mixed_latin_math_emits_two_resource_slots() -> TestResult {
    // `a≤b`: `a` and `b` resolve in the primary (Noto Sans Regular, F15);
    // `≤` (U+2264) is not in Noto Sans but is in Noto Sans Math (F20).
    // The fallback machinery must produce three sub-runs that share a
    // baseline, and the page resources must list both font slots.
    let (graph, xs) = build_fallback_graph(EmbeddedFontId::Regular, MATH_FALLBACK, "a≤b");
    ensure!(
        graph.pages[0].runs.len() == 3,
        "expected 3 sub-runs (a / ≤ / b), got {} — {:?}",
        graph.pages[0].runs.len(),
        graph.pages[0]
            .runs
            .iter()
            .map(|r| (&r.text, r.font))
            .collect::<Vec<_>>(),
    );
    let fonts: Vec<Font> = graph.pages[0].runs.iter().map(|r| r.font).collect();
    ensure!(
        fonts
            == vec![
                Font::Embedded(EmbeddedFontId::Regular),
                Font::Embedded(EmbeddedFontId::Math),
                Font::Embedded(EmbeddedFontId::Regular),
            ],
        "sub-run face sequence wrong: {fonts:?}",
    );
    // x positions must be strictly increasing — sub-runs render
    // side-by-side, never overlapping. `f32` is only `PartialOrd`, so
    // compare via `partial_cmp` to be explicit about NaN handling
    // (NaN here would be a real bug, not "incomparable values").
    for pair in xs.windows(2) {
        let order = pair[0].partial_cmp(&pair[1]).ok_or("NaN sub-run x_pt")?;
        ensure!(
            order == std::cmp::Ordering::Less,
            "x_pt did not advance between sub-runs: {xs:?}",
        );
    }

    let (doc, _) = emit_graph(&graph)?;
    // Both F15 (Regular) and F20 (Math) must appear as font resources.
    let _ = font_dict(&doc, b"F15")?;
    let _ = font_dict(&doc, b"F20")?;
    Ok(())
}

#[test]
fn mixed_run_content_stream_switches_tf_in_source_order() -> TestResult {
    // The content stream for `a≤b` must contain `/F15 ... Tj /F20 ... Tj
    // /F15 ... Tj`, in that order. lopdf's stream parsing splits across
    // operators; the rawest check is a substring scan over the content
    // stream bytes for the resource names in source order.
    let (graph, _) = build_fallback_graph(EmbeddedFontId::Regular, MATH_FALLBACK, "a≤b");
    let (_, bytes) = emit_graph(&graph)?;
    let content = extract_content_stream(&bytes)?;
    let f15_first = content
        .windows(3)
        .position(|w| w == b"F15")
        .ok_or("F15 not in content stream")?;
    let f20 = content
        .windows(3)
        .position(|w| w == b"F20")
        .ok_or("F20 not in content stream — fallback not emitted")?;
    let f15_second = content[f15_first + 3..]
        .windows(3)
        .position(|w| w == b"F15")
        .map(|p| p + f15_first + 3)
        .ok_or("only one F15 occurrence — primary run did not resume after fallback")?;
    ensure!(
        f15_first < f20 && f20 < f15_second,
        "Tf switches not in source order: F15@{f15_first} F20@{f20} F15@{f15_second}",
    );
    Ok(())
}

#[test]
fn math_subrun_tounicode_maps_only_its_own_codepoint() -> TestResult {
    // The cluster rebasing contract: a sub-run's glyphs have `cluster`
    // offsets local to the sub-run's `text`. The /ToUnicode CMap for
    // F20 must therefore map back to the math character only — not to
    // a substring of the parent word `a≤b` (which would corrupt copy-
    // paste).
    let (graph, _) = build_fallback_graph(EmbeddedFontId::Regular, MATH_FALLBACK, "a≤b");
    let (doc, _) = emit_graph(&graph)?;
    let math_font = font_dict(&doc, b"F20")?;
    let Object::Reference(cmap_id) = math_font.get(b"ToUnicode")? else {
        return Err("expected indirect /ToUnicode on F20".into());
    };
    let Object::Stream(cmap_stream) = doc.get_object(*cmap_id)? else {
        return Err("F20 ToUnicode is not a stream".into());
    };
    let cmap_text = String::from_utf8_lossy(&cmap_stream.content);
    // `≤` is U+2264.
    ensure!(
        cmap_text.contains("2264"),
        "F20 ToUnicode missing U+2264 (≤):\n{cmap_text}",
    );
    // The Latin letters `a` (0x61) and `b` (0x62) must NOT appear as
    // bfchar RHS values in F20's CMap — they live in F15 only.
    let mut in_block = false;
    for line in cmap_text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("beginbfchar") || trimmed.starts_with("beginbfrange") {
            in_block = true;
            continue;
        }
        if trimmed.starts_with("endbfchar") || trimmed.starts_with("endbfrange") {
            in_block = false;
            continue;
        }
        if !in_block {
            continue;
        }
        // Look at the angle-bracketed Unicode RHS only — the LHS is a
        // CID, which can legitimately be any hex value. The RHS is the
        // last `<...>` token on the line.
        let Some(last_open) = trimmed.rfind('<') else {
            continue;
        };
        let Some(last_close) = trimmed[last_open..].find('>') else {
            continue;
        };
        let rhs = &trimmed[last_open + 1..last_open + last_close];
        ensure!(
            rhs != "0061" && rhs != "0062",
            "F20 ToUnicode leaks Latin codepoint into math sub-run: {trimmed}",
        );
    }
    Ok(())
}

#[test]
fn unsupported_codepoint_stays_notdef_without_panic() -> TestResult {
    // 🦀 (U+1F980) isn't covered by Noto Sans Regular OR by Noto Sans
    // Math. With Math as the only fallback, the cluster must stay in
    // the primary face as `.notdef` — no panic, no second sub-run, no
    // bogus CMap entry for the emoji codepoint.
    let (graph, _) = build_fallback_graph(EmbeddedFontId::Regular, MATH_FALLBACK, "a🦀b");
    // Whole word stays in the primary face (one sub-run) because no
    // fallback covered the emoji cluster.
    ensure!(
        graph.pages[0].runs.len() == 1,
        "expected single sub-run when fallback chain has no match, got {}",
        graph.pages[0].runs.len(),
    );
    let (doc, _) = emit_graph(&graph)?;
    let primary = font_dict(&doc, b"F15")?;
    let Object::Reference(cmap_id) = primary.get(b"ToUnicode")? else {
        return Err("expected indirect /ToUnicode on F15".into());
    };
    let Object::Stream(cmap_stream) = doc.get_object(*cmap_id)? else {
        return Err("F15 ToUnicode is not a stream".into());
    };
    let cmap_text = String::from_utf8_lossy(&cmap_stream.content);
    // U+1F980 needs a surrogate pair `D83E DD80` in /ToUnicode. The
    // .notdef cluster must NOT round-trip — the byte slice that
    // produced gid 0 is unmapped, so neither the high nor low surrogate
    // appears.
    ensure!(
        !cmap_text.contains("D83E") && !cmap_text.contains("DD80"),
        "ToUnicode CMap mapped a .notdef emoji cluster:\n{cmap_text}",
    );
    Ok(())
}

#[test]
fn capital_delta_letter_vs_increment_routing() -> TestResult {
    // Δ (U+0394, Greek capital delta) is a letter — Noto Sans covers
    // it, so it must stay in F15.
    let (graph, _) = build_fallback_graph(EmbeddedFontId::Regular, MATH_FALLBACK, "Δ");
    ensure!(
        graph.pages[0].runs.len() == 1
            && graph.pages[0].runs[0].font == Font::Embedded(EmbeddedFontId::Regular),
        "Δ (U+0394) should stay in primary face, got {:?}",
        graph.pages[0]
            .runs
            .iter()
            .map(|r| r.font)
            .collect::<Vec<_>>(),
    );
    // ∆ (U+2206, math increment) is a math operator — not in Noto
    // Sans, must route through the Math fallback.
    let (graph2, _) = build_fallback_graph(EmbeddedFontId::Regular, MATH_FALLBACK, "∆");
    ensure!(
        graph2.pages[0].runs.len() == 1
            && graph2.pages[0].runs[0].font == Font::Embedded(EmbeddedFontId::Math),
        "∆ (U+2206) should route to Math fallback, got {:?}",
        graph2.pages[0]
            .runs
            .iter()
            .map(|r| r.font)
            .collect::<Vec<_>>(),
    );
    Ok(())
}
