//! Font discovery, shaping, and metrics (manifest §22.1).
//!
//! MVP-2 facade over [`pdf_base14_metrics`]: a [`Font`] newtype around
//! [`Base14Font`] plus the width/ascent/descent helpers `mosaic-layout`
//! needs. The newtype gives Mosaic a single anchor point to refactor when
//! issue #9 widens `Font` to `Base14(Base14Font) | Embedded(EmbeddedFont)`;
//! that widening will still be a breaking change for call sites that touch
//! `.0`, the newtype just keeps those call sites localised.
//!
//! Only printable ASCII (`0x20..=0x7E`) is measurable here. Callers must
//! substitute anything else upstream — the layout engine emits `W040` and
//! rewrites to `?` at a single boundary so non-ASCII never reaches
//! [`text_width`]. Broader Unicode → `WinAnsi` coverage is issue #8.

#![deny(missing_docs)]

pub use pdf_base14_metrics::Base14Font;

/// One of the 14 standard PDF fonts, wrapped in a newtype.
///
/// Use [`Font::ALL`] to iterate every face in Mosaic's PDF-resource order,
/// or `Font::from(Base14Font::Helvetica)` to construct directly.
#[derive(Copy, Clone, Eq, PartialEq, Debug, Hash)]
pub struct Font(pub Base14Font);

impl Font {
    /// All 14 base fonts in **Mosaic's** PDF-resource order.
    ///
    /// This ordering is deliberately **decoupled** from [`Base14Font::ALL`]:
    /// the four pre-existing layout faces keep their historical
    /// `F1`..=`F4` resource numbers (`Helvetica`, `HelveticaBold`,
    /// `HelveticaOblique`, `Courier`) so PDF byte output stays stable for
    /// existing integration tests. The remaining ten faces follow.
    /// [`Base14Font::ALL`] is the canonical PDF-spec ordering; this one is
    /// Mosaic-specific.
    pub const ALL: [Self; 14] = [
        Self(Base14Font::Helvetica),
        Self(Base14Font::HelveticaBold),
        Self(Base14Font::HelveticaOblique),
        Self(Base14Font::Courier),
        Self(Base14Font::HelveticaBoldOblique),
        Self(Base14Font::TimesRoman),
        Self(Base14Font::TimesBold),
        Self(Base14Font::TimesItalic),
        Self(Base14Font::TimesBoldItalic),
        Self(Base14Font::CourierBold),
        Self(Base14Font::CourierOblique),
        Self(Base14Font::CourierBoldOblique),
        Self(Base14Font::Symbol),
        Self(Base14Font::ZapfDingbats),
    ];

    /// PDF `/BaseFont` name, e.g. `"Helvetica-BoldOblique"`.
    #[must_use]
    pub fn pdf_base_name(self) -> &'static str {
        self.0.pdf_base_name()
    }

    /// Stable resource name (`F1`..`F14`) written into the page's font
    /// dictionary. PDF allows any name; using fixed `Fn` identifiers keeps
    /// streams short and byte-stable across builds. The mapping matches
    /// [`Font::ALL`] index + 1.
    #[must_use]
    pub fn pdf_resource_name(self) -> &'static [u8] {
        match self.0 {
            Base14Font::Helvetica => b"F1",
            Base14Font::HelveticaBold => b"F2",
            Base14Font::HelveticaOblique => b"F3",
            Base14Font::Courier => b"F4",
            Base14Font::HelveticaBoldOblique => b"F5",
            Base14Font::TimesRoman => b"F6",
            Base14Font::TimesBold => b"F7",
            Base14Font::TimesItalic => b"F8",
            Base14Font::TimesBoldItalic => b"F9",
            Base14Font::CourierBold => b"F10",
            Base14Font::CourierOblique => b"F11",
            Base14Font::CourierBoldOblique => b"F12",
            Base14Font::Symbol => b"F13",
            Base14Font::ZapfDingbats => b"F14",
        }
    }
}

