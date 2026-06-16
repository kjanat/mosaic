//! Integration tests for the AFM parser.
//!
//! Real Adobe Core 14 AFM fixtures live in `tests/fixtures/`,
//! vendored from `tecnickcom/tc-font-core14-afms` (see
//! `LICENSE-Adobe-Core14-AFM` alongside them). They are pinned
//! per-crate rather than shared with `pdf-base14-metrics` so each
//! crate stays independently buildable.

use adobe_font_metrics::{FontMetrics, ParseError, parse};

const HELVETICA: &str = include_str!("fixtures/Helvetica.afm");
const COURIER: &str = include_str!("fixtures/Courier.afm");
const TIMES_ROMAN: &str = include_str!("fixtures/Times-Roman.afm");

fn glyph_width(metrics: &FontMetrics<'_>, name: &str) -> Option<f32> {
    metrics
        .character_metrics
        .iter()
        .find(|m| m.name == name)
        .map(|m| m.width_x)
}

fn approx_eq(a: f32, b: f32) -> bool {
    (a - b).abs() < f32::EPSILON
}

fn is_static<T: 'static>(_: &T) {}

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
    let a = glyph_width(&m, "A");
    assert!(
        a.is_some_and(|w| approx_eq(w, 667.0)),
        "Helvetica A width should be 667, got {a:?}"
    );
    // Ascender / descender pinning (matches mos-layout::metrics).
    assert!(approx_eq(m.ascender, 718.0));
    assert!(approx_eq(m.descender, -207.0));
}

#[test]
fn parses_courier_monospace() {
    let m = parse(COURIER).expect("Courier.afm should parse");
    assert_eq!(m.font_name, "Courier");
    assert!(m.is_fixed_pitch, "Courier must be marked fixed pitch");
    assert!(m.character_metrics.len() > 200);
    // Every glyph in Courier is 600 units wide.
    for name in ["A", "M", "i"] {
        let w = glyph_width(&m, name);
        assert!(
            w.is_some_and(|x| approx_eq(x, 600.0)),
            "Courier {name} should be 600, got {w:?}",
        );
    }
}

#[test]
fn parses_times_roman() {
    let m = parse(TIMES_ROMAN).expect("Times-Roman.afm should parse");
    assert_eq!(m.font_name, "Times-Roman");
    assert_eq!(m.family_name, "Times");
    assert!(m.character_metrics.len() > 200);
    let a = glyph_width(&m, "A");
    assert!(
        a.is_some_and(|w| approx_eq(w, 722.0)),
        "Times-Roman A should be 722, got {a:?}"
    );
}

#[test]
fn times_roman_carries_kerning() {
    let m = parse(TIMES_ROMAN).expect("Times-Roman.afm should parse");
    assert!(!m.kerning_pairs.is_empty());
    // "A" / "V" tightens to a negative value in Adobe's Times-Roman.afm.
    let av = m
        .kerning_pairs
        .iter()
        .find(|kp| kp.left == "A" && kp.right == "V");
    assert!(
        av.is_some_and(|kp| kp.adjust < 0.0),
        "A/V kern should exist and be negative, got {av:?}"
    );
}

#[test]
fn into_owned_roundtrip_preserves_data() {
    let borrowed = parse(HELVETICA).expect("parse");
    let cloned = borrowed.clone();
    let owned = borrowed.into_owned();
    // PartialEq works across the lifetime boundary because the
    // contents compare structurally, not by `Cow` tag.
    assert_eq!(cloned, owned);
    // Sanity: owned really is 'static.
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
    assert!(
        matches!(
            err,
            ParseError::InvalidNumber {
                line: 2,
                field: "FontBBox",
                ..
            } | ParseError::MalformedRecord {
                line: 2,
                keyword: "FontBBox",
                ..
            }
        ),
        "error must point at line 2 / FontBBox; got {err:?}"
    );
}

#[test]
fn rejects_unsupported_version() {
    let src = "StartFontMetrics 5.0\nFontName X\nFontBBox 0 0 0 0\nEndFontMetrics\n";
    assert!(matches!(
        parse(src),
        Err(ParseError::UnsupportedVersion { line: 1, ref version }) if version == "5.0"
    ));
}

#[test]
fn rejects_missing_header() {
    let src = "FontName Bad\nFontBBox 0 0 0 0\nEndFontMetrics\n";
    assert!(matches!(
        parse(src),
        Err(ParseError::MissingHeader { line: 1 })
    ));
}

#[test]
fn rejects_missing_required_fields() {
    let src = "StartFontMetrics 4.1\nEndFontMetrics\n";
    assert!(matches!(
        parse(src),
        Err(ParseError::MissingRequiredField { field: "FontName" })
    ));
}

#[test]
fn rejects_malformed_version_minor() {
    // `4.x`, `4.bad`, `4.` should all fail; not just non-`4.` prefixes.
    for bad in ["4.x", "4.bad", "4."] {
        let src = format!("StartFontMetrics {bad}\nFontName X\nFontBBox 0 0 0 0\nEndFontMetrics\n");
        assert!(
            matches!(
                parse(&src),
                Err(ParseError::UnsupportedVersion { line: 1, ref version }) if version == bad
            ),
            "expected UnsupportedVersion for {bad:?}"
        );
    }
}

