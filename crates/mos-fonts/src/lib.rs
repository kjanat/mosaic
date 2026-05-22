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
//!   `mos-pdf` plans. Characters outside both tiers — Cyrillic,
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
//! Six cuts ship in this crate's `data/` directory: four Noto Sans
//! style cuts (Regular, Bold, Italic, `BoldItalic`) for proportional
//! body text, one Noto Sans Mono Regular cut for `` `raw` `` runs, and
//! one Noto Sans Math cut for per-glyph fallback (see `SOURCES.md`
//! under the crate root). Style selection happens through [`FontFamily`],
//! which the layout engine receives from the eval lowerer.

#![doc(
    html_logo_url = "https://mosaic.kjanat.dev/assets/A4.svg",
    html_favicon_url = "https://mosaic.kjanat.dev/assets/A4.svg"
)]
#![deny(missing_docs)]

mod embedded;
mod font;
mod resources;

use mos_core::{Diagnostic, DiagnosticCode, Severity, SourceSpan};

pub use embedded::{EmbeddedFont, ShapedGlyph, shape, subset};
pub use font::{EmbeddedFontId, Font};
pub use pdf_base14_metrics::{Base14Font, extended_glyph_name, winansi_byte};

/// A four-cut family — Regular, Bold, Italic, `BoldItalic`. The layout
/// engine picks one slot per styled run (`*emphasis*` → italic,
/// `**strong**` → bold, raw → fixed-width family, body → regular).
///
/// Build via [`FontFamily::resolve`], which understands Base14 family
/// names and the bundled `"Noto Sans"` family. Unknown names fall back
/// to Noto Sans and emit a `W045` diagnostic.
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
    /// yields `.notdef` for some cluster, [`shape_with_fallback`] retries
    /// that cluster against each embedded face in this slice in order.
    /// The first face to cover the cluster wins the whole cluster
    /// (cluster-granular replacement). Math fallback is therefore
    /// upright even inside bold or italic text until style-aware fallback
    /// chains exist. Empty chain = primary-only shaping (Base14 families
    /// don't have an embedded fallback target).
    pub fallbacks: &'static [EmbeddedFontId],
}

/// Per-glyph fallback chain for [`FontFamily::noto_sans`]. Math
/// codepoints (`≤ ≥ √ ∂ ∑ ∆ ◊` …) outside Noto Sans's coverage
/// route through Noto Sans Math via the cluster-granular retry in
/// [`shape_with_fallback`].
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
    /// and pushes a `W045` warning so users don't silently get the
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
///
/// # Examples
///
/// ```
/// use mos_fonts::{Base14Font, Font, shape_text};
///
/// let run = shape_text(Font::Base14(Base14Font::Helvetica), 10.0, "A");
///
/// assert!(run.glyphs.is_empty());
/// assert_eq!(run.advance_pt, 6.67);
/// ```
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
///
/// # Examples
///
/// ```
/// use mos_fonts::ShapedRun;
///
/// let run = ShapedRun {
///     glyphs: Vec::new(),
///     advance_pt: 12.0,
/// };
///
/// assert_eq!(run.advance_pt, 12.0);
/// ```
#[derive(Debug, Clone)]
pub struct ShapedRun {
    /// Glyphs in visual order (LTR). Empty for Base14 runs.
    pub glyphs: Vec<ShapedGlyph>,
    /// Total horizontal advance of the run, in PDF user-space units.
    pub advance_pt: f32,
}

/// One face's slice of a per-glyph-fallback shaping result. A word
/// shaped through [`shape_with_fallback`] produces a `Vec<WordSubRun>`;
/// each sub-run is self-contained: its `text` is the source UTF-8 slice
/// covered by exactly this sub-run, its `glyphs`' `cluster` offsets are
/// **rebased to the sub-run's local `text`** (so `plan_embedded` can
/// build `/ToUnicode` without knowing about the parent word), and its
/// `advance_pt` is the sum of per-glyph advances at the requested
/// point size.
///
/// Caller emits **one PDF `TextRun` per `WordSubRun`** — same
/// baseline, x-cursor advances by `advance_pt` between sub-runs — and
/// PDF emit's existing `Tf` switch fires naturally on the font change.
///
/// # Examples
///
/// ```
/// use mos_fonts::{Base14Font, Font, WordSubRun};
///
/// let subrun = WordSubRun {
///     font: Font::Base14(Base14Font::Helvetica),
///     text: "A".to_owned(),
///     glyphs: Vec::new(),
///     advance_pt: 6.67,
/// };
///
/// assert_eq!(subrun.text, "A");
/// ```
#[derive(Debug, Clone)]
pub struct WordSubRun {
    /// Which face owns the glyphs in this slice. May be the primary
    /// (no fallback was needed for this span) or a fallback face that
    /// covered codepoints the primary lacked.
    pub font: Font,
    /// Source byte slice covered by this sub-run. `glyphs`' `cluster`
    /// values are byte offsets into **this** field, not into the parent
    /// word's text.
    pub text: String,
    /// Shaped glyphs in visual order (LTR). Cluster offsets are local
    /// to `text` (rebased from the parent word's full text). Empty for
    /// Base14 sub-runs (Base14 has no glyph stream — PDF emit goes via
    /// `WinAnsi`-byte encoding instead).
    pub glyphs: Vec<ShapedGlyph>,
    /// Total horizontal advance of this sub-run, in PDF user-space
    /// units. Sum of PDF-emittable `glyphs[i].advance_units` scaled by
    /// `size_pt / units_per_em`.
    pub advance_pt: f32,
}

