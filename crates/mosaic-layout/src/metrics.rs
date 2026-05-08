//! Glyph metrics for the four standard PDF fonts MVP 0 uses.
//!
//! Widths and ascent/descent are sourced from the **Adobe Core 14
//! AFMs** (canonical mirror: `tecnickcom/tc-font-core14-afms`).
//! For Helvetica that's `Ascender 718`, `Descender -207`, and
//! `C 65 ; WX 667 ; N A` — and the values are identical for
//! Helvetica-Oblique. Helvetica-Bold shares the same ascent /
//! descent and only widths differ. Courier is monospace
//! (every glyph 600 units, ascent 629, descent -157).
//!
//! Important: do **not** "correct" these to URW Nimbus Sans values
//! (729/-218 ascent/descent, 556 for capital A). URW Nimbus Sans is
//! the Liberation-family Helvetica *substitute* that many Linux
//! distributions install as `Helvetica.afm` — it's metrically
//! similar but a different font, and PDF readers consume Adobe's
//! Type 1 Helvetica metrics for the standard `/Helvetica`
//! resource per the PDF spec.
//!
//! Only printable ASCII (`0x20..=0x7E`) is supported. The layout
//! engine substitutes anything outside that range with `?` at a
//! single boundary (and emits a `W040` diagnostic), so by the time
//! widths are queried every character is in the table — no
//! fallback heuristic, no chance of measurement and rendering
//! disagreeing.

/// Font face used by the layout engine; maps 1:1 onto the four
/// PDF base fonts MVP 0 supports.
#[derive(Copy, Clone, Eq, PartialEq, Debug, Hash)]
pub enum Font {
    Helvetica,
    HelveticaBold,
    HelveticaOblique,
    Courier,
}

impl Font {
    /// PDF `/BaseFont` name for the standard 14 mapping.
    #[must_use]
    pub fn pdf_base_name(self) -> &'static str {
        match self {
            Self::Helvetica => "Helvetica",
            Self::HelveticaBold => "Helvetica-Bold",
            Self::HelveticaOblique => "Helvetica-Oblique",
            Self::Courier => "Courier",
        }
    }

    /// Stable resource name written into the page's font dictionary.
    /// PDF allows any name; using `F1`–`F4` keeps the streams short
    /// and identical across builds.
    #[must_use]
    pub fn pdf_resource_name(self) -> &'static [u8] {
        match self {
            Self::Helvetica => b"F1",
            Self::HelveticaBold => b"F2",
            Self::HelveticaOblique => b"F3",
            Self::Courier => b"F4",
        }
    }
}

/// All four faces in a stable order so PDF emission can iterate
/// without depending on hash ordering.
pub const ALL_FONTS: &[Font] = &[
    Font::Helvetica,
    Font::HelveticaBold,
    Font::HelveticaOblique,
    Font::Courier,
];

/// Helvetica advance widths for chars `0x20..=0x7E`, in 1/1000 em.
const HELVETICA_WIDTHS: [u16; 95] = [
    278, 278, 355, 556, 556, 889, 667, 222, 333, 333, 389, 584, 278, 333, 278, 278, // 32..=47
    556, 556, 556, 556, 556, 556, 556, 556, 556, 556, 278, 278, 584, 584, 584, 556, // 48..=63
    1015, 667, 667, 722, 722, 667, 611, 778, 722, 278, 500, 667, 556, 833, 722, 778, // 64..=79
    667, 778, 722, 667, 611, 722, 667, 944, 667, 667, 611, 278, 278, 278, 469, 556, // 80..=95
    222, 556, 556, 500, 556, 556, 278, 556, 556, 222, 222, 500, 222, 833, 556, 556, // 96..=111
    556, 556, 333, 500, 278, 556, 500, 722, 500, 500, 500, 334, 260, 334, 584, // 112..=126
];

/// Helvetica-Bold advance widths for chars `0x20..=0x7E`, in 1/1000 em.
const HELVETICA_BOLD_WIDTHS: [u16; 95] = [
    278, 333, 474, 556, 556, 889, 722, 278, 333, 333, 389, 584, 278, 333, 278, 278, // 32..=47
    556, 556, 556, 556, 556, 556, 556, 556, 556, 556, 333, 333, 584, 584, 584, 611, // 48..=63
    975, 722, 722, 722, 722, 667, 611, 778, 722, 278, 556, 722, 611, 833, 722, 778, // 64..=79
    667, 778, 722, 667, 611, 722, 667, 944, 667, 667, 611, 333, 278, 333, 584, 556, // 80..=95
    278, 556, 611, 556, 611, 556, 333, 611, 611, 278, 278, 556, 278, 889, 611, 611, // 96..=111
    611, 611, 389, 556, 333, 611, 556, 778, 556, 556, 500, 389, 280, 389, 584, // 112..=126
];

