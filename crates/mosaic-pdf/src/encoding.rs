//! Per-document `/Differences`-based encoding planning for the Core 14
//! Latin fonts.
//!
//! ## Problem
//!
//! PDF single-byte fonts address at most 256 glyphs. The Core 14
//! `WinAnsiEncoding` only carries the ~216 Latin-1+Windows glyphs that
//! every PDF reader ships built-in (Annex D.2). But each Core 14 AFM
//! also lists 99 extra glyphs — Latin Extended-A (`Ł`, `ł`, `Ě`, …),
//! the Romanian comma-below set, the spacing diacritics, the math
//! operators, the `fi`/`fl` ligatures — that have no `WinAnsi` byte.
//!
//! PDF's escape hatch is the `/Encoding` dictionary with a
//! `/Differences` array: it lets us declare "byte 0x7F means
//! `/lslash`, byte 0x81 means `/Lslash`, byte 0x90 means `/ecaron`"
//! and so on, sitting on top of `WinAnsiEncoding` as the base. The
//! glyph outlines still come from the reader's built-in Helvetica/
//! Times/Courier; we just rearrange which byte addresses which glyph
//! name from the AFM. No font data ships.
//!
//! ## Algorithm
//!
//! For each Latin Core 14 face actually used by some text run:
//!
//! 1. Walk every char of every run and partition into:
//!    - `WinAnsi natives`: have `winansi_byte(ch) = Some(b)`. The byte
//!      `b` is **claimed** — it can't be repurposed for a `Differences`
//!      remap because the content stream already uses it.
//!    - `Extended`: no `winansi_byte`, but `extended_glyph_name(ch)`
//!      resolves to an AFM glyph name. Needs a remapped slot.
//!    - `Unmappable`: neither (Cyrillic, CJK, emoji). Won't occur in
//!      practice — the layout engine substitutes these to `?` upstream.
//!      We treat them defensively as `?` here.
//!
//! 2. Allocate slots for the extended set from a deterministic free
//!    pool:
//!    a. The six `WinAnsi` gap bytes `0x7F, 0x81, 0x8D, 0x8F, 0x90,
//!    0x9D` first — these are guaranteed unmapped in `WinAnsiEncoding`
//!    and produce stable golden output for the common case (≤ 6
//!    extended glyphs).
//!    b. Then unused `0x20..=0xFF` slots in descending order. Going
//!    high-to-low keeps short ASCII-heavy paragraphs from perturbing
//!    low-byte slots; documents with rich punctuation at `0xE0..0xFF`
//!    still get plenty of room from `0x20..0x7E`.
//!
//! 3. If we exhaust the pool before placing every extended char,
//!    emit `W041` and drop the overflow (those chars render as `?`).
//!
//! ## Output
//!
//! [`DocEncoding`] carries everything the PDF emit code needs: the
//! `/Differences` pairs (slot → AFM glyph name) for the font dict,
//! `byte_for_char` for the content-stream encoder, and
//! `to_unicode_entries` for the `/ToUnicode` `CMap` so copy-paste keeps
//! working.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use mosaic_core::{Diagnostic, DiagnosticCode, Severity};
use mosaic_layout::{Base14Font, Font, TextRun};

// `Base14Font` doesn't derive `Ord`. Keying the planner's per-face
// bucket by `Font` (which doesn't either, but is Hash/Eq) and using a
// HashMap keeps the storage trivial; deterministic output comes from
// sorting `differences` at the end of `plan_face` and from iterating
// each face's char set through a `BTreeSet`.