/// Shape `text` against `primary` with per-glyph fallback. Walks each
/// HarfBuzz cluster in the primary's shaped output; clusters that
/// contain any `.notdef` (GID 0) glyph are re-shaped against each
/// embedded face in `fallbacks` in order. The first fallback to produce a
/// glyph stream with no `.notdef` wins the whole cluster (cluster-
/// granular replacement, never partial — partial replacement would
/// duplicate bases, drop marks, break ligatures).
///
/// Returns one [`WordSubRun`] per contiguous source span that shares
/// a face. Each sub-run's `glyphs` `cluster` offsets are rebased to
/// the sub-run's local `text`, so `mos-pdf::plan_embedded` reads
/// `/ToUnicode` clusters with no awareness of the parent word.
///
/// Base14 `primary`: returns a single sub-run with empty `glyphs`
/// (Base14 has no glyph stream to inspect for `.notdef`; fallback
/// isn't meaningful for that path). The advance comes from the AFM
/// width sum via [`text_width`], same as the legacy `shape_text`
/// path.
///
/// All-fallback-fails behaviour: if no face in `fallbacks` covers a
/// `.notdef` cluster, the cluster stays in `primary` with `.notdef`
/// glyphs. `plan_embedded` already skips GID 0 from `gid_to_text`,
/// so copy-paste extraction is correct (empty for the un-renderable
/// span); the PDF reader renders an empty box.
///
/// # Examples
///
/// ```
/// use mos_fonts::{Base14Font, Font, shape_with_fallback};
///
/// let subruns = shape_with_fallback(Font::Base14(Base14Font::Helvetica), &[], 10.0, "A");
///
/// assert_eq!(subruns.len(), 1);
/// assert_eq!(subruns[0].text, "A");
/// ```
#[must_use]
pub fn shape_with_fallback(
    primary: Font,
    fallbacks: &[EmbeddedFontId],
    size_pt: f32,
    text: &str,
) -> Vec<WordSubRun> {
    if text.is_empty() {
        return Vec::new();
    }

    let primary_id = match primary {
        Font::Base14(_) => {
            // Base14: no glyph stream available, fallback doesn't apply.
            return vec![WordSubRun {
                font: primary,
                text: text.to_owned(),
                glyphs: Vec::new(),
                advance_pt: text_width(primary, size_pt, text),
            }];
        }
        Font::Embedded(id) => id,
    };

    let primary_ef = primary_id.data();
    let primary_glyphs = shape(primary_ef, text);

    if primary_glyphs.iter().all(|g| g.gid != 0) || fallbacks.is_empty() {
        // No `.notdef`, OR no fallbacks configured. One sub-run.
        return vec![into_subrun(
            primary,
            text.to_owned(),
            primary_glyphs,
            primary_ef.units_per_em,
            size_pt,
        )];
    }

    // Group primary glyphs by cluster. Each cluster covers source
    // bytes `[c_n..c_{n+1})` (last cluster runs to `text.len()`).
    let clusters = group_clusters(&primary_glyphs, text.len());

    // Per-cluster resolution: which face owns it + which glyphs to use.
    // `glyphs` here carry `cluster` offsets into the *parent* `text`;
    // rebasing to sub-run-local offsets happens at the merge step.
    let mut resolutions: Vec<ClusterResolution> = Vec::with_capacity(clusters.len());
    for cluster in &clusters {
        let has_notdef = cluster.glyphs.iter().any(|g| g.gid == 0);
        if !has_notdef {
            resolutions.push(ClusterResolution {
                font: primary,
                byte_range: cluster.byte_range.clone(),
                glyphs: cluster.glyphs.clone(),
            });
            continue;
        }
        // Retry against each fallback. Cluster-granular: replace the
        // entire cluster's glyph slice if a fallback covers it.
        let cluster_text = &text[cluster.byte_range.clone()];
        let mut accepted: Option<(Font, Vec<ShapedGlyph>)> = None;
        for &fb_id in fallbacks {
            let fb_font = Font::Embedded(fb_id);
            let fb_ef = fb_id.data();
            let fb_glyphs = shape(fb_ef, cluster_text);
            if !fb_glyphs.is_empty() && fb_glyphs.iter().all(|g| g.gid != 0) {
                // Shift fallback glyph clusters into the parent text's
                // coordinate system; rebasing to the sub-run's local
                // text happens in the merge step.
                // `cluster.byte_range.start` is a byte offset into a `&str`
                // that's at most `text.len()` bytes long. We pipe through
                // `u32::try_from` for the lint, saturating to u32::MAX in the
                // unreachable case of source strings ≥ 4 GiB.
                let shift = u32::try_from(cluster.byte_range.start).unwrap_or(u32::MAX);
                let shifted: Vec<_> = fb_glyphs
                    .into_iter()
                    .map(|g| ShapedGlyph {
                        cluster: g.cluster + shift,
                        ..g
                    })
                    .collect();
                accepted = Some((fb_font, shifted));
                break;
            }
        }
        match accepted {
            Some((fb_font, fb_glyphs)) => resolutions.push(ClusterResolution {
                font: fb_font,
                byte_range: cluster.byte_range.clone(),
                glyphs: fb_glyphs,
            }),
            None => resolutions.push(ClusterResolution {
                font: primary,
                byte_range: cluster.byte_range.clone(),
                glyphs: cluster.glyphs.clone(),
            }),
        }
    }

    // Merge adjacent same-font resolutions into one sub-run apiece.
    let mut subruns: Vec<WordSubRun> = Vec::new();
    let mut current: Option<(Font, std::ops::Range<usize>, Vec<ShapedGlyph>)> = None;
    for res in resolutions {
        match current.take() {
            Some((font, range, mut glyphs)) if font == res.font => {
                let new_range = range.start..res.byte_range.end;
                glyphs.extend(res.glyphs);
                current = Some((font, new_range, glyphs));
            }
            Some((font, range, glyphs)) => {
                subruns.push(finalize_subrun(font, range, glyphs, text, size_pt));
                current = Some((res.font, res.byte_range, res.glyphs));
            }
            None => current = Some((res.font, res.byte_range, res.glyphs)),
        }
    }
    if let Some((font, range, glyphs)) = current {
        subruns.push(finalize_subrun(font, range, glyphs, text, size_pt));
    }
    subruns
}

