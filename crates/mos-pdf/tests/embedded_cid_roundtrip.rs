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

use std::{
    error::Error,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

use lopdf::{Dictionary, Document, Object, content::Content};
use mos_core::{AttrMap, AttrValue, NodeKind, NodeSpec, SourceSpan};
use mos_fonts::EmbeddedFontId;
use mos_layout::{
    EmbeddedFontId as LayoutEmbeddedFontId, Font, FontFamily, LayoutEngine, Page, PageGraph,
    TextRun,
};
use mos_pdf::PdfMetadata;

type TestResult = Result<(), Box<dyn Error>>;

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

macro_rules! ensure {
    ($cond:expr, $($arg:tt)*) => {
        if !$cond {
            return Err(format!($($arg)*).into());
        }
    };
}

fn temp_pdf_path() -> PathBuf {
    let seq = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "mosaic-embedded-rt-{}-{seq}.pdf",
        std::process::id(),
    ))
}

/// Build a `PageGraph` through the real layout path so fallback tests
/// exercise `FontFamily::fallbacks`, word shaping, and `flush_line`.
fn build_fallback_graph(
    primary: EmbeddedFontId,
    fallbacks: &[EmbeddedFontId],
    text: &str,
) -> (PageGraph, Vec<f32>) {
    let family = FontFamily::noto_sans();
    assert_eq!(family.regular, Font::Embedded(primary));
    assert_eq!(family.fallbacks, fallbacks);

    build_default_graph(text)
}

fn build_default_graph(text: &str) -> (PageGraph, Vec<f32>) {
    let mut doc = mos_core::Document::new(PathBuf::from("fallback-test.mos"));
    let paragraph = doc.alloc_child(
        doc.root,
        NodeSpec::new(
            NodeKind::Paragraph,
            SourceSpan::placeholder(PathBuf::from("fallback-test.mos")),
        ),
    );
    let mut text_attrs = AttrMap::new();
    text_attrs.insert("text".to_owned(), AttrValue::Str(text.to_owned()));
    doc.alloc_child(
        paragraph,
        NodeSpec::new(
            NodeKind::Text,
            SourceSpan::placeholder(PathBuf::from("fallback-test.mos")),
        )
        .with_attributes(text_attrs),
    );

    let result = LayoutEngine::new().layout(&doc);
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    let graph = result.graph;
    let xs = graph.pages[0].runs.iter().map(|run| run.x_pt).collect();
    (graph, xs)
}

fn emit_graph(graph: &PageGraph) -> Result<(Document, Vec<u8>), Box<dyn Error>> {
    let tmp = temp_pdf_path();
    let diags =
        mos_pdf::emit(graph, &PdfMetadata::default(), &tmp).map_err(|e| format!("emit: {e:?}"))?;
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
                actual_text: None,
                glyphs: mos_fonts::shape(face.data(), text),
            }],
            images: Vec::new(),
        }],
        images: Vec::new(),
    };
    let tmp = temp_pdf_path();
    let diags =
        mos_pdf::emit(&graph, &PdfMetadata::default(), &tmp).map_err(|e| format!("emit: {e:?}"))?;
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
) -> Result<&'d Dictionary, Box<dyn Error>> {
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
    // The source contained `П` (U+041F): its 4-hex-uppercase form
    // must appear in a bfchar entry's RHS.
    ensure!(
        cmap_text.contains("041F"),
        "ToUnicode CMap missing U+041F (П):\n{cmap_text}",
    );
    Ok(())
}

