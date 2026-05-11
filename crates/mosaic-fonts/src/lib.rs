//! Font discovery, shaping, and metrics (manifest §22.1).
//!
//! Two font-emission paths live behind one [`Font`] enum:
//!
//! - [`Font::Base14`] — the 14 standard PDF base fonts. No glyph data
//!   ships; the PDF reader supplies outlines. Advance widths come from
//!   bundled Adobe AFMs, addressed through [`pdf_base14_metrics`].
//!   `WinAnsi` natives go out as their canonical byte; the small set
//!   of extended Latin glyphs each face carries (Latin Extended-A
//!   beyond `WinAnsi`, the math operators, `fi`/`fl` ligatures) goes
//!   out through a per-document `/Differences` remap that
//!   `mosaic-pdf` plans. Characters outside both tiers — Cyrillic,
//!   CJK, emoji — silently substitute to `?` in both the width and
//!   emit paths (no warning, no panic; callers that want non-Latin
//!   should pick the embedded family).
//! - [`Font::Embedded`] — a bundled Noto Sans cut shaped with
//!   `rustybuzz` (`HarfBuzz` Rust port). The PDF backend embeds a
//!   subset of the actual `TrueType` outlines as a Type 0 CID font
//!   with a `/ToUnicode` `CMap`, so the output is a real
//!   Unicode-aware document: copy/paste round-trips through Cyrillic,
//!   Greek, accented Latin, and anything else Noto Sans covers.
//!
//! Five cuts ship in this crate's `data/` directory: four Noto Sans
//! style cuts (Regular, Bold, Italic, `BoldItalic`) for proportional
//! body text plus one Noto Sans Mono Regular cut for `` `raw` `` runs
//! (see `SOURCES.md` under the crate root). Style selection happens
//! through [`FontFamily`], which the layout engine receives from the
//! eval lowerer.

#![deny(missing_docs)]

mod embedded;

use std::sync::LazyLock;

use mosaic_core::{Diagnostic, DiagnosticCode, Severity, SourceSpan};

pub use embedded::{EmbeddedFont, ShapedGlyph, shape, subset};
pub use pdf_base14_metrics::{Base14Font, extended_glyph_name, winansi_byte};

/// A renderable font — either one of the Adobe Core 14 (no data
/// embedded; outlines from the PDF reader) or a bundled TrueType
/// cut (data embedded, subset per-document).
#[derive(Copy, Clone, Eq, PartialEq, Debug, Hash)]
pub enum Font {
    /// A Base14 face. Layout uses AFM metrics; PDF emit uses
    /// `WinAnsiEncoding` + per-document `/Differences`.
    Base14(Base14Font),
    /// A bundled embedded face. Layout uses `rustybuzz` shaping;
    /// PDF emit produces a Type 0 CID font with `/ToUnicode`.
    Embedded(EmbeddedFontId),
}

/// Stable identifier for each bundled embedded cut. Used as the enum
/// payload of [`Font::Embedded`] so [`Font`] stays `Copy`/`Hash`/`Eq`
/// without resorting to pointer identity.
///
/// The crate ships two bundled families today: Noto Sans (four style
/// cuts — `Regular`/`Bold`/`Italic`/`BoldItalic`) for proportional
/// body text, and Noto Sans Mono (one cut — `Mono`) for `` `raw` ``
/// runs. The flat-enum shape is deliberate at this scale — when a
/// third family lands, or Mono grows additional cuts, the right move
/// is restructuring to `{ family, cut }` rather than expanding this
/// enum variant-by-variant further.
#[derive(Copy, Clone, Eq, PartialEq, Debug, Hash, Ord, PartialOrd)]
pub enum EmbeddedFontId {
    /// Noto Sans Regular.
    Regular,
    /// Noto Sans Bold.
    Bold,
    /// Noto Sans Italic.
    Italic,
    /// Noto Sans Bold Italic.
    BoldItalic,
    /// Noto Sans Mono Regular. The crate's monospace face for raw runs.
    Mono,
}