/// Internal: one HarfBuzz cluster's worth of primary-shaped glyphs
/// plus the cluster's source byte range.
struct ClusterGroup {
    byte_range: std::ops::Range<usize>,
    glyphs: Vec<ShapedGlyph>,
}

/// Internal: one cluster's resolution after fallback retry. `glyphs`
/// carry `cluster` offsets into the parent word text.
struct ClusterResolution {
    font: Font,
    byte_range: std::ops::Range<usize>,
    glyphs: Vec<ShapedGlyph>,
}

/// Walk a `rustybuzz`-ordered LTR glyph stream and group consecutive
/// glyphs sharing the same `cluster` value. Each group's byte range is
/// `[c..c_next)` where `c_next` is the next cluster's start (or
/// `text_len` for the last cluster). The shaper currently forces LTR;
/// RTL support must revisit this monotonic-cluster assumption.
fn group_clusters(glyphs: &[ShapedGlyph], text_len: usize) -> Vec<ClusterGroup> {
    let mut groups: Vec<ClusterGroup> = Vec::new();
    let mut i = 0;
    while i < glyphs.len() {
        let cluster = glyphs[i].cluster;
        let mut j = i + 1;
        while j < glyphs.len() && glyphs[j].cluster == cluster {
            j += 1;
        }
        let end_byte = if j < glyphs.len() {
            glyphs[j].cluster as usize
        } else {
            text_len
        };
        debug_assert!(end_byte >= cluster as usize);
        groups.push(ClusterGroup {
            byte_range: (cluster as usize)..end_byte,
            glyphs: glyphs[i..j].to_vec(),
        });
        i = j;
    }
    groups
}

