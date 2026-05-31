use mos_core::{Diagnostic, SourceSpan, codes};

use crate::{Base14Font, EmbeddedFontId, Font};

/// A four-cut family — Regular, Bold, Italic, `BoldItalic`. The layout
/// engine picks one slot per styled run (`*emphasis*` → italic,
/// `**strong**` → bold, raw → fixed-width family, body → regular).
///
/// Build via [`FontFamily::resolve`], which understands Base14 family
/// names and the bundled `"Noto Sans"` family. Unknown names fall back
/// to Noto Sans and emit a `MOS0034` diagnostic.
///
/// # Examples
///
/// ```
/// use mos_fonts::{Font, FontFamily, EmbeddedFontId};
///
/// let family = FontFamily::noto_sans();
///
/// assert_eq!(family.regular, Font::Embedded(EmbeddedFontId::Regular));
/// ```
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
    /// Per-glyph fallback chain shared by every style slot in this
    /// family. When shaping against any of the style-slot faces above
    /// yields `.notdef` for some cluster, [`crate::shape_with_fallback`]
    /// retries that cluster against each embedded face in this slice in
    /// order. The first face to cover the cluster wins the whole cluster
    /// (cluster-granular replacement). Math fallback is therefore
    /// upright even inside bold or italic text until style-aware fallback
    /// chains exist. Empty chain = primary-only shaping (Base14 families
    /// don't have an embedded fallback target).
    pub fallbacks: &'static [EmbeddedFontId],
}

/// Per-glyph fallback chain for [`FontFamily::noto_sans`]. Math
/// codepoints (`≤ ≥ √ ∂ ∑ ∆ ◊` …) outside Noto Sans's coverage
/// route through Noto Sans Math via the cluster-granular retry in
/// [`crate::shape_with_fallback`].
const NOTO_SANS_FALLBACKS: &[EmbeddedFontId] = &[EmbeddedFontId::Math];

impl FontFamily {
    /// The bundled Noto Sans family — embedded TTFs, real designed
    /// cuts for every style slot (no faux-bold or faux-italic). Raw
    /// runs route through the bundled Noto Sans Mono Regular cut so
    /// `` `Привет` `` and other non-WinAnsi raw content shape through
    /// the same `rustybuzz` + `/ToUnicode` pipeline as body text
    /// instead of dropping to the Base14 `?` substitution.
    ///
    /// Per-glyph fallback chain: `[Math]`. Codepoints not in Noto
    /// Sans (math operators like `≤ ≥ √ ∂ ∑ ∆ ◊`) shape against
    /// the bundled Noto Sans Math cut.
    ///
    /// # Examples
    ///
    /// ```
    /// use mos_fonts::{EmbeddedFontId, Font, FontFamily};
    ///
    /// let family = FontFamily::noto_sans();
    ///
    /// assert_eq!(family.monospace, Font::Embedded(EmbeddedFontId::Mono));
    /// ```
    #[must_use]
    pub const fn noto_sans() -> Self {
        Self {
            regular: Font::Embedded(EmbeddedFontId::Regular),
            bold: Font::Embedded(EmbeddedFontId::Bold),
            italic: Font::Embedded(EmbeddedFontId::Italic),
            bold_italic: Font::Embedded(EmbeddedFontId::BoldItalic),
            monospace: Font::Embedded(EmbeddedFontId::Mono),
            fallbacks: NOTO_SANS_FALLBACKS,
        }
    }