/// Width of a single character in 1/1000 em.
///
/// # Panics
///
/// Panics if `ch` is outside `0x20..=0x7E`. The layout engine is
/// responsible for substituting non-ASCII before calling here.
fn glyph_width_units(font: Font, ch: char) -> u16 {
    if matches!(font, Font::Courier) {
        return 600;
    }
    let code = u32::from(ch);
    assert!(
        (0x20..=0x7E).contains(&code),
        "glyph_width_units: non-ASCII char {ch:?} reached metrics; \
         the layout engine must substitute non-ASCII before measuring"
    );
    let table = match font {
        Font::HelveticaBold => &HELVETICA_BOLD_WIDTHS,
        _ => &HELVETICA_WIDTHS,
    };
    let idx = (code - 0x20) as usize;
    table[idx]
}

/// Advance width of `text` rendered in `font` at `size` points.
///
/// Uses `f32` to match the PDF backend's coordinate type — PDF page
/// dimensions never come close to f32's representable range so there's
/// no precision win from f64 here.
///
/// # Panics
///
/// See [`glyph_width_units`]: every character must be printable ASCII.
#[must_use]
pub fn text_width(font: Font, size: f32, text: &str) -> f32 {
    // Accumulate in f32: `f32::from(u16)` is exact, and the running
    // sum is bounded by line-length × max-glyph-width which stays
    // well inside f32's 24-bit mantissa for any realistic input.
    let mut units: f32 = 0.0;
    for ch in text.chars() {
        units += f32::from(glyph_width_units(font, ch));
    }
    units * size / 1000.0
}

/// Width of a single glyph rendered in `font` at `size` points.
///
/// # Panics
///
/// See [`glyph_width_units`].
#[must_use]
pub fn glyph_width(font: Font, size: f32, ch: char) -> f32 {
    f32::from(glyph_width_units(font, ch)) * size / 1000.0
}

/// Ascender height for `font` at `size` points (distance from
/// baseline to top of tallest glyph). Pulled from AFM's
/// `Ascender` field divided by 1000.
#[must_use]
pub fn ascent(font: Font, size: f32) -> f32 {
    let units: f32 = match font {
        Font::Helvetica | Font::HelveticaBold | Font::HelveticaOblique => 718.0,
        Font::Courier => 629.0,
    };
    units * size / 1000.0
}

/// Descender depth for `font` at `size` points (positive value).
#[must_use]
pub fn descent(font: Font, size: f32) -> f32 {
    let units: f32 = match font {
        Font::Helvetica | Font::HelveticaBold | Font::HelveticaOblique => 207.0,
        Font::Courier => 157.0,
    };
    units * size / 1000.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn helvetica_space_width_is_278_thou_em() {
        let w = text_width(Font::Helvetica, 1000.0, " ");
        assert!((w - 278.0).abs() < 1e-6);
    }

    #[test]
    fn helvetica_apostrophe_matches_afm() {
        // U+0027 in Adobe's Helvetica AFM is 222 (the typographic
        // U+2019 `quoteright` is 191 — not what we want here).
        let w = text_width(Font::Helvetica, 1000.0, "'");
        assert!((w - 222.0).abs() < 1e-6, "got {w}");
    }

    #[test]
    fn courier_is_monospace() {
        // Courier returns a constant 600-unit advance for every
        // glyph, so both calls go through identical f32 arithmetic
        // and must produce byte-equal results.
        let a = text_width(Font::Courier, 12.0, "a");
        let m = text_width(Font::Courier, 12.0, "M");
        assert_eq!(a, m);
    }

    #[test]
    fn bold_is_wider_than_regular_for_caps() {
        // `B` is 667 in regular Helvetica, 722 in Helvetica-Bold.
        let r = text_width(Font::Helvetica, 100.0, "B");
        let b = text_width(Font::HelveticaBold, 100.0, "B");
        assert!(b > r);
    }

    #[test]
    fn helvetica_capital_a_matches_adobe_core14_afm() {
        // Adobe Helvetica.afm: `C 65 ; WX 667 ; N A ; B 14 0 654 718 ;`
        // URW Nimbus Sans (the Linux/htmldoc Helvetica substitute)
        // reports 556 here — that's a *different font* and is not
        // what PDF readers use for the standard `/Helvetica`
        // resource. This test pins us to Adobe's Type 1 metrics so
        // a future "drive-by AFM fix" can't silently change them.
        let w = text_width(Font::Helvetica, 1000.0, "A");
        assert!((w - 667.0).abs() < 1e-3, "got {w}");
        let wo = text_width(Font::HelveticaOblique, 1000.0, "A");
        assert!((wo - 667.0).abs() < 1e-3, "got {wo}");
        // Bold differs: `C 65 ; WX 722 ; N A` in Helvetica-Bold.afm.
        let wb = text_width(Font::HelveticaBold, 1000.0, "A");
        assert!((wb - 722.0).abs() < 1e-3, "got {wb}");
    }

    #[test]
    #[should_panic(expected = "non-ASCII char")]
    fn non_ascii_panics() {
        // Explicitly contracted: only the layout engine's substitution
        // pass should ever feed non-ASCII into the engine, so reaching
        // here is a programming error, not user-facing.
        let _ = text_width(Font::Helvetica, 12.0, "µ");
    }
}
