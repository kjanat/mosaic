use crate::{Base14Font, Font, extended_glyph_name, normalize::nfc_text, shape, winansi_byte};

/// Advance width of `text` rendered in `font` at `size` points.
///
/// Input is normalized through [`crate::nfc_text`] before any width
/// calculation. Decomposed sequences such as `S\u{0326}` therefore
/// measure as their precomposed NFC form (`Ș`) in both the Base14
/// per-character AFM path and the embedded-font shaping path.
/// [`glyph_width`] delegates here for a one-character string, so it
/// inherits the same normalization behavior.
///
/// For Base14 faces this sums per-character AFM widths (`WinAnsi`
/// natives + extended Latin reachable via [`extended_glyph_name`]).
/// Characters outside both tiers: Cyrillic, CJK, emoji: get the
/// width of `?` (the substitution glyph the PDF emit path also uses
/// for those characters in Base14 runs). No diagnostic; callers wanting
/// real coverage should pick an embedded family.
///
/// For embedded faces this shapes via `rustybuzz` for glyph selection
/// and sums the resulting PDF-emittable glyph advances. Positioning
/// offsets are currently normalized away so layout matches PDF output.
///
/// # Examples
///
/// ```
/// use mos_fonts::{Base14Font, Font, text_width};
///
/// let width = text_width(Font::Base14(Base14Font::Helvetica), 10.0, "A");
///
/// assert_eq!(width, 6.67);
/// ```
#[must_use]
pub fn text_width(font: Font, size: f32, text: &str) -> f32 {
    let text = nfc_text(text);
    let text = text.as_ref();
    match font {
        Font::Base14(f) => {
            let mut units: f32 = 0.0;
            for ch in text.chars() {
                units += base14_glyph_units(f, ch);
            }
            units * size / 1000.0
        }
        Font::Embedded(id) => {
            let ef = id.data();
            let glyphs = shape(ef, text);
            let upem = f32::from(ef.units_per_em);
            glyphs
                .iter()
                .map(|g| advance_units_to_pt(g.advance_units, size, upem))
                .sum()
        }
    }
}

/// Convert a font-unit advance to PDF user-space points.
///
/// Values are carried as `i32` because shapers use signed advances. Preserve
/// sign here so future positioned shaping cannot turn a negative adjustment
/// into a huge positive width.
///
/// # Examples
///
/// ```
/// use mos_fonts::advance_units_to_pt;
///
/// assert_eq!(advance_units_to_pt(500, 12.0, 1000.0), 6.0);
/// assert_eq!(advance_units_to_pt(-500, 12.0, 1000.0), -6.0);
/// ```
#[must_use]
pub fn advance_units_to_pt(advance_units: i32, size_pt: f32, upem: f32) -> f32 {
    let magnitude = u16::try_from(advance_units.unsigned_abs()).unwrap_or(u16::MAX);
    let advance = f32::from(magnitude);
    if advance_units.is_negative() {
        -advance * size_pt / upem
    } else {
        advance * size_pt / upem
    }
}

/// Width of a single glyph in `font` at `size` points.
///
/// Base14 faces use one AFM lookup; embedded faces shape the single character.
///
/// # Examples
///
/// ```
/// use mos_fonts::{Base14Font, Font, glyph_width};
///
/// assert_eq!(glyph_width(Font::Base14(Base14Font::Helvetica), 10.0, 'A'), 6.67);
/// ```
#[must_use]
pub fn glyph_width(font: Font, size: f32, ch: char) -> f32 {
    let mut buf = [0u8; 4];
    let s = ch.encode_utf8(&mut buf);
    text_width(font, size, s)
}

/// Ascender height for `font` at `size` points.
///
/// # Examples
///
/// ```
/// use mos_fonts::{Base14Font, Font, ascent};
///
/// assert!(ascent(Font::Base14(Base14Font::Helvetica), 10.0) > 0.0);
/// ```
#[must_use]
pub fn ascent(font: Font, size: f32) -> f32 {
    match font {
        Font::Base14(f) => f.metrics().ascender * size / 1000.0,
        Font::Embedded(id) => {
            let ef = id.data();
            f32::from(ef.ascender) * size / f32::from(ef.units_per_em)
        }
    }
}

/// Descender depth for `font` at `size` points, as a **positive**
/// number (the AFM/TTF storage convention is negative; both backends
/// normalise on the way out).
///
/// # Examples
///
/// ```
/// use mos_fonts::{Base14Font, Font, descent};
///
/// assert!(descent(Font::Base14(Base14Font::Helvetica), 10.0) > 0.0);
/// ```
#[must_use]
pub fn descent(font: Font, size: f32) -> f32 {
    match font {
        Font::Base14(f) => -f.metrics().descender * size / 1000.0,
        Font::Embedded(id) => {
            let ef = id.data();
            -f32::from(ef.descender) * size / f32::from(ef.units_per_em)
        }
    }
}