/// Per-font planning output. The PDF emit path consumes this once per
/// document.
#[derive(Debug, Clone, Default)]
pub(crate) struct DocEncoding {
    /// Slots remapped on top of `WinAnsiEncoding`. Sorted ascending
    /// by slot so the emitted `/Differences` array is stable.
    pub differences: Vec<(u8, &'static str)>,
    /// Direct char → byte for the content-stream encoder. Covers
    /// both `WinAnsi` natives (mapping to their canonical byte) and
    /// extended chars (mapping to a remapped slot).
    pub byte_for_char: HashMap<char, u8>,
    /// Byte → original Unicode codepoint for the `/ToUnicode` `CMap`.
    /// Ascending by byte for stable output.
    pub to_unicode_entries: Vec<(u8, char)>,
}

impl DocEncoding {
    /// `true` if this face needs a custom `/Encoding` dict (one or
    /// more remapped slots). When `false`, callers should emit the
    /// existing `/Encoding /WinAnsiEncoding` shortcut and skip
    /// `/ToUnicode`.
    pub(crate) fn has_differences(&self) -> bool {
        !self.differences.is_empty()
    }
}

/// Two-phase encoding planner: caller streams every `(face, ch)` in
/// through [`Self::observe`], then calls [`Self::finalize`] to get one
/// [`DocEncoding`] per Latin Core 14 face that participated.
#[derive(Debug, Default)]
pub(crate) struct EncodingPlanner {
    /// Observed chars per face. `BTreeSet` for deterministic order
    /// during finalize, which keeps `/Differences` arrays byte-stable
    /// between runs. (`Base14Font` doesn't derive `Ord`, so the outer
    /// container is a `HashMap` — finalize sorts what matters.)
    used: HashMap<Base14Font, BTreeSet<char>>,
}

impl EncodingPlanner {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Record that `face` will need to render `ch`. Idempotent.
    /// `Symbol` and `ZapfDingbats` are silently ignored — those faces
    /// don't participate in `/Differences` planning (their encodings
    /// are different category entirely; see crate-level
    /// `pdf-base14-metrics` docs).
    pub(crate) fn observe(&mut self, face: Base14Font, ch: char) {
        if matches!(face, Base14Font::Symbol | Base14Font::ZapfDingbats) {
            return;
        }
        self.used.entry(face).or_default().insert(ch);
    }

    /// Convenience: feed every char of every text run.
    pub(crate) fn observe_runs(&mut self, runs: &[TextRun]) {
        for run in runs {
            for ch in run.text.chars() {
                self.observe(run.font.into(), ch);
            }
        }
    }

