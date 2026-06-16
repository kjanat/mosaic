//! Integration tests for the baked Core 14 metrics. These pin the
//! contract `mos-fonts` and `mos-pdf` rely on:
//!
//! - Per-variant PDF `/BaseFont` strings (written verbatim into PDF
//!   resource dictionaries).
//! - Adobe-canonical glyph widths for `A` in Helvetica / Times /
//!   Courier; the same values the legacy hand-typed
//!   `mos-layout::metrics` table used to assert, now read through
//!   the parsed AFM instead of typed by hand.
//! - PDF `WinAnsiEncoding` byte lookups span all four bands of the
//!   table (ASCII, Win-specific, Latin-1, accented Latin), plus the
//!   `None` cases (control char and PDF-WinAnsi gap), plus the
//!   "Symbol/`ZapfDingbats` don't use `WinAnsi`" rule.

use pdf_base14_metrics::Base14Font;

// AFM widths are integers (per Adobe spec) and integer values up to
// 1015 round-trip exactly through f32, so bare `==` is safe here.
// no approximate comparison needed.

#[test]
fn helvetica_capital_a_matches_adobe_core14_afm() {
    // Adobe Helvetica.adobe-font-metrics: `C 65 ; WX 667 ; N A ; B 14 0 654 718 ;`
    // Helvetica-Bold.adobe-font-metrics: WX 722. Helvetica-Oblique.adobe-font-metrics: WX 667.
    assert_eq!(Base14Font::Helvetica.glyph_width("A"), Some(667.0));
    assert_eq!(Base14Font::HelveticaBold.glyph_width("A"), Some(722.0));
    assert_eq!(Base14Font::HelveticaOblique.glyph_width("A"), Some(667.0));
    assert_eq!(
        Base14Font::HelveticaBoldOblique.glyph_width("A"),
        Some(722.0)
    );
}

#[test]
fn times_roman_capital_a() {
    // Adobe Times-Roman.adobe-font-metrics: `C 65 ; WX 722 ; N A ; B 15 0 706 674 ;`
    assert_eq!(Base14Font::TimesRoman.glyph_width("A"), Some(722.0));
}

#[test]
fn courier_is_monospace() {
    // Every glyph in every Courier variant is 600 units wide.
    let fonts = [
        Base14Font::Courier,
        Base14Font::CourierBold,
        Base14Font::CourierOblique,
        Base14Font::CourierBoldOblique,
    ];
    for f in fonts {
        for g in ["A", "M", "i", "period", "exclam"] {
            assert_eq!(f.glyph_width(g), Some(600.0), "{f:?}/{g}");
        }
        assert!(
            f.metrics().is_fixed_pitch,
            "{f:?} should be marked is_fixed_pitch"
        );
    }
}

#[test]
fn all_fonts_have_metrics_and_bbox() {
    for f in Base14Font::ALL {
        let m = f.metrics();
        assert!(
            !m.character_metrics.is_empty(),
            "{f:?} has no character_metrics"
        );
        let bb = &m.font_bbox;
        assert!(
            bb.urx > bb.llx && bb.ury > bb.lly,
            "{f:?} font_bbox is empty: {bb:?}",
        );
    }
}

#[test]
fn text_fonts_have_ascender_and_descender() {
    // The 12 Latin text fonts must publish a positive ascender and a
    // negative descender. Symbol/ZapfDingbats are not text fonts and
    // legitimately omit these.
    let text_fonts = [
        Base14Font::Helvetica,
        Base14Font::HelveticaBold,
        Base14Font::HelveticaOblique,
        Base14Font::HelveticaBoldOblique,
        Base14Font::TimesRoman,
        Base14Font::TimesBold,
        Base14Font::TimesItalic,
        Base14Font::TimesBoldItalic,
        Base14Font::Courier,
        Base14Font::CourierBold,
        Base14Font::CourierOblique,
        Base14Font::CourierBoldOblique,
    ];
    for f in text_fonts {
        let m = f.metrics();
        assert!(m.ascender > 0.0, "{f:?} ascender = {}", m.ascender);
        assert!(m.descender < 0.0, "{f:?} descender = {}", m.descender);
    }
}

