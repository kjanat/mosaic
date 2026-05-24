use mos_fonts::{EmbeddedFontId, Font, ShapedGlyph, WordSubRun, shape_with_fallback, text_width};

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
    /// Byte offsets into `text` where the source contained a U+00AD
    /// soft hyphen. The SHY codepoints are stripped before shaping
    /// (`split_soft_hyphens` in the layout crate); these offsets mark
    /// the cluster boundaries where the author permits a line break.
    /// The greedy breaker consults them via [`try_shy_break`] when a
    /// word would otherwise overflow the line: the chosen prefix gets
    /// a visible `-` appended and the suffix continues as the next
    /// word. The Knuth-Plass cutover will use the same offsets as
    /// flagged Penalty(50) items for optimal (non-greedy) selection.
    pub(crate) shy_break_offsets: Vec<usize>,
}

/// Result of splitting a [`Word`] at one of its SHY break offsets.
/// `prefix.text` already includes a trailing U+002D HYPHEN-MINUS and
/// its `width_pt` is the post-shape advance sum (including the
/// hyphen). `suffix.text` carries the remaining bytes with
/// `shy_break_offsets` rebased to the suffix's local indexing and
/// boundary offsets (0 / `len`) dropped.
#[derive(Clone, Debug)]
pub(crate) struct ShyBreak {
    pub(crate) prefix: Word,
    pub(crate) suffix: Word,
}