impl EmbeddedFontId {
    /// All bundled embedded cuts in a stable order. Used by the PDF
    /// backend to enumerate `/Font` resource entries deterministically.
    pub const ALL: [Self; 5] = [
        Self::Regular,
        Self::Bold,
        Self::Italic,
        Self::BoldItalic,
        Self::Mono,
    ];

    /// Resolve to the bundled [`EmbeddedFont`] data. Initialised on
    /// first use; subsequent calls return the same `&'static` reference.
    #[must_use]
    pub fn data(self) -> &'static EmbeddedFont {
        match self {
            Self::Regular => &NOTO_SANS_REGULAR,
            Self::Bold => &NOTO_SANS_BOLD,
            Self::Italic => &NOTO_SANS_ITALIC,
            Self::BoldItalic => &NOTO_SANS_BOLD_ITALIC,
            Self::Mono => &NOTO_SANS_MONO,
        }
    }

    /// PDF resource name: `F15`..`F19` (Base14 keeps `F1`..`F14`).
    /// The mapping is fixed per variant so byte-stable golden tests
    /// stay byte-stable.
    #[must_use]
    pub fn pdf_resource_name(self) -> &'static [u8] {
        match self {
            Self::Regular => b"F15",
            Self::Bold => b"F16",
            Self::Italic => b"F17",
            Self::BoldItalic => b"F18",
            Self::Mono => b"F19",
        }
    }
}

static NOTO_SANS_REGULAR: LazyLock<EmbeddedFont> = LazyLock::new(|| {
    EmbeddedFont::from_static(
        include_bytes!("../data/NotoSans-Regular.ttf"),
        "NotoSans",
        false,
        false,
    )
});

static NOTO_SANS_BOLD: LazyLock<EmbeddedFont> = LazyLock::new(|| {
    EmbeddedFont::from_static(
        include_bytes!("../data/NotoSans-Bold.ttf"),
        "NotoSans-Bold",
        true,
        false,
    )
});

static NOTO_SANS_ITALIC: LazyLock<EmbeddedFont> = LazyLock::new(|| {
    EmbeddedFont::from_static(
        include_bytes!("../data/NotoSans-Italic.ttf"),
        "NotoSans-Italic",
        false,
        true,
    )
});

static NOTO_SANS_BOLD_ITALIC: LazyLock<EmbeddedFont> = LazyLock::new(|| {
    EmbeddedFont::from_static(
        include_bytes!("../data/NotoSans-BoldItalic.ttf"),
        "NotoSans-BoldItalic",
        true,
        true,
    )
});

static NOTO_SANS_MONO: LazyLock<EmbeddedFont> = LazyLock::new(|| {
    EmbeddedFont::from_static(
        include_bytes!("../data/NotoSansMono-Regular.ttf"),
        "NotoSansMono",
        false,
        false,
    )
});

impl Font {
    /// All 14 Base14 faces in Mosaic's PDF-resource order (`F1`..`F14`).
    ///
    /// Page resource dictionaries always enumerate these — even when
    /// unused — so a Base14-only document's byte output stays stable
    /// across runs. Embedded faces are added on top, per-document.
    ///
    /// The ordering is **decoupled** from [`Base14Font::ALL`]: the
    /// four pre-existing layout faces keep their historical `F1`..`F4`
    /// resource numbers so existing integration goldens don't shift.
    pub const ALL_BASE14: [Self; 14] = [
        Self::Base14(Base14Font::Helvetica),
        Self::Base14(Base14Font::HelveticaBold),
        Self::Base14(Base14Font::HelveticaOblique),
        Self::Base14(Base14Font::Courier),
        Self::Base14(Base14Font::HelveticaBoldOblique),
        Self::Base14(Base14Font::TimesRoman),
        Self::Base14(Base14Font::TimesBold),
        Self::Base14(Base14Font::TimesItalic),
        Self::Base14(Base14Font::TimesBoldItalic),
        Self::Base14(Base14Font::CourierBold),
        Self::Base14(Base14Font::CourierOblique),
        Self::Base14(Base14Font::CourierBoldOblique),
        Self::Base14(Base14Font::Symbol),
        Self::Base14(Base14Font::ZapfDingbats),
    ];