#[test]
fn pdf_base_names_pin_all_14() {
    // These bytes go verbatim into PDF resource dictionaries; mos-pdf
    // depends on the exact strings remaining stable.
    let expected = [
        (Base14Font::Helvetica, "Helvetica"),
        (Base14Font::HelveticaBold, "Helvetica-Bold"),
        (Base14Font::HelveticaOblique, "Helvetica-Oblique"),
        (Base14Font::HelveticaBoldOblique, "Helvetica-BoldOblique"),
        (Base14Font::TimesRoman, "Times-Roman"),
        (Base14Font::TimesBold, "Times-Bold"),
        (Base14Font::TimesItalic, "Times-Italic"),
        (Base14Font::TimesBoldItalic, "Times-BoldItalic"),
        (Base14Font::Courier, "Courier"),
        (Base14Font::CourierBold, "Courier-Bold"),
        (Base14Font::CourierOblique, "Courier-Oblique"),
        (Base14Font::CourierBoldOblique, "Courier-BoldOblique"),
        (Base14Font::Symbol, "Symbol"),
        (Base14Font::ZapfDingbats, "ZapfDingbats"),
    ];
    for (f, name) in expected {
        assert_eq!(f.pdf_base_name(), name);
    }
}

#[test]
fn winansi_covers_each_band() {
    let h = Base14Font::Helvetica;
    // ASCII band: 0x41 = 'A'. Helvetica 'A' = 667.
    assert_eq!(h.winansi_width(0x41), Some(667.0));
    // Win-specific band: 0x80 = Euro. Helvetica Euro = 556.
    assert_eq!(h.winansi_width(0x80), Some(556.0));
    // Win-specific band: 0x99 = trademark. Helvetica trademark = 1000.
    assert_eq!(h.winansi_width(0x99), Some(1000.0));
    // Latin-1 band: 0xA7 = section. Helvetica section = 556.
    assert_eq!(h.winansi_width(0xA7), Some(556.0));
    // Accented Latin: 0xC9 = Eacute. Helvetica Eacute = 667.
    assert_eq!(h.winansi_width(0xC9), Some(667.0));
}

#[test]
fn winansi_unmapped_returns_none() {
    let h = Base14Font::Helvetica;
    // 0x00 (NUL): no WinAnsi mapping (control character).
    assert_eq!(h.winansi_width(0x00), None);
    // 0x81: gap in PDF WinAnsi per PDF 1.7 Annex D.2 Table D.2.
    // unlike CP1252, PDF doesn't define a glyph at this slot.
    assert_eq!(h.winansi_width(0x81), None);
}

#[test]
fn winansi_returns_none_for_symbol_and_zapfdingbats() {
    // Symbol and ZapfDingbats don't use WinAnsi; every byte returns
    // `None`, even ones (like 0x20 / `"space"`) that DO have a glyph
    // by name in the font. Callers must reach for `glyph_width` on
    // the encoding-specific name instead.
    for code in [0x20_u8, 0x41, 0x80, 0xA7, 0xC9] {
        assert_eq!(
            Base14Font::Symbol.winansi_width(code),
            None,
            "Symbol 0x{code:02X}"
        );
        assert_eq!(
            Base14Font::ZapfDingbats.winansi_width(code),
            None,
            "ZapfDingbats 0x{code:02X}"
        );
    }
}

#[test]
fn winansi_nbsp_aliases_space() {
    // PDF 1.7 Annex D.2: 0xA0 (non-breaking space) renders with the
    // same glyph as 0x20 (space). The table aliases both codes to
    // the same name, so widths must agree.
    let h = Base14Font::Helvetica;
    assert_eq!(h.winansi_width(0x20), h.winansi_width(0xA0));
}

#[test]
fn winansi_soft_hyphen_aliases_hyphen() {
    // PDF 1.7 Annex D.2: 0xAD (soft hyphen) renders with the same
    // glyph as 0x2D (hyphen).
    let h = Base14Font::Helvetica;
    assert_eq!(h.winansi_width(0x2D), h.winansi_width(0xAD));
}

#[test]
fn winansi_glyph_name_exported() {
    // The encoding table is reachable via the free function so
    // downstream crates can delegate to the canonical mapping
    // without owning their own copy.
    assert_eq!(pdf_base14_metrics::winansi_glyph_name(0x41), Some("A"));
    assert_eq!(
        pdf_base14_metrics::winansi_glyph_name(0xA7),
        Some("section")
    );
    assert_eq!(pdf_base14_metrics::winansi_glyph_name(0x00), None);
    assert_eq!(pdf_base14_metrics::winansi_glyph_name(0x81), None);
}