/// Convert a `(font, byte_range, parent-relative glyphs)` triple into
/// a fully-baked [`WordSubRun`]: slices `parent_text` to the sub-run's
/// local string, rebases glyph clusters to that local string, sums
/// the advance.
fn finalize_subrun(
    font: Font,
    byte_range: std::ops::Range<usize>,
    glyphs: Vec<ShapedGlyph>,
    parent_text: &str,
    size_pt: f32,
) -> WordSubRun {
    let local_text = parent_text[byte_range.clone()].to_owned();
    // Sub-run byte ranges are bounded by the parent word's `text.len()`,
    // always well below u32::MAX in practice. Saturating cast keeps clippy
    // happy without an `#[allow]` annotation; the saturation branch is
    // unreachable for any realistic input.
    let shift = u32::try_from(byte_range.start).unwrap_or(u32::MAX);
    let rebased: Vec<_> = glyphs
        .into_iter()
        .map(|g| ShapedGlyph {
            cluster: g.cluster.saturating_sub(shift),
            ..g
        })
        .collect();
    let advance_pt: f32 = match font {
        Font::Embedded(id) => {
            let upem = embedded_upem(id);
            rebased
                .iter()
                .map(|g| advance_units_to_pt(g.advance_units, size_pt, upem))
                .sum()
        }
        Font::Base14(_) => text_width(font, size_pt, &local_text),
    };
    WordSubRun {
        font,
        text: local_text,
        glyphs: rebased,
        advance_pt,
    }
}

/// Single-sub-run packaging when no fallback retry was needed. Skips
/// the rebasing branch: the input glyphs already have cluster offsets
/// relative to `local_text` (which is the full parent text).
fn into_subrun(
    font: Font,
    text: String,
    glyphs: Vec<ShapedGlyph>,
    upem: u16,
    size_pt: f32,
) -> WordSubRun {
    let upem_f = f32::from(upem);
    let advance_pt: f32 = glyphs
        .iter()
        .map(|g| advance_units_to_pt(g.advance_units, size_pt, upem_f))
        .sum();
    WordSubRun {
        font,
        text,
        glyphs,
        advance_pt,
    }
}

/// `units_per_em` for an embedded face.
fn embedded_upem(id: EmbeddedFontId) -> f32 {
    f32::from(id.data().units_per_em)
}

/// Convert a font-unit advance to PDF user-space points at `size_pt`,
/// given the face's units-per-em. Values are carried as `i32` because
/// shapers use signed advances, but current embedded output normalizes
/// to PDF-emittable `hmtx` advances in `0..=65535`. Preserve sign here
/// anyway so future positioned shaping cannot turn a negative adjustment
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
pub fn advance_units_to_pt(advance_units: i32, size_pt: f32, upem: f32) -> f32 {
    let magnitude = u16::try_from(advance_units.unsigned_abs()).unwrap_or(u16::MAX);
    let advance = f32::from(magnitude);
    if advance_units.is_negative() {
        -advance * size_pt / upem
    } else {
        advance * size_pt / upem
    }
}