    /// If this is a Base14 face, return the underlying variant.
    #[must_use]
    pub const fn base14(self) -> Option<Base14Font> {
        match self {
            Self::Base14(f) => Some(f),
            Self::Embedded(_) => None,
        }
    }

    /// If this is an embedded face, return its bundled id.
    #[must_use]
    pub const fn embedded(self) -> Option<EmbeddedFontId> {
        match self {
            Self::Embedded(id) => Some(id),
            Self::Base14(_) => None,
        }
    }

    /// PDF `/BaseFont` name for Base14 (e.g. `"Helvetica-BoldOblique"`)
    /// or the embedded face's PostScript name. The embedded path also
    /// gets a six-letter subset tag in the actual PDF emission, but that
    /// belongs to the per-document subset, not the bundled cut.
    #[must_use]
    pub fn pdf_base_name(self) -> &'static str {
        match self {
            Self::Base14(f) => f.pdf_base_name(),
            Self::Embedded(id) => id.data().postscript_name,
        }
    }

    /// Stable per-resource name (`F1`..`F14` for Base14, `F15`..`F19`
    /// for embedded). Page font dictionaries map these to indirect
    /// font refs.
    #[must_use]
    pub fn pdf_resource_name(self) -> &'static [u8] {
        match self {
            Self::Base14(f) => Self::Base14(f).base14_resource_name(),
            Self::Embedded(id) => id.pdf_resource_name(),
        }
    }

    /// Internal: Base14-only resource name table (`F1`..`F14`). Kept as
    /// a method on `Font` rather than on `Base14Font` so the lookup
    /// reads the same way the enum dispatch does. The `Embedded` arm
    /// is handled at the caller (`pdf_resource_name`) so this private
    /// helper never sees it.
    fn base14_resource_name(self) -> &'static [u8] {
        let Self::Base14(f) = self else {
            return b"F0";
        };
        match f {
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
        Self::Base14(f)
    }
}

impl From<EmbeddedFontId> for Font {
    fn from(id: EmbeddedFontId) -> Self {
        Self::Embedded(id)
    }
}

/// A four-cut family — Regular, Bold, Italic, `BoldItalic`. The layout
/// engine picks one slot per styled run (`*emphasis*` → italic,
/// `**strong**` → bold, raw → fixed-width family, body → regular).
///
/// Build via [`FontFamily::resolve`], which understands Base14 family
/// names and the bundled `"Noto Sans"` family. Unknown names fall back
/// to Noto Sans and emit a `W045` diagnostic.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct FontFamily {
    /// Default upright face. Used for body text.
    pub regular: Font,
    /// Bold face. Used for `**strong**` and headings.
    pub bold: Font,
    /// Italic / oblique face. Used for `*emphasis*`.
    pub italic: Font,
    /// Bold italic face. Used for `***bold italic***` constructs.
    pub bold_italic: Font,
    /// Monospace face. Used for `` `raw` `` runs. The four-slot family
    /// concept is upright/styled-Latin; raw is its own typeface choice
    /// that the layout engine pins independently of the family.
    pub monospace: Font,
}

