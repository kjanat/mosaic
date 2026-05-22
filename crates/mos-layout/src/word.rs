use mos_fonts::{Font, ShapedGlyph, WordSubRun, text_width};

#[derive(Clone, Debug)]
pub(crate) struct Word {
    pub(crate) text: String,
    pub(crate) actual_text: Option<String>,
    /// Primary face -- the style-resolved choice from the active
    /// `FontFamily` (regular/bold/italic/monospace). Used for line
    /// metrics (ascent/descent), inter-word spacing, and
    /// character-wise hyphenation width estimates. Per-glyph fallback
    /// faces (e.g. Noto Sans Math for `<=`) live inside [`Word::subruns`].
    pub(crate) font: Font,
    pub(crate) size_pt: f32,
    /// Pre-computed advance width -- populated when the word is
    /// constructed in `collect_words` (sum of `subruns[i].advance_pt`)
    /// so the line-breaker doesn't re-measure on every comparison.
    pub(crate) width_pt: f32,
    /// Per-glyph-fallback sub-runs produced by `shape_with_fallback`.
    /// One sub-run per contiguous source span that shares a face;
    /// each carries its own font + text slice + glyph stream with
    /// cluster offsets rebased to its local text. `flush_line` emits
    /// one [`crate::TextRun`] per sub-run, advancing the x cursor by
    /// `subrun.advance_pt` between them. For Base14 primary faces
    /// the result is always a single sub-run with empty `glyphs`
    /// (no fallback target -- Base14 emit path uses `WinAnsi`-byte
    /// strings instead).
    pub(crate) subruns: Vec<WordSubRun>,
}

pub(crate) fn word_clusters(word: &Word) -> Vec<WordSubRun> {
    let mut clusters = Vec::new();
    for sub in &word.subruns {
        if sub.glyphs.is_empty() {
            for ch in sub.text.chars() {
                let mut text = String::new();
                text.push(ch);
                clusters.push(WordSubRun {
                    font: sub.font,
                    advance_pt: text_width(sub.font, word.size_pt, &text),
                    text,
                    glyphs: Vec::new(),
                });
            }
            continue;
        }

        let mut i = 0;
        while i < sub.glyphs.len() {
            let cluster = sub.glyphs[i].cluster;
            let mut j = i + 1;
            while j < sub.glyphs.len() && sub.glyphs[j].cluster == cluster {
                j += 1;
            }
            let start = usize::try_from(cluster).unwrap_or(usize::MAX);
            let end = if j < sub.glyphs.len() {
                usize::try_from(sub.glyphs[j].cluster).unwrap_or(usize::MAX)
            } else {
                sub.text.len()
            };
            debug_assert!(start <= end && end <= sub.text.len());
            let Some(text) = sub.text.get(start..end) else {
                i = j;
                continue;
            };
            let shift = u32::try_from(start).unwrap_or(u32::MAX);
            let glyphs: Vec<_> = sub.glyphs[i..j]
                .iter()
                .map(|g| ShapedGlyph {
                    cluster: g.cluster.saturating_sub(shift),
                    ..*g
                })
                .collect();
            clusters.push(WordSubRun {
                font: sub.font,
                text: text.to_owned(),
                advance_pt: glyphs_advance_pt(sub.font, word.size_pt, &glyphs),
                glyphs,
            });
            i = j;
        }
    }
    clusters
}

fn glyphs_advance_pt(font: Font, size_pt: f32, glyphs: &[ShapedGlyph]) -> f32 {
    let upem = match font {
        Font::Embedded(id) => f32::from(id.data().units_per_em),
        Font::Base14(_) => 1000.0,
    };
    // Sign-preserving conversion lives in mos-fonts to keep the
    // two crates from drifting on hmtx semantics.
    glyphs
        .iter()
        .map(|g| mos_fonts::advance_units_to_pt(g.advance_units, size_pt, upem))
        .sum()
}