/// Width of a single glyph in `font` at `size` points. For Base14
/// faces this is one AFM lookup; for embedded faces it shapes the
/// single character. Used by the paragraph engine for character-wise
/// hyphenation of oversized words.
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
    fn embedded_shape_advances_match_pdf_width_path() {
        let ef = EmbeddedFontId::Regular.data();
        let glyphs = shape(ef, "AV");
        assert!(!glyphs.is_empty());
        for glyph in &glyphs {
            assert_eq!(glyph.advance_units, i32::from(ef.advance_units(glyph.gid)));
            assert_eq!(glyph.x_offset_units, 0);
            assert_eq!(glyph.y_offset_units, 0);
        }
    }

    #[test]
    fn advance_units_to_pt_preserves_negative_sign() {
        let actual = advance_units_to_pt(-1000, 12.0, 1000.0);
        assert!((actual + 12.0).abs() < f32::EPSILON, "got {actual}");
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

    #[test]
    fn fallback_empty_text_returns_empty() {
        let primary = Font::Embedded(EmbeddedFontId::Regular);
        let fallbacks = &[EmbeddedFontId::Math];
        assert!(shape_with_fallback(primary, fallbacks, 11.0, "").is_empty());
    }

    #[test]
    fn fallback_pure_primary_returns_single_subrun() {
        // Pure ASCII is fully covered by Noto Sans Regular — no
        // fallback needed; one sub-run, primary-owned glyphs.
        let primary = Font::Embedded(EmbeddedFontId::Regular);
        let fallbacks = &[EmbeddedFontId::Math];
        let subs = shape_with_fallback(primary, fallbacks, 11.0, "Hello");
        assert_eq!(subs.len(), 1, "expected one sub-run, got {}", subs.len());
        assert_eq!(subs[0].font, primary);
        assert_eq!(subs[0].text, "Hello");
        assert!(!subs[0].glyphs.is_empty());
        assert!(subs[0].glyphs.iter().all(|g| g.gid != 0));
        assert!(subs[0].advance_pt > 0.0);
    }

    #[test]
    fn fallback_mixed_latin_and_math_produces_alternating_subruns() {
        // `a≤b`: Latin `a` covered by Regular, math `≤` (U+2264) needs
        // the Math fallback, Latin `b` covered by Regular again. Expect
        // three sub-runs in source order. Each sub-run's text is its
        // own slice; glyph clusters rebased to local text.
        let primary = Font::Embedded(EmbeddedFontId::Regular);
        let fallbacks = &[EmbeddedFontId::Math];
        let subs = shape_with_fallback(primary, fallbacks, 11.0, "a\u{2264}b");
        assert_eq!(subs.len(), 3, "expected 3 sub-runs, got {subs:?}");

        assert_eq!(subs[0].font, primary);
        assert_eq!(subs[0].text, "a");

        assert_eq!(subs[1].font, Font::Embedded(EmbeddedFontId::Math));
        assert_eq!(subs[1].text, "\u{2264}");
        // Math sub-run glyph clusters must be local: cluster 0 points
        // at the start of "≤", not at offset 1 in the parent word.
        assert!(
            subs[1]
                .glyphs
                .iter()
                .all(|g| (g.cluster as usize) < subs[1].text.len()),
            "math sub-run glyph clusters not rebased: {:?}",
            subs[1].glyphs,
        );

        assert_eq!(subs[2].font, primary);
        assert_eq!(subs[2].text, "b");
    }

    #[test]
    fn fallback_all_fail_keeps_primary_notdef() {
        // Emoji 🎉 (U+1F389) is not in Noto Sans Regular OR Math.
        // The cluster stays with primary as `.notdef`; no panic, no
        // duplication, copy-paste yields the source codepoint via the
        // existing `/ToUnicode` machinery (PDF reader paints empty box).
        let primary = Font::Embedded(EmbeddedFontId::Regular);
        let fallbacks = &[EmbeddedFontId::Math];
        let subs = shape_with_fallback(primary, fallbacks, 11.0, "\u{1F389}");
        assert_eq!(subs.len(), 1);
        assert_eq!(subs[0].font, primary);
        assert!(
            subs[0].glyphs.iter().any(|g| g.gid == 0),
            "expected .notdef for unsupported emoji, got {:?}",
            subs[0].glyphs,
        );
    }

    #[test]
    fn fallback_base14_primary_returns_empty_glyphs_subrun() {
        // Base14 has no glyph stream to inspect for `.notdef`; fallback
        // doesn't apply. One sub-run, empty glyphs, advance via the
        // AFM `text_width` path.
        let primary = Font::Base14(Base14Font::Helvetica);
        let fallbacks = &[EmbeddedFontId::Math];
        let subs = shape_with_fallback(primary, fallbacks, 11.0, "Hello");
        assert_eq!(subs.len(), 1);
        assert_eq!(subs[0].font, primary);
        assert!(subs[0].glyphs.is_empty());
        assert!(subs[0].advance_pt > 0.0);
    }

    #[test]
    fn fallback_no_fallbacks_configured_returns_single_subrun_even_with_notdef() {
        // Empty fallback chain — even if the primary has .notdef, we
        // produce one sub-run with whatever the primary shaped. PDF
        // reader paints empty boxes; no panic.
        let primary = Font::Embedded(EmbeddedFontId::Regular);
        let subs = shape_with_fallback(primary, &[], 11.0, "\u{2264}");
        assert_eq!(subs.len(), 1);
        assert_eq!(subs[0].font, primary);
    }

    #[test]
    fn fallback_math_subrun_advance_uses_math_face_upem() {
        // Sanity: the math sub-run's `advance_pt` is computed from
        // Math's `units_per_em`, not Regular's. Both happen to be
        // 1000 in Noto Sans, but the contract should hold regardless.
        let primary = Font::Embedded(EmbeddedFontId::Regular);
        let fallbacks = &[EmbeddedFontId::Math];
        let subs = shape_with_fallback(primary, fallbacks, 11.0, "\u{2264}");
        assert_eq!(subs.len(), 1);
        assert_eq!(subs[0].font, Font::Embedded(EmbeddedFontId::Math));
        assert!(subs[0].advance_pt > 0.0);
    }
}