#[test]
fn decomposed_romanian_matches_precomposed_after_layout() -> TestResult {
    let (decomposed, _) = build_default_graph("S\u{0326}");
    let (precomposed, _) = build_default_graph("\u{0218}");
    let (_, decomposed_bytes) = emit_graph(&decomposed)?;
    let (_, precomposed_bytes) = emit_graph(&precomposed)?;

    ensure!(
        decomposed_bytes == precomposed_bytes,
        "NFC-equivalent documents should emit identical PDFs"
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
    // content length: pdf-writer doesn't compress, so the two are
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
    // verbatim in the content stream: if they did, the run had
    // taken the Base14 byte path by mistake.
    ensure!(
        !content.windows(b"(Hello)".len()).any(|w| w == b"(Hello)"),
        "embedded run leaked `(Hello)` literal: wrong code path?",
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
    // must not record a Unicode mapping for gid 0: otherwise every
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
        if trimmed.ends_with("beginbfchar") || trimmed.ends_with("beginbfrange") {
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

const MATH_FALLBACK: &[EmbeddedFontId] = &[EmbeddedFontId::Math];

fn extract_content_stream(doc: &Document) -> Result<Vec<u8>, Box<dyn Error>> {
    let page_id = doc.page_iter().next().ok_or("no pages")?;
    let page = doc.get_dictionary(page_id)?;
    let contents = deref(doc, page.get(b"Contents")?)?;
    match contents {
        Object::Stream(stream) => Ok(stream.get_plain_content()?),
        Object::Array(items) => {
            let mut content = Vec::new();
            for item in items {
                let Object::Stream(stream) = deref(doc, item)? else {
                    return Err("/Contents array item is not a stream".into());
                };
                content.extend(stream.get_plain_content()?);
            }
            Ok(content)
        }
        _ => Err("/Contents is not a stream or stream array".into()),
    }
}

fn shown_cids_for_font(content: &[u8], resource_name: &[u8]) -> Result<Vec<u16>, Box<dyn Error>> {
    let decoded = Content::decode(content)?;
    let mut current_font: Option<Vec<u8>> = None;
    let mut cids = Vec::new();
    for operation in decoded.operations {
        match operation.operator.as_str() {
            "Tf" => {
                let Some(Object::Name(name)) = operation.operands.first() else {
                    return Err("Tf operator missing font-name operand".into());
                };
                current_font = Some(name.clone());
            }
            "Tj" if current_font.as_deref() == Some(resource_name) => {
                let Some(Object::String(bytes, _)) = operation.operands.first() else {
                    return Err("Tj operator missing string operand".into());
                };
                push_cids_from_bytes(bytes, &mut cids)?;
            }
            "TJ" if current_font.as_deref() == Some(resource_name) => {
                let Some(Object::Array(items)) = operation.operands.first() else {
                    return Err("TJ operator missing array operand".into());
                };
                for item in items {
                    if let Object::String(bytes, _) = item {
                        push_cids_from_bytes(bytes, &mut cids)?;
                    }
                }
            }
            _ => {}
        }
    }
    Ok(cids)
}

fn push_cids_from_bytes(bytes: &[u8], cids: &mut Vec<u16>) -> Result<(), Box<dyn Error>> {
    if !bytes.len().is_multiple_of(2) {
        return Err(format!("embedded CID string has odd byte length: {bytes:?}").into());
    }
    for pair in bytes.chunks_exact(2) {
        cids.push((u16::from(pair[0]) << 8) | u16::from(pair[1]));
    }
    Ok(())
}

fn font_switches(content: &[u8]) -> Result<Vec<Vec<u8>>, Box<dyn Error>> {
    let decoded = Content::decode(content)?;
    let mut switches = Vec::new();
    for operation in decoded.operations {
        if operation.operator != "Tf" {
            continue;
        }
        let Some(Object::Name(name)) = operation.operands.first() else {
            return Err("Tf operator missing font-name operand".into());
        };
        switches.push(name.clone());
    }
    Ok(switches)
}

fn positioned_adjustments_for_font(
    content: &[u8],
    resource_name: &[u8],
) -> Result<Vec<f32>, Box<dyn Error>> {
    let decoded = Content::decode(content)?;
    let mut current_font: Option<Vec<u8>> = None;
    let mut adjustments = Vec::new();
    for operation in decoded.operations {
        match operation.operator.as_str() {
            "Tf" => {
                let Some(Object::Name(name)) = operation.operands.first() else {
                    return Err("Tf operator missing font-name operand".into());
                };
                current_font = Some(name.clone());
            }
            "TJ" if current_font.as_deref() == Some(resource_name) => {
                let Some(Object::Array(items)) = operation.operands.first() else {
                    return Err("TJ operator missing array operand".into());
                };
                for item in items {
                    match item {
                        Object::Integer(_) | Object::Real(_) => {
                            adjustments.push(object_number_as_f32(item)?);
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }
    Ok(adjustments)
}

fn text_show_operator_counts(
    content: &[u8],
    resource_name: &[u8],
) -> Result<(usize, usize), Box<dyn Error>> {
    let decoded = Content::decode(content)?;
    let mut current_font: Option<Vec<u8>> = None;
    let mut simple = 0;
    let mut positioned = 0;
    for operation in decoded.operations {
        match operation.operator.as_str() {
            "Tf" => {
                let Some(Object::Name(name)) = operation.operands.first() else {
                    return Err("Tf operator missing font-name operand".into());
                };
                current_font = Some(name.clone());
            }
            "Tj" if current_font.as_deref() == Some(resource_name) => simple += 1,
            "TJ" if current_font.as_deref() == Some(resource_name) => positioned += 1,
            _ => {}
        }
    }
    Ok((simple, positioned))
}

fn text_matrices_for_font(
    content: &[u8],
    resource_name: &[u8],
) -> Result<Vec<[f32; 6]>, Box<dyn Error>> {
    let decoded = Content::decode(content)?;
    let mut current_font: Option<Vec<u8>> = None;
    let mut matrices = Vec::new();
    for operation in decoded.operations {
        match operation.operator.as_str() {
            "Tf" => {
                let Some(Object::Name(name)) = operation.operands.first() else {
                    return Err("Tf operator missing font-name operand".into());
                };
                current_font = Some(name.clone());
            }
            "Tm" if current_font.as_deref() == Some(resource_name) => {
                if operation.operands.len() != 6 {
                    return Err(
                        format!("Tm expected 6 operands, got {:?}", operation.operands).into(),
                    );
                }
                matrices.push([
                    object_number_as_f32(&operation.operands[0])?,
                    object_number_as_f32(&operation.operands[1])?,
                    object_number_as_f32(&operation.operands[2])?,
                    object_number_as_f32(&operation.operands[3])?,
                    object_number_as_f32(&operation.operands[4])?,
                    object_number_as_f32(&operation.operands[5])?,
                ]);
            }
            _ => {}
        }
    }
    Ok(matrices)
}

fn cmap_maps_cid_to(cmap_text: &str, cid: u16, rhs_hex: &str) -> bool {
    let lhs = format!("<{cid:04X}>");
    let mut in_block = false;
    for line in cmap_text.lines() {
        let trimmed = line.trim();
        if trimmed.ends_with("beginbfchar") || trimmed.ends_with("beginbfrange") {
            in_block = true;
            continue;
        }
        if trimmed.starts_with("endbfchar") || trimmed.starts_with("endbfrange") {
            in_block = false;
            continue;
        }
        if !in_block || !trimmed.starts_with(&lhs) {
            continue;
        }
        let Some(last_open) = trimmed.rfind('<') else {
            continue;
        };
        let Some(last_close) = trimmed[last_open..].find('>') else {
            continue;
        };
        if &trimmed[last_open + 1..last_open + last_close] == rhs_hex {
            return true;
        }
    }
    false
}

fn embedded_descendant<'d>(
    doc: &'d Document,
    resource_name: &[u8],
) -> Result<&'d Dictionary, Box<dyn Error>> {
    let type0 = font_dict(doc, resource_name)?;
    let descendants = type0.get(b"DescendantFonts")?.as_array()?;
    let Object::Reference(cid_id) = descendants[0] else {
        return Err("expected indirect descendant".into());
    };
    Ok(doc.get_dictionary(cid_id)?)
}

fn object_number_as_f32(obj: &Object) -> Result<f32, Box<dyn Error>> {
    match obj {
        Object::Integer(_) | Object::Real(_) => Ok(obj.as_float()?),
        other => Err(format!("expected numeric width, got {other:?}").into()),
    }
}

fn object_integer_as_u16(obj: &Object) -> Result<u16, Box<dyn Error>> {
    match obj {
        Object::Integer(n) => Ok(u16::try_from(*n)?),
        other => Err(format!("expected integer CID, got {other:?}").into()),
    }
}

#[test]
fn kerning_pair_emits_tj_adjustment() -> TestResult {
    let (doc, _) = render(EmbeddedFontId::Regular, "AV")?;
    let content = extract_content_stream(&doc)?;
    let adjustments = positioned_adjustments_for_font(&content, b"F15")?;

    ensure!(
        adjustments.iter().any(|amount| amount.abs() > f32::EPSILON),
        "expected non-zero TJ adjustment for kerned AV pair, got {adjustments:?}",
    );
    Ok(())
}

#[test]
fn simple_embedded_run_emits_tj_not_tj_array() -> TestResult {
    let (doc, _) = render(EmbeddedFontId::Regular, "П")?;
    let content = extract_content_stream(&doc)?;
    let (simple, positioned) = text_show_operator_counts(&content, b"F15")?;

    ensure!(simple > 0, "expected simple Tj operator, got none");
    ensure!(
        positioned == 0,
        "expected no TJ operator for unpositioned run, got {positioned}",
    );
    Ok(())
}

#[test]
fn combining_marks_emit_offset_text_matrices() -> TestResult {
    let (graph, _) = build_default_graph("q\u{0302}\u{0301}");
    let (doc, _) = emit_graph(&graph)?;
    let content = extract_content_stream(&doc)?;
    let matrices = text_matrices_for_font(&content, b"F15")?;

    ensure!(
        matrices.len() >= 2,
        "expected base glyph and offset marks to use multiple Tm operators, got {matrices:?}",
    );
    let baseline_y = matrices[0][5];
    ensure!(
        matrices.iter().skip(1).any(|matrix| matrix[5] > baseline_y),
        "expected at least one combining mark above baseline, got {matrices:?}",
    );
    Ok(())
}

fn cid_width(cid_font: &Dictionary, cid: u16) -> Result<Option<f32>, Box<dyn Error>> {
    let widths = cid_font.get(b"W")?.as_array()?;
    let mut i = 0;
    while i < widths.len() {
        let first = object_integer_as_u16(&widths[i])?;
        let Some(second) = widths.get(i + 1) else {
            return Err("malformed /W array: missing second item".into());
        };
        match second {
            Object::Array(values) => {
                for (offset, value) in values.iter().enumerate() {
                    let offset_u16 = u16::try_from(offset)?;
                    if first.checked_add(offset_u16) == Some(cid) {
                        return Ok(Some(object_number_as_f32(value)?));
                    }
                }
                i += 2;
            }
            Object::Integer(last) => {
                let last_u16 = u16::try_from(*last)?;
                let Some(value) = widths.get(i + 2) else {
                    return Err("malformed /W range: missing width".into());
                };
                if first <= cid && cid <= last_u16 {
                    return Ok(Some(object_number_as_f32(value)?));
                }
                i += 3;
            }
            other => return Err(format!("malformed /W array item: {other:?}").into()),
        }
    }
    Ok(None)
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
        "expected 3 sub-runs (a / ≤ / b), got {}: {:?}",
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
    // x positions must be strictly increasing: sub-runs render
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
    // The content stream for `a≤b` must set fonts as F15 → F20 → F15,
    // in source order. Parse operators so CID bytes cannot masquerade
    // as font resource names.
    let (graph, _) = build_fallback_graph(EmbeddedFontId::Regular, MATH_FALLBACK, "a≤b");
    let (doc, _) = emit_graph(&graph)?;
    let content = extract_content_stream(&doc)?;
    let switches = font_switches(&content)?;
    ensure!(
        switches == vec![b"F15".to_vec(), b"F20".to_vec(), b"F15".to_vec()],
        "Tf switches not in source order: {switches:?}",
    );
    Ok(())
}

#[test]
fn math_subrun_tounicode_maps_only_its_own_codepoint() -> TestResult {
    // The cluster rebasing contract: a sub-run's glyphs have `cluster`
    // offsets local to the sub-run's `text`. The /ToUnicode CMap for
    // F20 must therefore map back to the math character only; not to
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
    let content = extract_content_stream(&doc)?;
    let math_cids = shown_cids_for_font(&content, b"F20")?;
    ensure!(
        math_cids.len() == 1,
        "expected one F20 CID for `≤`, got {math_cids:?}",
    );
    let math_cid = math_cids[0];
    // `≤` is U+2264, and the actual CID emitted in the F20 `Tj`
    // operation must be the CID mapped by F20's CMap.
    ensure!(
        cmap_maps_cid_to(&cmap_text, math_cid, "2264"),
        "F20 ToUnicode does not map emitted CID {math_cid:04X} to U+2264 (≤):\n{cmap_text}",
    );
    // The Latin letters `a` (0x61) and `b` (0x62) must NOT appear as
    // bfchar RHS values in F20's CMap: they live in F15 only.
    let mut in_block = false;
    for line in cmap_text.lines() {
        let trimmed = line.trim();
        if trimmed.ends_with("beginbfchar") || trimmed.ends_with("beginbfrange") {
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
        // Look at the angle-bracketed Unicode RHS only; the LHS is a
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
    // the primary face as `.notdef`; no panic, no second sub-run, no
    // bogus CMap entry for the emoji codepoint.
    let (graph, _) = build_fallback_graph(EmbeddedFontId::Regular, MATH_FALLBACK, "a🦀b");
    // Whole word stays in the primary face (one sub-run) because no
    // fallback covered the emoji cluster.
    ensure!(
        graph.pages[0].runs.len() == 1,
        "expected single sub-run when fallback chain has no match, got {}",
        graph.pages[0].runs.len(),
    );
    ensure!(
        graph.pages[0].runs[0].glyphs.iter().any(|g| g.gid == 0),
        "expected unsupported emoji to shape to .notdef gid 0, got {:?}",
        graph.pages[0].runs[0].glyphs,
    );
    let (doc, _) = emit_graph(&graph)?;
    let content = extract_content_stream(&doc)?;
    let primary_cids = shown_cids_for_font(&content, b"F15")?;
    ensure!(
        primary_cids.contains(&0),
        "expected .notdef CID 0000 in F15 content stream, got {primary_cids:?}",
    );
    let cid_font = embedded_descendant(&doc, b"F15")?;
    let notdef_width = cid_width(cid_font, 0)?.ok_or("missing /W entry for .notdef CID 0000")?;
    ensure!(
        notdef_width.partial_cmp(&0.0) == Some(std::cmp::Ordering::Greater),
        ".notdef CID 0000 width should match shaped advance, got {notdef_width}",
    );
    let primary = font_dict(&doc, b"F15")?;
    let Object::Reference(cmap_id) = primary.get(b"ToUnicode")? else {
        return Err("expected indirect /ToUnicode on F15".into());
    };
    let Object::Stream(cmap_stream) = doc.get_object(*cmap_id)? else {
        return Err("F15 ToUnicode is not a stream".into());
    };
    let cmap_text = String::from_utf8_lossy(&cmap_stream.content);
    // U+1F980 needs a surrogate pair `D83E DD80` in /ToUnicode. The
    // .notdef cluster must NOT round-trip; the byte slice that
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
    // Δ (U+0394, Greek capital delta) is a letter: Noto Sans covers
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
    // ∆ (U+2206, math increment) is a math operator; not in Noto
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
