use crate::{
    EmbeddedFontId, Font, ShapedGlyph, advance_units_to_pt, normalize::nfc_text, shape, text_width,
};

/// Shape `text` against `font` and return both the glyph stream and
/// the advance widths in user-space points. Callers that need only the
/// width can use [`text_width`]; callers that will also emit glyphs
/// downstream should use this to avoid shaping twice.
///
/// Input is normalized through [`crate::nfc_text`] before shaping, so
/// decomposed sequences are precomposed to Unicode NFC. Returned glyph
/// cluster offsets are byte offsets into that normalized text, not
/// necessarily the caller's original string.
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
    let text = nfc_text(text);
    let text = text.as_ref();
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
/// Caller emits **one PDF `TextRun` per `WordSubRun`**: same
/// baseline, x-cursor advances by `advance_pt` between sub-runs: and
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
    /// Base14 sub-runs (Base14 has no glyph stream: PDF emit goes via
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
/// granular replacement, never partial: partial replacement would
/// duplicate bases, drop marks, break ligatures).
///
/// Returns one [`WordSubRun`] per contiguous source span that shares
/// a face. Each sub-run's `glyphs` `cluster` offsets are rebased to
/// the sub-run's local `text`, so `mos-pdf::plan_embedded` reads
/// `/ToUnicode` clusters with no awareness of the parent word.
///
/// Input is normalized through [`crate::nfc_text`] before fallback
/// shaping. Each returned [`WordSubRun::text`] is therefore a slice of
/// the normalized NFC string; decomposed caller input may not be
/// byte-identical to returned text.
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
    let text = nfc_text(text);
    let text = text.as_ref();
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Base14Font;

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
    fn embedded_shape_preserves_gpos_kerning() {
        let ef = EmbeddedFontId::Regular.data();
        let glyphs = shape(ef, "AV");
        assert!(!glyphs.is_empty());
        let nominal: i32 = glyphs
            .iter()
            .map(|g| i32::from(ef.advance_units(g.gid)))
            .sum();
        let shaped: i32 = glyphs.iter().map(|g| g.advance_units).sum();

        assert!(
            shaped < nominal,
            "expected AV kerning to tighten advance: shaped={shaped} nominal={nominal}"
        );
    }

    #[test]
    fn embedded_shape_preserves_combining_mark_offsets() {
        let ef = EmbeddedFontId::Regular.data();
        let glyphs = shape(ef, "q\u{0302}\u{0301}");
        assert!(
            glyphs.len() >= 3,
            "expected base 'q' + 2 combining marks, got {glyphs:?}"
        );
        assert!(
            glyphs[1..]
                .iter()
                .any(|g| g.x_offset_units != 0 || g.y_offset_units != 0),
            "expected at least one combining mark offset, got {glyphs:?}"
        );
    }

    #[test]
    fn shape_text_normalizes_decomposed_romanian() {
        let font = Font::Embedded(EmbeddedFontId::Regular);
        let decomposed = shape_text(font, 12.0, "S\u{0326}");
        let precomposed = shape_text(font, 12.0, "\u{0218}");

        let decomposed_gids: Vec<u16> = decomposed.glyphs.iter().map(|g| g.gid).collect();
        let precomposed_gids: Vec<u16> = precomposed.glyphs.iter().map(|g| g.gid).collect();
        assert_eq!(decomposed_gids, precomposed_gids);
        assert!((decomposed.advance_pt - precomposed.advance_pt).abs() < f32::EPSILON);
    }

    #[test]
    fn embedded_fi_ligature_collapses_glyphs() {
        // Noto Sans contains an `fi` ligature; rustybuzz returns one
        // glyph for `fi` (not two). The substituted gid differs from
        // both the standalone `f` and `i` gids. (Noto Sans's `fi`
        // ligature has the same advance as f+i: purely visual,
        // joining the dot of `i` with the terminal of `f`, so width
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
        // Pure ASCII is fully covered by Noto Sans Regular: no
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
    fn fallback_shape_normalizes_subrun_text() {
        let primary = Font::Embedded(EmbeddedFontId::Regular);
        let fallbacks = &[EmbeddedFontId::Math];
        let subs = shape_with_fallback(primary, fallbacks, 11.0, "S\u{0326}");

        assert_eq!(subs.len(), 1);
        assert_eq!(subs[0].text, "\u{0218}");
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
        // Empty fallback chain: even if the primary has .notdef, we
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