/// Try to break `word` at the latest SHY offset whose prefix-plus-
/// visible-hyphen fits in `max_prefix_width`. Returns `None` if no
/// valid offset fits. Offsets equal to `0` or `word.text.len()`
/// (leading / trailing SHY) are ignored, matching the rule that a
/// break must produce a non-empty visible prefix and a non-empty
/// suffix. Consecutive duplicate offsets (e.g. `a\u{AD}\u{AD}b` →
/// `[1, 1]`) are deduped on the fly.
///
/// Re-shapes both halves through [`shape_with_fallback`] so the
/// resulting [`Word::width_pt`] matches the post-shape subrun
/// advances exactly — the cheap [`text_width`] estimate used during
/// the fit search may differ from the shaped width when fallback
/// splits the run, so the shaped sum is the authoritative value and
/// is re-checked against `max_prefix_width` before the candidate is
/// accepted.
pub(crate) fn try_shy_break(
    word: &Word,
    max_prefix_width: f32,
    fallbacks: &[EmbeddedFontId],
) -> Option<ShyBreak> {
    if word.shy_break_offsets.is_empty() {
        return None;
    }
    let text_len = word.text.len();
    // Walk offsets right-to-left so the first candidate that fits is
    // the latest fitting break (greedy = prefer longer prefix).
    let mut seen: Option<usize> = None;
    for &off in word.shy_break_offsets.iter().rev() {
        if off == 0 || off >= text_len {
            continue;
        }
        if seen == Some(off) {
            continue;
        }
        seen = Some(off);
        let Some(prefix_src) = word.text.get(..off) else {
            continue;
        };
        let mut prefix_text = String::with_capacity(prefix_src.len() + 1);
        prefix_text.push_str(prefix_src);
        prefix_text.push('-');
        // Cheap pre-shape estimate so we only re-shape the winner.
        let estimate = text_width(word.font, word.size_pt, &prefix_text);
        if estimate > max_prefix_width {
            continue;
        }
        let prefix_subruns = shape_with_fallback(word.font, fallbacks, word.size_pt, &prefix_text);
        let prefix_width: f32 = prefix_subruns.iter().map(|s| s.advance_pt).sum();
        if prefix_width > max_prefix_width {
            // Rounding pushed the shaped width just over; try the
            // next-smaller candidate rather than emit an overflow.
            continue;
        }
        let Some(suffix_src) = word.text.get(off..) else {
            continue;
        };
        let suffix_text = suffix_src.to_owned();
        let suffix_len = suffix_text.len();
        let suffix_offsets: Vec<usize> = word
            .shy_break_offsets
            .iter()
            .filter_map(|&o| {
                if o > off {
                    let rebased = o - off;
                    if rebased > 0 && rebased < suffix_len {
                        Some(rebased)
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
            .collect();
        let suffix_subruns = shape_with_fallback(word.font, fallbacks, word.size_pt, &suffix_text);
        let suffix_width: f32 = suffix_subruns.iter().map(|s| s.advance_pt).sum();
        let prefix = Word {
            text: prefix_text,
            actual_text: None,
            font: word.font,
            size_pt: word.size_pt,
            width_pt: prefix_width,
            subruns: prefix_subruns,
            // The hyphenated side has committed a break already; no
            // further SHY breaks live on it.
            shy_break_offsets: Vec::new(),
        };
        let suffix = Word {
            text: suffix_text,
            actual_text: None,
            font: word.font,
            size_pt: word.size_pt,
            width_pt: suffix_width,
            subruns: suffix_subruns,
            shy_break_offsets: suffix_offsets,
        };
        return Some(ShyBreak { prefix, suffix });
    }
    None
}

/// Inline item emitted by `collect_words`. The greedy line-breaker
/// (and, later, the Knuth-Plass breaker) walks the stream and emits
/// page geometry; `HardBreak` is a sentinel that forces a flush of
/// the in-progress line without contributing any glyphs.
#[derive(Clone, Debug)]
pub(crate) enum WordItem {
    Word(Word),
    HardBreak,
}

/// Strip U+00AD (soft hyphen) codepoints from `text` and return the
/// stripped string plus the byte offsets *in the stripped output*
/// where each SHY originally sat. The offsets mark the codepoint
/// boundary *after* the preceding cluster: a break taken at offset
/// `o` leaves bytes `[0..o)` on the previous line and `[o..)` on the
/// next.
///
/// The greedy line-breaker consumes these offsets through
/// [`try_shy_break`] when a word would overflow; the Knuth-Plass
/// cutover treats each as a flagged Penalty(50) item with hyphen-glyph
/// advance as its post-break width.
///
/// `text` is expected to be NFC-normalized. NFC does not decompose
/// U+00AD, so no quasi-SHY sequences need to be handled.
pub(crate) fn split_soft_hyphens(text: &str) -> (String, Vec<usize>) {
    if !text.contains('\u{AD}') {
        return (text.to_owned(), Vec::new());
    }
    let mut stripped = String::with_capacity(text.len());
    let mut offsets = Vec::new();
    for ch in text.chars() {
        if ch == '\u{AD}' {
            offsets.push(stripped.len());
        } else {
            stripped.push(ch);
        }
    }
    (stripped, offsets)
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

#[cfg(test)]
mod tests {
    use super::{Word, split_soft_hyphens, try_shy_break};
    use mos_fonts::{Base14Font, Font, WordSubRun, shape_with_fallback, text_width};

    fn make_shy_word(text: &str, offsets: Vec<usize>) -> Word {
        let font = Font::Base14(Base14Font::Helvetica);
        let size_pt = 12.0;
        let subruns: Vec<WordSubRun> = shape_with_fallback(font, &[], size_pt, text);
        let width_pt: f32 = subruns.iter().map(|s| s.advance_pt).sum();
        Word {
            text: text.to_owned(),
            actual_text: None,
            font,
            size_pt,
            width_pt,
            subruns,
            shy_break_offsets: offsets,
        }
    }

    #[test]
    fn split_soft_hyphens_no_op_when_absent() {
        let (stripped, offsets) = split_soft_hyphens("hello");
        assert_eq!(stripped, "hello");
        assert!(offsets.is_empty());
    }

    #[test]
    fn split_soft_hyphens_records_offsets_in_stripped_text() {
        // "super\u{AD}cali\u{AD}fragil" -> "supercalifragil", with
        // break opportunities at the byte offsets where SHY sat in
        // the *stripped* text (i.e. between the preceding and
        // following clusters in the rendered word).
        let (stripped, offsets) = split_soft_hyphens("super\u{AD}cali\u{AD}fragil");
        assert_eq!(stripped, "supercalifragil");
        assert_eq!(offsets, vec![5, 9]);
    }

    #[test]
    fn split_soft_hyphens_handles_consecutive_shy() {
        // Two SHYs in a row collapse to the same offset (no
        // codepoints separate them in the stripped output).
        let (stripped, offsets) = split_soft_hyphens("a\u{AD}\u{AD}b");
        assert_eq!(stripped, "ab");
        assert_eq!(offsets, vec![1, 1]);
    }

    #[test]
    fn try_shy_break_returns_none_when_no_offsets() {
        let word = make_shy_word("hello", Vec::new());
        assert!(try_shy_break(&word, 1000.0, &[]).is_none());
    }

    #[test]
    fn try_shy_break_picks_latest_offset_that_fits() {
        // "supercalifragil" with breaks at byte 5 ("super") and 9 ("cali").
        // A generous width admits both; the latest one (9) wins.
        let word = make_shy_word("supercalifragil", vec![5, 9]);
        let result = try_shy_break(&word, 1000.0, &[]).expect("must split");
        assert_eq!(result.prefix.text, "supercali-");
        assert_eq!(result.suffix.text, "fragil");
        assert!(result.suffix.shy_break_offsets.is_empty());
    }

    #[test]
    fn try_shy_break_falls_back_to_earlier_offset_when_latest_overflows() {
        // Tight width: "supercali-" (10 chars) too wide, "super-" (6) fits.
        let word = make_shy_word("supercalifragil", vec![5, 9]);
        let font = word.font;
        let size = word.size_pt;
        let max = text_width(font, size, "super-") + 0.5;
        let result = try_shy_break(&word, max, &[]).expect("must split");
        assert_eq!(result.prefix.text, "super-");
        assert_eq!(result.suffix.text, "califragil");
        // Suffix retains the later SHY rebased: 9 - 5 = 4.
        assert_eq!(result.suffix.shy_break_offsets, vec![4]);
    }

    #[test]
    fn try_shy_break_ignores_leading_and_trailing_offsets() {
        // Offsets at 0 and len() must never be chosen.
        let word = make_shy_word("foo", vec![0, 3]);
        assert!(try_shy_break(&word, 1000.0, &[]).is_none());
    }

    #[test]
    fn try_shy_break_returns_none_when_no_break_fits() {
        // Width smaller than even the shortest prefix+hyphen.
        let word = make_shy_word("supercalifragil", vec![5, 9]);
        assert!(try_shy_break(&word, 1.0, &[]).is_none());
    }

    #[test]
    fn try_shy_break_dedupes_consecutive_duplicate_offsets() {
        // `a\u{AD}\u{AD}b` produces offsets [1, 1]; both point to
        // the same break, so dedupe-on-the-fly avoids re-shaping.
        let word = make_shy_word("ab", vec![1, 1]);
        let result = try_shy_break(&word, 1000.0, &[]).expect("must split");
        assert_eq!(result.prefix.text, "a-");
        assert_eq!(result.suffix.text, "b");
    }
}