/// Width of a single character in a Base14 face, in 1/1000 em. `WinAnsi`
/// natives go through the baked O(1) table; extended glyphs (Latin
/// Extended-A, math operators, ligatures) go through the baked sorted
/// name index. Anything else (Cyrillic, CJK, emoji) silently returns
/// the width of `?`; the PDF emit path renders those characters as
/// `?` too, so widths and content stream stay in sync. Embedded
/// families exist precisely so callers wanting real coverage can opt
/// out of this `?`-everywhere behaviour.
fn base14_glyph_units(face: Base14Font, ch: char) -> f32 {
    if matches!(face, Base14Font::Symbol | Base14Font::ZapfDingbats) {
        // Symbol/Dingbats don't carry WinAnsi widths. The layout
        // engine doesn't route runs into them today; treat as 0
        // rather than panic.
        return 0.0;
    }
    if let Some(byte) = winansi_byte(ch) {
        return face.winansi_width(byte).unwrap_or(0.0);
    }
    if let Some(name) = extended_glyph_name(ch)
        && let Some(w) = face.glyph_width_by_name(name)
    {
        return w;
    }
    // Fallback: width of `?` (WinAnsi byte 0x3F). Always present in
    // every Latin Core 14 face.
    face.winansi_width(b'?').unwrap_or(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::EmbeddedFontId;

    const HELV: Font = Font::Base14(Base14Font::Helvetica);
    const HELV_BOLD: Font = Font::Base14(Base14Font::HelveticaBold);
    const HELV_OBLIQUE: Font = Font::Base14(Base14Font::HelveticaOblique);
    const COURIER: Font = Font::Base14(Base14Font::Courier);

    #[test]
    fn helvetica_space_width_is_278_thou_em() {
        let w = text_width(HELV, 1000.0, " ");
        assert!((w - 278.0).abs() < 1e-6);
    }

    #[test]
    fn helvetica_apostrophe_matches_afm() {
        let w = text_width(HELV, 1000.0, "'");
        assert!((w - 191.0).abs() < 1e-6, "got {w}");
    }

    #[test]
    fn courier_is_monospace() {
        let a = text_width(COURIER, 12.0, "a");
        let m = text_width(COURIER, 12.0, "M");
        assert!((a - m).abs() < f32::EPSILON);
    }

    #[test]
    fn bold_is_wider_than_regular_for_caps() {
        let r = text_width(HELV, 100.0, "B");
        let b = text_width(HELV_BOLD, 100.0, "B");
        assert!(b > r);
    }

    #[test]
    fn helvetica_capital_a_matches_adobe_core14_afm() {
        let w = text_width(HELV, 1000.0, "A");
        assert!((w - 667.0).abs() < 1e-3, "got {w}");
        let wo = text_width(HELV_OBLIQUE, 1000.0, "A");
        assert!((wo - 667.0).abs() < 1e-3, "got {wo}");
        let wb = text_width(HELV_BOLD, 1000.0, "A");
        assert!((wb - 722.0).abs() < 1e-3, "got {wb}");
    }

    #[test]
    fn helvetica_eacute_matches_adobe_core14_afm() {
        let lower = text_width(HELV, 1000.0, "é");
        assert!((lower - 556.0).abs() < 1e-3, "got {lower}");
        let upper = text_width(HELV, 1000.0, "É");
        assert!((upper - 667.0).abs() < 1e-3, "got {upper}");
    }

    #[test]
    fn base14_non_winansi_falls_back_to_question_mark_silently() {
        // Cyrillic П has no glyph in any Base14 face. The width path
        // returns the width of `?` (so width measurements stay
        // consistent with the rendered output) and emits no diagnostic.
        // PDF emission renders `?` for the same character.
        let q = text_width(HELV, 1000.0, "?");
        let cyrillic = text_width(HELV, 1000.0, "П");
        assert!((q - cyrillic).abs() < 1e-3, "q={q} cyr={cyrillic}");
    }

    #[test]
    fn helvetica_lslash_resolves_through_extended_glyph_name_lookup() {
        let w = text_width(HELV, 1000.0, "ł");
        assert!((w - 222.0).abs() < 1e-3, "got {w}");
        let lodz = text_width(HELV, 1000.0, "Łódź");
        assert!(
            (lodz - (556.0 + 556.0 + 556.0 + 500.0)).abs() < 1e-3,
            "got {lodz}"
        );
    }

    #[test]
    fn embedded_text_width_is_nonzero_for_cyrillic() {
        // The whole point: scripts the Base14 fonts can't render get
        // real widths through the embedded path.
        let font = Font::Embedded(EmbeddedFontId::Regular);
        let w = text_width(font, 12.0, "Привет");
        assert!(w > 0.0);
    }

    #[test]
    fn embedded_text_width_normalizes_decomposed_romanian() {
        let font = Font::Embedded(EmbeddedFontId::Regular);
        let decomposed = text_width(font, 12.0, "S\u{0326}");
        let precomposed = text_width(font, 12.0, "\u{0218}");

        assert!((decomposed - precomposed).abs() < f32::EPSILON);
    }

    #[test]
    fn advance_units_to_pt_preserves_negative_sign() {
        let actual = advance_units_to_pt(-1000, 12.0, 1000.0);
        assert!((actual + 12.0).abs() < f32::EPSILON, "got {actual}");
    }
}