impl From<Base14Font> for Font {
    fn from(f: Base14Font) -> Self {
        Self(f)
    }
}

impl From<Font> for Base14Font {
    fn from(f: Font) -> Self {
        f.0
    }
}

/// Advance width of `text` rendered in `font` at `size` points, in PDF
/// user-space units.
///
/// # Panics
///
/// Panics if any character in `text` is outside printable ASCII
/// (`0x20..=0x7E`); callers must substitute non-ASCII upstream. Also
/// panics if `font` has no `WinAnsi` mapping (`Symbol`, `ZapfDingbats`).
#[must_use]
pub fn text_width(font: Font, size: f32, text: &str) -> f32 {
    let mut units: f32 = 0.0;
    for ch in text.chars() {
        units += glyph_width_units(font, ch);
    }
    units * size / 1000.0
}

/// Width of a single glyph in `font` at `size` points.
///
/// # Panics
///
/// Panics if `ch` is outside printable ASCII (`0x20..=0x7E`), or if
/// `font` has no `WinAnsi` mapping (`Symbol`, `ZapfDingbats`).
#[must_use]
pub fn glyph_width(font: Font, size: f32, ch: char) -> f32 {
    glyph_width_units(font, ch) * size / 1000.0
}

/// Ascender height for `font` at `size` points (baseline to top of
/// tallest glyph). Sourced from the AFM `Ascender` field divided by 1000.
#[must_use]
pub fn ascent(font: Font, size: f32) -> f32 {
    font.0.metrics().ascender * size / 1000.0
}

/// Descender depth for `font` at `size` points, returned as a **positive**
/// number. AFM stores descender negative; we negate at the boundary so the
/// layout engine's existing positive-descent contract holds.
#[must_use]
pub fn descent(font: Font, size: f32) -> f32 {
    -font.0.metrics().descender * size / 1000.0
}