    /// Compute the per-face encoding plan. Any face never observed
    /// is absent from the returned map; callers should fall back to
    /// the predefined `WinAnsiEncoding` shortcut for those.
    ///
    /// Pushes a `W041` diagnostic when a face's extended-glyph budget
    /// overflows the 256-slot single-byte ceiling.
    pub(crate) fn finalize(self, diagnostics: &mut Vec<Diagnostic>) -> HashMap<Font, DocEncoding> {
        let mut out = HashMap::with_capacity(self.used.len());
        for (face, chars) in self.used {
            out.insert(Font(face), plan_face(face, &chars, diagnostics));
        }
        out
    }
}

/// Computes the encoding plan for a single face given the set of
/// chars the document needs from it.
fn plan_face(
    face: Base14Font,
    chars: &BTreeSet<char>,
    diagnostics: &mut Vec<Diagnostic>,
) -> DocEncoding {
    let mut byte_for_char: HashMap<char, u8> = HashMap::with_capacity(chars.len());
    // `byte → char` so we can build /ToUnicode at the end. Used bytes
    // include WinAnsi natives we claim and remapped slots.
    let mut to_unicode: BTreeMap<u8, char> = BTreeMap::new();
    // Bytes already claimed (either by a WinAnsi native or already
    // remapped). Indexed by u8 for O(1) probe.
    let mut claimed = [false; 256];
    // Extended chars that need a remapped slot.
    let mut extended: Vec<char> = Vec::new();

    for &ch in chars {
        if let Some(byte) = mosaic_fonts::winansi_byte(ch) {
            byte_for_char.insert(ch, byte);
            to_unicode.insert(byte, ch);
            claimed[usize::from(byte)] = true;
        } else if mosaic_fonts::extended_glyph_name(ch).is_some() {
            extended.push(ch);
        }
        // No `else`: layout substituted unmappable chars to `?`
        // already; the `?` is itself a WinAnsi native handled above.
    }

    // Skip the rest if nothing extended showed up — typical for
    // pure-ASCII or pure-Latin-1 documents. Empty differences signals
    // "use /Encoding /WinAnsiEncoding shortcut" to the emitter.
    if extended.is_empty() {
        return DocEncoding {
            differences: Vec::new(),
            byte_for_char,
            to_unicode_entries: to_unicode.into_iter().collect(),
        };
    }

    // Allocate slots. We materialise the preferred order into a
    // small `Vec` rather than chaining a filter over `claimed`,
    // because the borrow checker rejects a long-lived closure that
    // captures `&claimed` while we also mutate `claimed[slot] =
    // true` mid-loop. The eager Vec is at most 230 entries (6 gaps
    // + 0x20..=0xFF) so the cost is negligible.
    let mut free_slots: Vec<u8> = allocation_order()
        .filter(|&b| !claimed[usize::from(b)])
        .collect();
    free_slots.reverse(); // pop from the back to preserve our order

    let mut differences: Vec<(u8, &'static str)> = Vec::with_capacity(extended.len());
    let mut overflowed: usize = 0;

    for ch in extended {
        let Some(name) = mosaic_fonts::extended_glyph_name(ch) else {
            continue;
        };
        // Defensive: confirm the face actually carries this glyph.
        // For the 12 Latin Core 14 faces the AGL subset only points
        // at glyphs present in every Latin AFM, so this always
        // succeeds; the check guards against future expansion of
        // `extended_glyph_name` past the shared 315-name inventory.
        if face.glyph_width_by_name(name).is_none() {
            overflowed += 1;
            continue;
        }
        let Some(slot) = free_slots.pop() else {
            overflowed += 1;
            continue;
        };
        differences.push((slot, name));
        byte_for_char.insert(ch, slot);
        to_unicode.insert(slot, ch);
        claimed[usize::from(slot)] = true;
    }

    if overflowed > 0 {
        diagnostics.push(Diagnostic {
            severity: Severity::Warning,
            code: DiagnosticCode("W041"),
            message: format!(
                "extended glyph budget exhausted in {face:?}: {overflowed} \
                 character(s) could not be encoded in the 256-slot \
                 /Differences map and rendered as `?`"
            ),
            span: None,
            notes: Vec::new(),
            suggestions: Vec::new(),
        });
    }

    differences.sort_unstable_by_key(|&(b, _)| b);

    DocEncoding {
        differences,
        byte_for_char,
        to_unicode_entries: to_unicode.into_iter().collect(),
    }
}

/// Preferred slot-allocation order: the six `WinAnsi` gap bytes
/// first (predictable golden output for ≤ 6 extended glyphs), then
/// `0xFF..=0x20` descending **excluding** those same six bytes. We
/// deliberately skip `0x00..=0x1F` — PDF readers tolerate control
/// bytes in `/Differences`, but content streams that need an
/// `Str(...)` literal can run afoul of `\0`/`\r`/`\n` escaping, and
/// using high-byte slots first keeps short paragraphs from
/// perturbing low-byte slots.
///
/// The gap-exclusion in the descending tail is load-bearing: without
/// it the gap bytes would appear twice in the iterator (once at the
/// front, once again at their natural position 0x7F/0x81/…/0x9D), and
/// the planner — which doesn't re-check `claimed[slot]` after pop —
/// would allocate the same byte to two different extended chars once
/// `extended.len()` grew past ~104. Today the AGL subset has only
/// ~99 entries so this latent bug couldn't fire, but it's wrong on
/// principle. See `differences_have_unique_slots` for the regression
/// guard.
fn allocation_order() -> impl Iterator<Item = u8> {
    const GAPS: [u8; 6] = [0x7F, 0x81, 0x8D, 0x8F, 0x90, 0x9D];
    GAPS.into_iter()
        .chain((0x20_u8..=0xFF_u8).rev().filter(|b| !GAPS.contains(b)))
}

#[cfg(test)]
mod tests {
    // No `#![allow]` here — every test uses `assert!`/`assert_eq!`
    // for failure reporting; nothing reaches for `unwrap`/`expect`/
    // `panic!`. Setup helpers stay infallible by routing missing-key
    // lookups through `unwrap_or_default` (an Option combinator, not
    // a panic).
    use super::*;

    fn plan(face: Base14Font, text: &str) -> (DocEncoding, Vec<Diagnostic>) {
        let mut p = EncodingPlanner::new();
        for ch in text.chars() {
            p.observe(face, ch);
        }
        let mut diags = Vec::new();
        let mut out = p.finalize(&mut diags);
        let enc = out.remove(&Font(face)).unwrap_or_default();
        (enc, diags)
    }

    #[test]
    fn pure_ascii_needs_no_differences() {
        let (enc, diags) = plan(Base14Font::Helvetica, "Hello, world!");
        assert!(enc.differences.is_empty());
        assert!(diags.is_empty());
        // Every char round-trips through byte_for_char.
        assert_eq!(enc.byte_for_char.get(&'H'), Some(&b'H'));
        assert_eq!(enc.byte_for_char.get(&'!'), Some(&b'!'));
    }

    #[test]
    fn latin1_only_needs_no_differences() {
        // café Straße §1: every char is a WinAnsi native.
        let (enc, diags) = plan(Base14Font::Helvetica, "café Straße §1");
        assert!(enc.differences.is_empty());
        assert!(diags.is_empty());
        assert_eq!(enc.byte_for_char.get(&'é'), Some(&0xE9_u8));
        assert_eq!(enc.byte_for_char.get(&'ß'), Some(&0xDF_u8));
    }

    #[test]
    fn polish_lslash_lands_in_first_gap_slot() {
        // Łódź: Ł and ź are extended, óó d ż all WinAnsi. ź is U+017A
        // not WinAnsi.  Wait: ź is U+017A → AGL `zacute`, extended.
        // ó is U+00F3 → WinAnsi 0xF3. d is ASCII 0x64.
        let (enc, diags) = plan(Base14Font::Helvetica, "Łódź");
        assert!(diags.is_empty());
        // Two extended chars: Ł and ź. First two go to gap slots
        // 0x7F (Ł) and 0x81 (ź) since the BTreeSet iterates in
        // codepoint order (Ł=U+0141 < ź=U+017A).
        assert_eq!(enc.differences.len(), 2);
        assert_eq!(enc.differences[0], (0x7F, "Lslash"));
        assert_eq!(enc.differences[1], (0x81, "zacute"));
        assert_eq!(enc.byte_for_char.get(&'Ł'), Some(&0x7F_u8));
        assert_eq!(enc.byte_for_char.get(&'ź'), Some(&0x81_u8));
        // WinAnsi natives keep their canonical byte.
        assert_eq!(enc.byte_for_char.get(&'ó'), Some(&0xF3_u8));
        assert_eq!(enc.byte_for_char.get(&'d'), Some(&b'd'));
    }

    #[test]
    fn czech_uses_only_gap_slots_when_under_6() {
        // "Příliš žluťoučký kůň" — extended chars: ě? no, "Příliš":
        // Příliš = P, ř, í, l, i, š. ř (U+0159) is extended (rcaron).
        // š (U+0161) is WinAnsi (0x9A). í (U+00ED) is WinAnsi (0xED).
        // So one extended char: ř → first gap slot 0x7F.
        let (enc, _) = plan(Base14Font::Helvetica, "Příliš");
        assert_eq!(enc.differences.len(), 1);
        assert_eq!(enc.differences[0], (0x7F, "rcaron"));
    }

    #[test]
    fn to_unicode_covers_every_used_byte() {
        let (enc, _) = plan(Base14Font::Helvetica, "Łódź");
        // Every byte we emit (whether a WinAnsi native or a remap)
        // must round-trip back to its Unicode codepoint.
        let map: HashMap<u8, char> = enc.to_unicode_entries.iter().copied().collect();
        assert_eq!(map.get(&0x7F), Some(&'Ł'));
        assert_eq!(map.get(&0x81), Some(&'ź'));
        assert_eq!(map.get(&0xF3), Some(&'ó'));
        assert_eq!(map.get(&b'd'), Some(&'d'));
    }

    #[test]
    fn budget_exhaustion_emits_w041() {
        // Force overflow: claim every ASCII printable + every Latin-1
        // codepoint as a WinAnsi native, then ask for 62 extended
        // chars. ASCII (95) + Latin-1 0xA0..=0xFF (96) = 191 slots
        // claimed; pool starts at 230, so 39 free + 6 gaps already in
        // pool ... wait: gaps are in the 230 count, not on top. So
        // free pool after these claims = 230 - 191 = 39 slots.
        // Extended chars requested = 62. Overflow = 62 - 39 = 23.
        let mut all_chars: BTreeSet<char> = BTreeSet::new();
        for b in 0x20_u8..=0x7E_u8 {
            all_chars.insert(char::from(b));
        }
        for c in '\u{00A0}'..='\u{00FF}' {
            all_chars.insert(c);
        }
        // 62 extended codepoints from the AGL subset (every Latin
        // Extended-A entry minus those already in WinAnsi like š/Š/ž/Ž).
        for c in [
            '\u{0102}', '\u{0103}', '\u{0104}', '\u{0105}', '\u{0106}', '\u{0107}', '\u{010C}',
            '\u{010D}', '\u{010E}', '\u{010F}', '\u{0110}', '\u{0111}', '\u{0118}', '\u{0119}',
            '\u{011A}', '\u{011B}', '\u{011E}', '\u{011F}', '\u{0122}', '\u{0123}', '\u{0136}',
            '\u{0137}', '\u{0139}', '\u{013A}', '\u{013B}', '\u{013C}', '\u{013D}', '\u{013E}',
            '\u{0141}', '\u{0142}', '\u{0143}', '\u{0144}', '\u{0145}', '\u{0146}', '\u{0147}',
            '\u{0148}', '\u{0150}', '\u{0151}', '\u{0154}', '\u{0155}', '\u{0156}', '\u{0157}',
            '\u{0158}', '\u{0159}', '\u{015A}', '\u{015B}', '\u{015E}', '\u{015F}', '\u{0162}',
            '\u{0163}', '\u{0164}', '\u{0165}', '\u{016E}', '\u{016F}', '\u{0170}', '\u{0171}',
            '\u{0172}', '\u{0173}', '\u{0179}', '\u{017A}', '\u{017B}', '\u{017C}',
        ] {
            all_chars.insert(c);
        }
        let mut diags = Vec::new();
        let enc = plan_face(Base14Font::Helvetica, &all_chars, &mut diags);
        // 39 differences max — the pool size after WinAnsi claims.
        // (Some Windows-band codepoints in 0xA0..=0xFF actually claim
        // bytes that overlap our pool 0x20..=0xFF, which is exactly
        // the design.)
        assert!(
            enc.differences.len() < 62,
            "expected overflow, got {} differences",
            enc.differences.len()
        );
        assert_eq!(diags.len(), 1, "expected exactly one W041");
        assert_eq!(diags[0].code.0, "W041");
        assert!(
            diags[0].message.contains("budget exhausted"),
            "msg = {:?}",
            diags[0].message
        );
    }

    #[test]
    fn symbol_and_dingbats_are_ignored() {
        let mut p = EncodingPlanner::new();
        p.observe(Base14Font::Symbol, 'A');
        p.observe(Base14Font::ZapfDingbats, 'A');
        let mut diags = Vec::new();
        let out = p.finalize(&mut diags);
        assert!(out.is_empty());
        assert!(diags.is_empty());
    }

    #[test]
    fn allocation_order_starts_with_gaps_then_descends_without_dups() {
        let mut order = allocation_order();
        assert_eq!(order.next(), Some(0x7F));
        assert_eq!(order.next(), Some(0x81));
        assert_eq!(order.next(), Some(0x8D));
        assert_eq!(order.next(), Some(0x8F));
        assert_eq!(order.next(), Some(0x90));
        assert_eq!(order.next(), Some(0x9D));
        assert_eq!(order.next(), Some(0xFF));
        assert_eq!(order.next(), Some(0xFE));
        // 6 gaps + (0xFF - 0x20 + 1 - 6 gaps in that range = 218) = 224.
        let all: Vec<u8> = allocation_order().collect();
        assert_eq!(all.len(), 6 + 218);
        // Every slot appears at most once.
        let unique: BTreeSet<u8> = all.iter().copied().collect();
        assert_eq!(
            unique.len(),
            all.len(),
            "allocation_order yields duplicate slots"
        );
        // The descending tail should never re-emit a gap byte.
        for &b in &all[6..] {
            assert!(
                !matches!(b, 0x7F | 0x81 | 0x8D | 0x8F | 0x90 | 0x9D),
                "gap byte 0x{b:02X} re-appears in descending tail"
            );
        }
    }

    #[test]
    fn differences_have_unique_slots() {
        // Regression guard for the latent slot-dup bug: even when the
        // planner has to dip into the descending range past the 0x9D
        // gap, every entry in `differences` must address a distinct
        // byte. We feed the full AGL_SUBSET worth of extended chars
        // through a face that has every glyph (Helvetica), claim every
        // ASCII printable byte too so the descending pool is exercised,
        // then assert uniqueness.
        let mut all_chars: BTreeSet<char> = BTreeSet::new();
        for b in 0x20_u8..=0x7E_u8 {
            all_chars.insert(char::from(b));
        }
        for c in [
            '\u{0100}', '\u{0101}', '\u{0102}', '\u{0103}', '\u{0104}', '\u{0105}', '\u{0106}',
            '\u{0107}', '\u{010C}', '\u{010D}', '\u{010E}', '\u{010F}', '\u{0110}', '\u{0111}',
            '\u{0112}', '\u{0113}', '\u{0116}', '\u{0117}', '\u{0118}', '\u{0119}', '\u{011A}',
            '\u{011B}', '\u{011E}', '\u{011F}', '\u{0122}', '\u{0123}', '\u{012A}', '\u{012B}',
            '\u{012E}', '\u{012F}', '\u{0130}', '\u{0131}', '\u{0136}', '\u{0137}', '\u{0139}',
            '\u{013A}', '\u{013B}', '\u{013C}', '\u{013D}', '\u{013E}', '\u{0141}', '\u{0142}',
            '\u{0143}', '\u{0144}', '\u{0145}', '\u{0146}', '\u{0147}', '\u{0148}', '\u{014C}',
            '\u{014D}', '\u{0150}', '\u{0151}', '\u{0154}', '\u{0155}', '\u{0156}', '\u{0157}',
            '\u{0158}', '\u{0159}', '\u{015A}', '\u{015B}', '\u{015E}', '\u{015F}', '\u{0162}',
            '\u{0163}', '\u{0164}', '\u{0165}', '\u{016A}', '\u{016B}', '\u{016E}', '\u{016F}',
            '\u{0170}', '\u{0171}', '\u{0172}', '\u{0173}', '\u{0179}', '\u{017A}', '\u{017B}',
            '\u{017C}',
        ] {
            all_chars.insert(c);
        }
        let mut diags = Vec::new();
        let enc = plan_face(Base14Font::Helvetica, &all_chars, &mut diags);
        let slots: BTreeSet<u8> = enc.differences.iter().map(|&(b, _)| b).collect();
        assert_eq!(
            slots.len(),
            enc.differences.len(),
            "duplicate slot in /Differences: {:?}",
            enc.differences
        );
        // Same uniqueness invariant for the byte → char map.
        let bytes: BTreeSet<u8> = enc.byte_for_char.values().copied().collect();
        assert_eq!(
            bytes.len(),
            enc.byte_for_char.len(),
            "byte_for_char maps two chars to the same byte"
        );
    }

    #[test]
    fn differences_are_sorted_by_slot() {
        // Mixed Czech + Polish + math: should still produce
        // ascending-slot differences.
        let (enc, _) = plan(Base14Font::Helvetica, "ł ě √ ≤");
        for w in enc.differences.windows(2) {
            assert!(w[0].0 < w[1].0, "out of order: {:?} vs {:?}", w[0], w[1]);
        }
    }
}