    /// The Base14 Helvetica family. Used when the document explicitly
    /// asks for `Helvetica`. Falls back through Courier for raw.
    ///
    /// Base14 has no per-glyph fallback target — the byte-encoded
    /// content stream path can't splice in glyph IDs from a sibling
    /// face. Non-WinAnsi codepoints silently substitute to `?` in
    /// `mos-pdf::encode_base14_run`.
    ///
    /// # Examples
    ///
    /// ```
    /// use mos_fonts::{Base14Font, Font, FontFamily};
    ///
    /// let family = FontFamily::helvetica();
    ///
    /// assert_eq!(family.regular, Font::Base14(Base14Font::Helvetica));
    /// ```
    #[must_use]
    pub const fn helvetica() -> Self {
        Self {
            regular: Font::Base14(Base14Font::Helvetica),
            bold: Font::Base14(Base14Font::HelveticaBold),
            italic: Font::Base14(Base14Font::HelveticaOblique),
            bold_italic: Font::Base14(Base14Font::HelveticaBoldOblique),
            monospace: Font::Base14(Base14Font::Courier),
            fallbacks: &[],
        }
    }

    /// The Base14 Times Roman family. Used when the document asks
    /// for `Times` or `Times-Roman`. No per-glyph fallback — see
    /// [`Self::helvetica`].
    ///
    /// # Examples
    ///
    /// ```
    /// use mos_fonts::{Base14Font, Font, FontFamily};
    ///
    /// let family = FontFamily::times();
    ///
    /// assert_eq!(family.bold, Font::Base14(Base14Font::TimesBold));
    /// ```
    #[must_use]
    pub const fn times() -> Self {
        Self {
            regular: Font::Base14(Base14Font::TimesRoman),
            bold: Font::Base14(Base14Font::TimesBold),
            italic: Font::Base14(Base14Font::TimesItalic),
            bold_italic: Font::Base14(Base14Font::TimesBoldItalic),
            monospace: Font::Base14(Base14Font::Courier),
            fallbacks: &[],
        }
    }

    /// The Base14 Courier family. Used when the document asks for
    /// `Courier` as the body face. All four style slots route to a
    /// Courier cut. No per-glyph fallback — see [`Self::helvetica`].
    ///
    /// # Examples
    ///
    /// ```
    /// use mos_fonts::{Base14Font, Font, FontFamily};
    ///
    /// let family = FontFamily::courier();
    ///
    /// assert_eq!(family.italic, Font::Base14(Base14Font::CourierOblique));
    /// ```
    #[must_use]
    pub const fn courier() -> Self {
        Self {
            regular: Font::Base14(Base14Font::Courier),
            bold: Font::Base14(Base14Font::CourierBold),
            italic: Font::Base14(Base14Font::CourierOblique),
            bold_italic: Font::Base14(Base14Font::CourierBoldOblique),
            monospace: Font::Base14(Base14Font::Courier),
            fallbacks: &[],
        }
    }

    /// Resolve a `#set text(font: ...)` name to a family.
    ///
    /// Matching is case-insensitive on the family component. Known
    /// names: `Helvetica`, `Times`/`Times-Roman`/`Times Roman`,
    /// `Courier`, `Noto Sans`. Anything else falls back to Noto Sans
    /// and pushes a `MOS0034` notice so users don't silently get the
    /// wrong typeface.
    ///
    /// # Examples
    ///
    /// ```
    /// use mos_fonts::{Base14Font, Font, FontFamily};
    ///
    /// let mut diagnostics = Vec::new();
    /// let family = FontFamily::resolve("Times", None, &mut diagnostics);
    ///
    /// assert_eq!(family.regular, Font::Base14(Base14Font::TimesRoman));
    /// assert!(diagnostics.is_empty());
    /// ```
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
                diagnostics.push(Diagnostic::simple(
                    &codes::MOS0034,
                    span,
                    format!(
                        "unknown font family `{name}`; falling back to bundled Noto Sans \
                         (known families: Helvetica, Times, Courier, Noto Sans)"
                    ),
                ));
                Self::noto_sans()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use mos_core::Severity;

    use super::*;

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
        assert_eq!(diags[0].def().code(), codes::MOS0034.code());
        assert_eq!(diags[0].severity(), Severity::Notice);
        assert_eq!(fam.regular, Font::Embedded(EmbeddedFontId::Regular));
    }
}
