//! Integration tests for the AFM parser.
//!
//! Real Adobe Core 14 AFM fixtures live in `tests/fixtures/`,
//! vendored from `tecnickcom/tc-font-core14-afms` (see
//! `LICENSE-Adobe-Core14-AFM` alongside them). They are pinned
//! per-crate rather than shared with `pdf-base14-metrics` so each
//! crate stays independently buildable.

#![allow(
    clippy::panic,
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::float_cmp_const,
    clippy::items_after_statements
)]

use afm::{ParseError, parse};

const HELVETICA: &str = include_str!("fixtures/Helvetica.afm");
const COURIER: &str = include_str!("fixtures/Courier.afm");
const TIMES_ROMAN: &str = include_str!("fixtures/Times-Roman.afm");

fn glyph_width(metrics: &afm::FontMetrics<'_>, name: &str) -> Option<f32> {
    metrics
        .character_metrics
        .iter()
        .find(|m| m.name == name)
        .map(|m| m.width_x)
}

#[test]
fn parses_helvetica() {
    let m = parse(HELVETICA).expect("Helvetica.afm should parse");
    assert_eq!(m.font_name, "Helvetica");
    assert_eq!(m.full_name, "Helvetica");
    assert_eq!(m.family_name, "Helvetica");
    assert!(
        m.character_metrics.len() > 200,
        "expected > 200 glyphs, got {}",
        m.character_metrics.len()
    );
    // Adobe Helvetica.afm: `C 65 ; WX 667 ; N A ; B 14 0 654 718 ;`
    assert_eq!(glyph_width(&m, "A"), Some(667.0));
    // Ascender / descender pinning (matches mosaic-layout::metrics).
    assert!((m.ascender - 718.0).abs() < f32::EPSILON);
    assert!((m.descender - -207.0).abs() < f32::EPSILON);
}

#[test]
fn parses_courier_monospace() {
    let m = parse(COURIER).expect("Courier.afm should parse");
    assert_eq!(m.font_name, "Courier");
    assert!(m.is_fixed_pitch, "Courier must be marked fixed pitch");
    assert!(m.character_metrics.len() > 200);
    // Every glyph in Courier is 600 units wide.
    assert_eq!(glyph_width(&m, "A"), Some(600.0));
    // Spot-check another to confirm the monospace property holds via parsing.
    assert_eq!(glyph_width(&m, "M"), Some(600.0));
    assert_eq!(glyph_width(&m, "i"), Some(600.0));
}

#[test]
fn parses_times_roman() {
    let m = parse(TIMES_ROMAN).expect("Times-Roman.afm should parse");
    assert_eq!(m.font_name, "Times-Roman");
    assert_eq!(m.family_name, "Times");
    assert!(m.character_metrics.len() > 200);
    assert_eq!(glyph_width(&m, "A"), Some(722.0));
}

#[test]
fn times_roman_carries_kerning() {
    // Times-Roman.afm has 2073 KPX records — pick a stable, well-known
    // pair: "A" / "V" tightens to -80 in Adobe's AFM.
    let m = parse(TIMES_ROMAN).expect("Times-Roman.afm should parse");
    assert!(
        !m.kerning_pairs.is_empty(),
        "expected non-empty kerning_pairs"
    );
    let av = m
        .kerning_pairs
        .iter()
        .find(|kp| kp.left == "A" && kp.right == "V");
    assert!(av.is_some(), "expected a KPX A V record");
    let kp = av.expect("just asserted Some");
    assert!(kp.adjust < 0.0, "A/V kern should be negative, got {}", kp.adjust);
}

#[test]
fn into_owned_roundtrip_preserves_data() {
    let borrowed = parse(HELVETICA).expect("parse");
    let cloned = borrowed.clone();
    let owned = borrowed.into_owned();
    // PartialEq works across the lifetime boundary because the
    // contents compare structurally, not by Cow tag.
    assert_eq!(cloned, owned);
    // Sanity: owned really is 'static.
    fn is_static<T: 'static>(_: &T) {}
    is_static(&owned);
}

#[test]
fn malformed_number_carries_line_context() {
    // Line 2 contains a non-numeric FontBBox.
    let src = "StartFontMetrics 4.1\n\
               FontBBox not a number\n\
               FontName Bad\n\
               EndFontMetrics\n";
    let err = parse(src).expect_err("must fail");
    match err {
        ParseError::InvalidNumber { line, field, .. }
        | ParseError::MalformedRecord {
            line,
            keyword: field,
            ..
        } => {
            assert_eq!(line, 2, "error must point at line 2");
            assert_eq!(field, "FontBBox");
        }
        other => panic!("unexpected error variant: {other:?}"),
    }
}

#[test]
fn rejects_unsupported_version() {
    let src = "StartFontMetrics 5.0\nFontName X\nFontBBox 0 0 0 0\nEndFontMetrics\n";
    let err = parse(src).expect_err("must fail on v5");
    match err {
        ParseError::UnsupportedVersion { line, version } => {
            assert_eq!(line, 1);
            assert_eq!(version, "5.0");
        }
        other => panic!("unexpected error: {other:?}"),
    }
}

#[test]
fn rejects_missing_header() {
    let src = "FontName Bad\nFontBBox 0 0 0 0\nEndFontMetrics\n";
    let err = parse(src).expect_err("must fail");
    match err {
        ParseError::MissingHeader { line } => assert_eq!(line, 1),
        other => panic!("unexpected error: {other:?}"),
    }
}

#[test]
fn rejects_missing_required_fields() {
    let src = "StartFontMetrics 4.1\nEndFontMetrics\n";
    let err = parse(src).expect_err("must fail");
    match err {
        ParseError::MissingRequiredField { field } => assert_eq!(field, "FontName"),
        other => panic!("unexpected error: {other:?}"),
    }
}
