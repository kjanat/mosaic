//! Vendor-equivalence check: the hand-curated `WINANSI_CHAR_MAP_LITERAL`
//! transcribed from PDF 1.7 Annex D.2 Table D.2 must match, byte for
//! byte, the AGL-derived `WINANSI_CHAR_MAP` baked by `build.rs`.
//!
//! The AGL-derived table is the oracle (it cross-references
//! `WINANSI_TABLE` glyph names against the canonical Adobe Glyph List
//! at build time). If the two diverge, fix the hand-curated table;
//! the AGL one is allowed to be opaque, the hand-curated one isn't.

use pdf_base14_metrics::{__WINANSI_CHAR_MAP_AGL, __WINANSI_CHAR_MAP_LITERAL};

#[test]
fn hand_curated_matches_agl_vendor() {
    let mut diffs: Vec<String> = Vec::new();
    for byte in 0u8..=u8::MAX {
        let hand = __WINANSI_CHAR_MAP_LITERAL[byte as usize];
        let agl = __WINANSI_CHAR_MAP_AGL[byte as usize];
        if hand != agl {
            diffs.push(format!(
                "  0x{byte:02X}: hand-curated = {hand:?}, AGL-derived = {agl:?}"
            ));
        }
    }
    assert!(
        diffs.is_empty(),
        "hand-curated WinAnsi map diverges from AGL-derived oracle at {} byte(s):\n{}",
        diffs.len(),
        diffs.join("\n"),
    );
}

#[test]
fn winansi_gaps_match_pdf_spec() {
    // PDF 1.7 Annex D.2 gaps the encoding leaves unassigned. Both
    // maps must agree these slots are `None`; this is a sanity check
    // that catches "off-by-one in the gap region" before the broader
    // equivalence test even runs.
    for &gap in &[0x7Fu8, 0x81, 0x8D, 0x8F, 0x90, 0x9D] {
        assert_eq!(
            __WINANSI_CHAR_MAP_LITERAL[gap as usize], None,
            "hand-curated table should leave 0x{gap:02X} unassigned",
        );
        assert_eq!(
            __WINANSI_CHAR_MAP_AGL[gap as usize], None,
            "AGL-derived table should leave 0x{gap:02X} unassigned",
        );
    }
    // C0 controls 0x00..=0x1F are also unassigned in PDF WinAnsi.
    for byte in 0u8..=0x1F {
        assert_eq!(__WINANSI_CHAR_MAP_LITERAL[byte as usize], None);
        assert_eq!(__WINANSI_CHAR_MAP_AGL[byte as usize], None);
    }
}

#[test]
fn winansi_aliases_collapse_to_ascii() {
    // PDF 1.7 Annex D.2 explicitly aliases 0xA0 → `space` and
    // 0xAD → `hyphen` (the glyph names, which AGL resolves to ASCII
    // U+0020 and U+002D — not the Latin-1 NBSP/SHY one might expect
    // from CP1252). This is the single most likely place for a
    // transcription error in the hand-curated table.
    assert_eq!(__WINANSI_CHAR_MAP_LITERAL[0xA0], Some(' '));
    assert_eq!(__WINANSI_CHAR_MAP_LITERAL[0xAD], Some('-'));
    assert_eq!(__WINANSI_CHAR_MAP_AGL[0xA0], Some(' '));
    assert_eq!(__WINANSI_CHAR_MAP_AGL[0xAD], Some('-'));
}