#[test]
fn rejects_bare_w_in_char_metric() {
    // `W` without an x operand must surface as MalformedRecord, not be
    // silently normalised to width_x = 0.0.
    let src = "StartFontMetrics 4.1\n\
               FontName Bad\n\
               FontBBox 0 0 0 0\n\
               StartCharMetrics 1\n\
               C 65 ; W ; N A ;\n\
               EndCharMetrics\n\
               EndFontMetrics\n";
    assert!(matches!(
        parse(src),
        Err(ParseError::MalformedRecord {
            line: 5,
            keyword: "W",
            ..
        })
    ));
}

#[test]
fn rejects_kpy_with_non_numeric_operand() {
    // `KPY A V nope` must fail: operand has to be numeric even though
    // KPY's value is discarded for the public x-only `adjust` field.
    let src = "StartFontMetrics 4.1\n\
               FontName Bad\n\
               FontBBox 0 0 0 0\n\
               StartKernPairs 1\n\
               KPY A V nope\n\
               EndKernPairs\n\
               EndFontMetrics\n";
    assert!(matches!(
        parse(src),
        Err(ParseError::InvalidNumber {
            line: 5,
            field: "KPY",
            ..
        })
    ));
}

#[test]
fn rejects_bbox_with_trailing_garbage() {
    // `FontBBox 0 0 0 0 junk` must fail: wrong arity, not silently truncated.
    let src = "StartFontMetrics 4.1\n\
               FontName Bad\n\
               FontBBox 0 0 0 0 junk\n\
               EndFontMetrics\n";
    assert!(matches!(
        parse(src),
        Err(ParseError::MalformedRecord {
            line: 3,
            keyword: "FontBBox",
            ..
        })
    ));
}

#[test]
fn rejects_kpx_with_trailing_garbage() {
    // `KPX A V -80 junk` must fail.
    let src = "StartFontMetrics 4.1\n\
               FontName Bad\n\
               FontBBox 0 0 0 0\n\
               StartKernPairs 1\n\
               KPX A V -80 junk\n\
               EndKernPairs\n\
               EndFontMetrics\n";
    assert!(matches!(
        parse(src),
        Err(ParseError::MalformedRecord {
            line: 5,
            keyword: "KPX",
            ..
        })
    ));
}

#[test]
fn rejects_kp_missing_y_operand() {
    // `KP A V 5` is missing the required y adjustment.
    let src = "StartFontMetrics 4.1\n\
               FontName Bad\n\
               FontBBox 0 0 0 0\n\
               StartKernPairs 1\n\
               KP A V 5\n\
               EndKernPairs\n\
               EndFontMetrics\n";
    assert!(matches!(
        parse(src),
        Err(ParseError::MalformedRecord {
            line: 5,
            keyword: "KP",
            ..
        })
    ));
}

#[test]
fn accepts_multibyte_ch_code() {
    // `CH <8000>` (= 32768) is just past `i16::MAX`. Real CJK AFMs
    // routinely use codes in the 0x2100–0x7FFF and 0x8000+ ranges.
    let src = "StartFontMetrics 4.1\n\
               FontName Test\n\
               FontBBox 0 0 0 0\n\
               StartCharMetrics 1\n\
               CH <8000> ; WX 500 ; N test ;\n\
               EndCharMetrics\n\
               EndFontMetrics\n";
    let m = parse(src).expect("parse");
    assert_eq!(m.character_metrics.len(), 1);
    assert_eq!(m.character_metrics[0].code, 0x8000);
}

#[test]
fn direction_1_kerns_dropped_not_conflated() {
    // Without the `StartKernPairs1` skip, the second KPX would either
    // be appended to the same vector (silent conflation) or silently
    // dropped depending on which path the state machine took.
    // Either way the output misrepresents the AFM. Here we pin the
    // intended behaviour: only the direction-0 pair survives.
    let src = "StartFontMetrics 4.1\n\
               FontName Test\n\
               FontBBox 0 0 0 0\n\
               StartKernData\n\
               StartKernPairs0 1\n\
               KPX A V -80\n\
               EndKernPairs\n\
               StartKernPairs1 1\n\
               KPX A V -999\n\
               EndKernPairs\n\
               EndKernData\n\
               EndFontMetrics\n";
    let m = parse(src).expect("parse");
    assert_eq!(m.kerning_pairs.len(), 1);
    assert!((m.kerning_pairs[0].adjust - -80.0).abs() < f32::EPSILON);
}

#[test]
fn non_zero_start_direction_does_not_clobber() {
    // `StartDirection 1` carries direction-1 metrics. Without the
    // skip, `UnderlinePosition -999` inside the block would overwrite
    // the top-level `-100` we already read for direction-0.
    let src = "StartFontMetrics 4.1\n\
               FontName Test\n\
               FontBBox 0 0 0 0\n\
               UnderlinePosition -100\n\
               StartDirection 1\n\
               UnderlinePosition -999\n\
               EndDirection\n\
               EndFontMetrics\n";
    let m = parse(src).expect("parse");
    assert!((m.underline_position - -100.0).abs() < f32::EPSILON);
}
