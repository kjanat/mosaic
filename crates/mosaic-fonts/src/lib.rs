//! Font discovery, shaping, and metrics (manifest §22.1).
//!
//! MVP-2 facade over [`pdf_base14_metrics`]: a [`Font`] newtype around
//! [`Base14Font`] plus the width/ascent/descent helpers `mosaic-layout`
//! needs. The newtype gives Mosaic a single anchor point to refactor when
//! issue #9 widens `Font` to `Base14(Base14Font) | Embedded(EmbeddedFont)`;
//! that widening will still be a breaking change for call sites that touch
//! `.0`, the newtype just keeps those call sites localised.
//!
//! Any character measurable through this crate falls into one of two
//! tiers, both of which the PDF backend can render through the Core 14
//! base fonts without embedding font data:
//!
//! - `WinAnsi` natives: ASCII (`0x20..=0x7E`), the Windows-specific
//!   band (`0x80..=0x9F` — Euro, smart quotes, bullet, trademark, …),
//!   and Latin-1 (`0xA0..=0xFF` — `é`, `ß`, `§`, accented Latin, …),
//!   plus `š`/`ž`/`Š`/`Ž` (Czech and friends at 0x8A/0x9A/0x8E/0x9E).
//!   Look up with [`winansi_byte`].
//! - Extended glyphs that exist in the Adobe Core 14 Latin AFMs but
//!   have no `WinAnsi` byte: the rest of Latin Extended-A (`Ł`, `ł`,
//!   `Ě`, `ě`, `Ő`, `ő`, …), the comma-below Romanian set
//!   (`Ș`/`ș`/`Ț`/`ț`), the spacing diacritics (`˘ˇ˙˝˛˚`), the math
//!   operators (`−≤≥≠√∂∑∆◊`), the `fraction` slash `⁄`, and the
//!   `fi`/`fl` ligatures. Look up with [`glyph_name`]. The PDF
//!   backend addresses these through a per-document `/Differences`
//!   encoding (the 256-slot ceiling caps about 100 extra glyphs per
//!   face per document — well above any realistic European doc).
//!
//! Anything past those two tiers — Cyrillic, CJK, Vietnamese accents,
//! emoji — has no glyph in any Core 14 font and requires font
//! embedding (issue #9). The layout engine substitutes those to `?`
//! with a `W040` warning.

#![deny(missing_docs)]