impl FontFamily {
    /// The bundled Noto Sans family — embedded TTFs, real designed
    /// cuts for every style slot (no faux-bold or faux-italic). Raw
    /// runs route through the bundled Noto Sans Mono Regular cut so
    /// `` `Привет` `` and other non-WinAnsi raw content shape through
    /// the same `rustybuzz` + `/ToUnicode` pipeline as body text
    /// instead of dropping to the Base14 `?` substitution.
    #[must_use]
    pub const fn noto_sans() -> Self {
        Self {
            regular: Font::Embedded(EmbeddedFontId::Regular),
            bold: Font::Embedded(EmbeddedFontId::Bold),
            italic: Font::Embedded(EmbeddedFontId::Italic),
            bold_italic: Font::Embedded(EmbeddedFontId::BoldItalic),
            monospace: Font::Embedded(EmbeddedFontId::Mono),
        }
    }

    /// The Base14 Helvetica family. Used when the document explicitly
    /// asks for `Helvetica`. Falls back through Courier for raw.
    #[must_use]
    pub const fn helvetica() -> Self {
        Self {
            regular: Font::Base14(Base14Font::Helvetica),
            bold: Font::Base14(Base14Font::HelveticaBold),
            italic: Font::Base14(Base14Font::HelveticaOblique),
            bold_italic: Font::Base14(Base14Font::HelveticaBoldOblique),
            monospace: Font::Base14(Base14Font::Courier),
        }
    }

    /// The Base14 Times Roman family. Used when the document asks
    /// for `Times` or `Times-Roman`.
    #[must_use]
    pub const fn times() -> Self {
        Self {
            regular: Font::Base14(Base14Font::TimesRoman),
            bold: Font::Base14(Base14Font::TimesBold),
            italic: Font::Base14(Base14Font::TimesItalic),
            bold_italic: Font::Base14(Base14Font::TimesBoldItalic),
            monospace: Font::Base14(Base14Font::Courier),
        }
    }

    /// The Base14 Courier family. Used when the document asks for
    /// `Courier` as the body face. All four style slots route to a
    /// Courier cut.
    #[must_use]
    pub const fn courier() -> Self {
        Self {
            regular: Font::Base14(Base14Font::Courier),
            bold: Font::Base14(Base14Font::CourierBold),
            italic: Font::Base14(Base14Font::CourierOblique),
            bold_italic: Font::Base14(Base14Font::CourierBoldOblique),
            monospace: Font::Base14(Base14Font::Courier),
        }
    }

    /// Resolve a `#set text(font: ...)` name to a family.
    ///
    /// Matching is case-insensitive on the family component. Known
    /// names: `Helvetica`, `Times`/`Times-Roman`/`Times Roman`,
    /// `Courier`, `Noto Sans`. Anything else falls back to Noto Sans
    /// and pushes a `W045` warning so users don't silently get the
    /// wrong typeface.
    pub fn resolve(
        name: &str,
        span: Option<SourceSpan>,
        diagnostics: &mut Vec<Diagnostic>,
    ) -> Self {
        let normalised = name.trim().to_ascii_lowercase();
        match normalised.as_str() {
            "helvetica" => Self::helvetica(),
            "times" | "times-roman" | "times roman" | "times new roman" => Self::times(),
            "courier" => Self::courier(),
            "noto sans" | "notosans" => Self::noto_sans(),
            _ => {
                diagnostics.push(Diagnostic {
                    severity: Severity::Warning,
                    code: DiagnosticCode("W045"),
                    message: format!(
                        "unknown font family `{name}`; falling back to bundled Noto Sans \
                         (known families: Helvetica, Times, Courier, Noto Sans)"
                    ),
                    span,
                    notes: Vec::new(),
                    suggestions: Vec::new(),
                });
                Self::noto_sans()
            }
        }
    }
}