/// Width of a single character in 1/1000 em as the AFM `f32` type.
///
/// # Panics
///
/// Panics if `ch` is outside printable ASCII (`0x20..=0x7E`), or if
/// `font` has no `WinAnsi` mapping for the resulting byte (`Symbol`,
/// `ZapfDingbats`).
fn glyph_width_units(font: Font, ch: char) -> f32 {
    let cp = u32::from(ch);
    assert!(
        (0x20..=0x7E).contains(&cp),
        "non-ASCII char {ch:?} reached metrics; \
         the layout engine must substitute non-ASCII before measuring"
    );
    // Post-assert: `cp` is in `0x20..=0x7E`, which fits in `u8`. Using
    // `try_from` + `unwrap_or(0)` avoids `cast_possible_truncation` (vs
    // `as u8`) and `unwrap_used` (vs `.unwrap()`); the `0` fallback is
    // statically unreachable, and if the upstream assert is ever
    // weakened so chars > `u8::MAX` slip through, byte `0x00` is a
    // WinAnsi control char with no mapping — the second `assert!`
    // below surfaces the regression naming the bogus byte. Clippy's
    // `unreachable = "warn"` rules out `unreachable!()` here.
    let byte = u8::try_from(cp).unwrap_or(0);
    let width = font.0.winansi_width(byte);
    assert!(
        width.is_some(),
        "font {:?} has no WinAnsi mapping for byte {byte:#04x} ({ch:?}); \
         Symbol and ZapfDingbats are unsupported by mosaic-fonts MVP-2",
        font.0
    );
    width.unwrap_or(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    const HELV: Font = Font(Base14Font::Helvetica);
    const HELV_BOLD: Font = Font(Base14Font::HelveticaBold);
    const HELV_OBLIQUE: Font = Font(Base14Font::HelveticaOblique);
    const COURIER: Font = Font(Base14Font::Courier);

    #[test]
    fn helvetica_space_width_is_278_thou_em() {
        let w = text_width(HELV, 1000.0, " ");
        assert!((w - 278.0).abs() < 1e-6);
    }

    #[test]
    fn helvetica_apostrophe_matches_afm() {
        // PDF WinAnsi byte 0x27 decodes to glyph `quotesingle` — Adobe
        // Helvetica.afm reports `WX 191` for that glyph. The AFM also
        // contains `C 39 ; WX 222 ; N quoteright`, but that `C 39` is
        // AdobeStandardEncoding, *not* WinAnsi; PDF readers consume the
        // WinAnsi mapping for unembedded Base14 fonts so 191 is the
        // value that ends up on the page.
        let w = text_width(HELV, 1000.0, "'");
        assert!((w - 191.0).abs() < 1e-6, "got {w}");
    }

    #[test]
    fn courier_is_monospace() {
        let a = text_width(COURIER, 12.0, "a");
        let m = text_width(COURIER, 12.0, "M");
        assert_eq!(a, m);
    }

    #[test]
    fn bold_is_wider_than_regular_for_caps() {
        // `B` is 667 in regular Helvetica, 722 in Helvetica-Bold.
        let r = text_width(HELV, 100.0, "B");
        let b = text_width(HELV_BOLD, 100.0, "B");
        assert!(b > r);
    }

    #[test]
    fn helvetica_capital_a_matches_adobe_core14_afm() {
        // Adobe Helvetica.afm: `C 65 ; WX 667 ; N A`. URW Nimbus Sans
        // (the Linux Helvetica *substitute*) reports 556 — a different
        // font. PDF readers consume Adobe's Type 1 Helvetica metrics for
        // the standard `/Helvetica` resource, so we pin to Adobe's.
        let w = text_width(HELV, 1000.0, "A");
        assert!((w - 667.0).abs() < 1e-3, "got {w}");
        let wo = text_width(HELV_OBLIQUE, 1000.0, "A");
        assert!((wo - 667.0).abs() < 1e-3, "got {wo}");
        // Helvetica-Bold.afm: `C 65 ; WX 722 ; N A`.
        let wb = text_width(HELV_BOLD, 1000.0, "A");
        assert!((wb - 722.0).abs() < 1e-3, "got {wb}");
    }

    #[test]
    #[should_panic(expected = "non-ASCII char")]
    fn non_ascii_panics() {
        let _ = text_width(HELV, 12.0, "µ");
    }

    #[test]
    #[should_panic(expected = "no WinAnsi mapping")]
    fn symbol_has_no_winansi_mapping() {
        // Symbol's glyph names don't overlap WinAnsi's; `winansi_width`
        // returns `None` for every code, so the second assert in
        // `glyph_width_units` fires loudly instead of silently producing
        // a zero or garbage width.
        let _ = text_width(Font(Base14Font::Symbol), 12.0, "A");
    }

    #[test]
    #[should_panic(expected = "no WinAnsi mapping")]
    fn zapfdingbats_has_no_winansi_mapping() {
        let _ = text_width(Font(Base14Font::ZapfDingbats), 12.0, "A");
    }

    #[test]
    fn pdf_resource_name_is_f1_through_f14() {
        for (i, font) in Font::ALL.iter().enumerate() {
            let expected = format!("F{}", i + 1);
            assert_eq!(font.pdf_resource_name(), expected.as_bytes());
        }
    }

    #[test]
    fn font_all_preserves_historical_pdf_resource_numbers() {
        // The four pre-existing layout faces must keep F1..=F4. Drift
        // here would silently break PDF byte output for integration
        // tests like `build_renders_section_numbers_and_resolves_references`.
        assert_eq!(Font::ALL[0].0, Base14Font::Helvetica);
        assert_eq!(Font::ALL[1].0, Base14Font::HelveticaBold);
        assert_eq!(Font::ALL[2].0, Base14Font::HelveticaOblique);
        assert_eq!(Font::ALL[3].0, Base14Font::Courier);
    }
}