pub use pdf_base14_metrics::{Base14Font, glyph_name, winansi_byte};

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
/// Panics if any character in `text` is unmappable — no `WinAnsi`
/// slot and no glyph name in the Core 14 Latin AFMs (Cyrillic, CJK,
/// emoji, …). Callers must substitute unmappable characters upstream.
/// Also panics if `font` is `Symbol` or `ZapfDingbats`, since those
/// use their own encodings.
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
/// Panics if `ch` is unmappable (see [`text_width`]), or if `font` is
/// `Symbol`/`ZapfDingbats`.
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
/// Two lookup tiers: `WinAnsi` natives go through the baked
/// `[Option<f32>; 256]` table (O(1)); extended glyphs reachable via
/// `/Differences` (Latin Extended-A, math operators, ligatures —
/// resolved through [`glyph_name`]) go through the baked sorted
/// `(name, width)` index (O(log n)). The PDF emit path mirrors this
/// split: `WinAnsi` natives go out as their native byte, extended
/// glyphs go out as remapped bytes in a `/Differences` array.
///
/// # Panics
///
/// Panics if `ch` is neither a `WinAnsi` native nor resolvable
/// through [`glyph_name`] (Cyrillic, CJK, emoji, …) — the layout
/// engine substitutes those to `?` at the `sanitize_text` boundary
/// before any measurement reaches here. Also panics if `font` is
/// `Symbol`/`ZapfDingbats` (no `WinAnsi` table for those).
fn glyph_width_units(font: Font, ch: char) -> f32 {
    // Two-stage assert-then-`unwrap_or` dance: workspace
    // `clippy::panic = "warn"` rules out the `let Some(..) else
    // { panic!(..) }` idiom (the bare `panic!` macro trips the lint),
    // and `clippy::unwrap_used = "warn"` rules out `.unwrap()`. So we
    // assert the Option is Some (clippy is fine with `assert!`),
    // then `unwrap_or` with a statically-unreachable fallback.
    if let Some(byte) = winansi_byte(ch) {
        let width = font.0.winansi_width(byte);
        assert!(
            width.is_some(),
            "font {:?} has no WinAnsi mapping for byte {byte:#04x} ({ch:?}); \
             Symbol and ZapfDingbats are unsupported by mosaic-fonts MVP-2",
            font.0
        );
        return width.unwrap_or(0.0);
    }
    let name_opt = glyph_name(ch);
    assert!(
        name_opt.is_some(),
        "char {ch:?} (U+{:04X}) has no glyph in the Core 14 AFMs; \
         the layout engine must substitute it before measuring",
        u32::from(ch)
    );
    let name = name_opt.unwrap_or("");
    let width = font.0.glyph_width_by_name(name);
    assert!(
        width.is_some(),
        "font {:?} has no glyph named {name:?} for char {ch:?}; \
         Symbol and ZapfDingbats don't carry extended Latin glyphs",
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
    fn helvetica_eacute_matches_adobe_core14_afm() {
        // Helvetica.afm: `C -1 ; WX 556 ; N eacute` (and the `Eacute`
        // glyph at 667). WinAnsi byte 0xE9 -> "eacute", 0xC9 -> "Eacute".
        let lower = text_width(HELV, 1000.0, "é");
        assert!((lower - 556.0).abs() < 1e-3, "got {lower}");
        let upper = text_width(HELV, 1000.0, "É");
        assert!((upper - 667.0).abs() < 1e-3, "got {upper}");
    }

    #[test]
    fn helvetica_winansi_band_widths() {
        // ß (germandbls, byte 0xDF) is 611 in Helvetica.afm.
        let germandbls = text_width(HELV, 1000.0, "ß");
        assert!((germandbls - 611.0).abs() < 1e-3, "got {germandbls}");
        // § (section, byte 0xA7) is 556.
        let section = text_width(HELV, 1000.0, "§");
        assert!((section - 556.0).abs() < 1e-3, "got {section}");
        // © (copyright, byte 0xA9) is 737.
        let copy = text_width(HELV, 1000.0, "©");
        assert!((copy - 737.0).abs() < 1e-3, "got {copy}");
        // Euro (Windows-specific band, byte 0x80) is 556.
        let euro = text_width(HELV, 1000.0, "€");
        assert!((euro - 556.0).abs() < 1e-3, "got {euro}");
    }

    #[test]
    #[should_panic(expected = "has no glyph in the Core 14 AFMs")]
    fn cyrillic_panics() {
        // "П" (Cyrillic Pe, U+041F) has no glyph in any Core 14 font;
        // the layout engine substitutes via sanitize_text before
        // measurements ever reach the fonts crate.
        let _ = text_width(HELV, 12.0, "П");
    }

    #[test]
    fn helvetica_lslash_resolves_through_glyph_name_lookup() {
        // Polish "ł" (U+0142) has no WinAnsi byte but exists in
        // Helvetica.afm as `lslash` (WX 222). The PDF backend will
        // address it through /Differences; the fonts crate measures
        // it through the baked name-keyed index.
        let w = text_width(HELV, 1000.0, "ł");
        assert!((w - 222.0).abs() < 1e-3, "got {w}");
        // "Łódź" is Polish: Ł(556) ó(556) d(556) ź(500)
        let lodz = text_width(HELV, 1000.0, "Łódź");
        assert!(
            (lodz - (556.0 + 556.0 + 556.0 + 500.0)).abs() < 1e-3,
            "got {lodz}"
        );
    }

    #[test]
    fn helvetica_czech_ecaron_resolves() {
        // Czech "ě" (U+011B) → glyph `ecaron`, WX 556.
        let w = text_width(HELV, 1000.0, "ě");
        assert!((w - 556.0).abs() < 1e-3, "got {w}");
    }

    #[test]
    fn courier_extended_glyphs_are_monospace() {
        // Courier's extended glyphs share the 600-unit advance.
        let lslash = text_width(COURIER, 1000.0, "ł");
        let ecaron = text_width(COURIER, 1000.0, "ě");
        assert!((lslash - 600.0).abs() < 1e-3, "got {lslash}");
        assert!((ecaron - 600.0).abs() < 1e-3, "got {ecaron}");
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