/// Advance width of `text` rendered in `font` at `size` points.
///
/// For Base14 faces this sums per-character AFM widths (`WinAnsi`
/// natives + extended Latin reachable via [`extended_glyph_name`]).
/// Characters outside both tiers — Cyrillic, CJK, emoji — get the
/// width of `?` (the substitution glyph the PDF emit path also uses
/// for those characters in Base14 runs). No diagnostic; callers wanting
/// real coverage should pick an embedded family.
///
/// For embedded faces this shapes via `rustybuzz` and sums the
/// resulting glyph advances. Mark positioning offsets do not contribute
/// to advance.
#[must_use]
pub fn text_width(font: Font, size: f32, text: &str) -> f32 {
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

/// Shape `text` against `font` and return both the glyph stream and
/// the advance widths in user-space points. Callers that need only the
/// width can use [`text_width`]; callers that will also emit glyphs
/// downstream should use this to avoid shaping twice.
///
/// For Base14 faces, `glyphs` is empty (Base14 runs go out as
/// `WinAnsi`-byte strings, not glyph IDs); only the width is computed.
#[must_use]
pub fn shape_text(font: Font, size: f32, text: &str) -> ShapedRun {
    match font {
        Font::Base14(_) => ShapedRun {
            glyphs: Vec::new(),
            advance_pt: text_width(font, size, text),
        },
        Font::Embedded(id) => {
            let ef = id.data();
            let glyphs = shape(ef, text);
            let upem = f32::from(ef.units_per_em);
            let advance_pt: f32 = glyphs
                .iter()
                .map(|g| advance_units_to_pt(g.advance_units, size, upem))
                .sum();
            ShapedRun { glyphs, advance_pt }
        }
    }
}

/// Output of [`shape_text`]: the shaped glyph stream and the total
/// advance width at the requested point size.
#[derive(Debug, Clone)]
pub struct ShapedRun {
    /// Glyphs in visual order (LTR). Empty for Base14 runs.
    pub glyphs: Vec<ShapedGlyph>,
    /// Total horizontal advance of the run, in PDF user-space units.
    pub advance_pt: f32,
}

/// Convert a font-unit advance to PDF user-space points at `size_pt`,
/// given the face's units-per-em. `rustybuzz` types advances as `i32`
/// but OpenType's `hmtx` table stores `advanceWidth` as `UFWord`
/// (unsigned 16-bit), so the underlying value is always in `0..=65535`.
/// Saturating through `u16` here lets us cross to `f32` losslessly
/// through `f32::From<u16>` and preserves the full `hmtx` range —
/// the prior `i16` saturation truncated wide glyphs in the
/// `32768..=65535` band to `i16::MAX`. The saturation clamps the
/// (practically unreachable) out-of-`u16` case to a finite advance
/// instead of a loose precision-lossy `i32 as f32` cast.
fn advance_units_to_pt(advance_units: i32, size_pt: f32, upem: f32) -> f32 {
    let advance_u16 = u16::try_from(advance_units).unwrap_or(u16::MAX);
    f32::from(advance_u16) * size_pt / upem
}

/// Width of a single glyph in `font` at `size` points. For Base14
/// faces this is one AFM lookup; for embedded faces it shapes the
/// single character. Used by the paragraph engine for character-wise
/// hyphenation of oversized words.
#[must_use]
pub fn glyph_width(font: Font, size: f32, ch: char) -> f32 {
    let mut buf = [0u8; 4];
    let s = ch.encode_utf8(&mut buf);
    text_width(font, size, s)
}

/// Ascender height for `font` at `size` points.
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
/// the width of `?` — the PDF emit path renders those characters as
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
        assert_eq!(a, m);
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
    fn pdf_resource_name_is_f1_through_f19() {
        for (i, font) in Font::ALL_BASE14.iter().enumerate() {
            let expected = format!("F{}", i + 1);
            assert_eq!(font.pdf_resource_name(), expected.as_bytes());
        }
        for (i, id) in EmbeddedFontId::ALL.iter().enumerate() {
            let expected = format!("F{}", 15 + i);
            assert_eq!(id.pdf_resource_name(), expected.as_bytes());
        }
    }

    #[test]
    fn font_all_base14_preserves_historical_resource_numbers() {
        assert_eq!(Font::ALL_BASE14[0], Font::Base14(Base14Font::Helvetica));
        assert_eq!(Font::ALL_BASE14[1], Font::Base14(Base14Font::HelveticaBold));
        assert_eq!(
            Font::ALL_BASE14[2],
            Font::Base14(Base14Font::HelveticaOblique)
        );
        assert_eq!(Font::ALL_BASE14[3], Font::Base14(Base14Font::Courier));
    }

    #[test]
    fn resolve_known_families_does_not_diagnose() {
        let mut diags = Vec::new();
        let fam = FontFamily::resolve("Helvetica", None, &mut diags);
        assert!(diags.is_empty());
        assert_eq!(fam.regular, Font::Base14(Base14Font::Helvetica));
        let _ = FontFamily::resolve("Times", None, &mut diags);
        let _ = FontFamily::resolve("Times-Roman", None, &mut diags);
        let _ = FontFamily::resolve("Courier", None, &mut diags);
        let _ = FontFamily::resolve("Noto Sans", None, &mut diags);
        assert!(diags.is_empty(), "unexpected diagnostics: {diags:?}");

        // Mixed case and leading/trailing whitespace must resolve the
        // same way the canonical spelling does — `resolve` normalises
        // through `.trim().to_ascii_lowercase()` before matching.
        let padded = FontFamily::resolve("  heLVETICA  ", None, &mut diags);
        assert!(
            diags.is_empty(),
            "padded mixed-case Helvetica diagnosed: {diags:?}"
        );
        assert_eq!(padded.regular, Font::Base14(Base14Font::Helvetica));
        let spaced = FontFamily::resolve("\tNoto Sans\n", None, &mut diags);
        assert!(diags.is_empty(), "padded Noto Sans diagnosed: {diags:?}");
        assert_eq!(spaced.regular, Font::Embedded(EmbeddedFontId::Regular));
    }

    #[test]
    fn resolve_unknown_family_emits_w045_and_falls_back_to_noto() {
        let mut diags = Vec::new();
        let fam = FontFamily::resolve("Libertinus Serif", None, &mut diags);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code.0, "W045");
        assert_eq!(diags[0].severity, Severity::Warning);
        assert_eq!(fam.regular, Font::Embedded(EmbeddedFontId::Regular));
    }

    #[test]
    fn embedded_shape_is_empty_for_empty_string() {
        let ef = EmbeddedFontId::Regular.data();
        let glyphs = shape(ef, "");
        assert!(glyphs.is_empty());
    }

    #[test]
    fn embedded_shape_returns_clusters_in_byte_order() {
        let ef = EmbeddedFontId::Regular.data();
        let glyphs = shape(ef, "Привет");
        assert!(!glyphs.is_empty());
        // Cluster values are byte offsets into the source string and
        // must be monotonically non-decreasing for LTR text.
        let mut prev: u32 = 0;
        for g in &glyphs {
            assert!(
                g.cluster >= prev,
                "cluster regression: {prev} -> {}",
                g.cluster
            );
            prev = g.cluster;
        }
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
    fn embedded_fi_ligature_collapses_glyphs() {
        // Noto Sans contains an `fi` ligature; rustybuzz returns one
        // glyph for `fi` (not two). The substituted gid differs from
        // both the standalone `f` and `i` gids. (Noto Sans's `fi`
        // ligature has the same advance as f+i — purely visual,
        // joining the dot of `i` with the terminal of `f` — so width
        // is not a useful invariant for this font.)
        let ef = EmbeddedFontId::Regular.data();
        let fi = shape(ef, "fi");
        let f = shape(ef, "f");
        let i = shape(ef, "i");
        assert_eq!(fi.len(), 1, "expected fi ligature, got glyphs {fi:?}");
        assert_ne!(fi[0].gid, f[0].gid);
        assert_ne!(fi[0].gid, i[0].gid);
    }
}
