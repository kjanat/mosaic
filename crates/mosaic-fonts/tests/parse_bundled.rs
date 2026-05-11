//! Compile-time-baked TTF blobs must parse cleanly at runtime. The
//! library's `EmbeddedFontId::data()` resolves through a `LazyLock`
//! whose initializer asserts parse success; if any bundled cut were
//! corrupted at vendor time or by an LFS misconfiguration, the first
//! shaping call would panic. This test pulls every cut up front so a
//! parse regression surfaces as a CI failure on the PR that
//! introduced it, not as a runtime panic in production.

use mosaic_fonts::EmbeddedFontId;

#[test]
fn every_bundled_cut_parses_and_has_glyph_coverage() {
    for id in EmbeddedFontId::ALL {
        let font = id.data();
        // `units_per_em` is read straight from the `head` table during
        // parse. A non-zero value confirms the parse path completed
        // and produced a usable face.
        assert!(
            font.units_per_em > 0,
            "{id:?} has zero units_per_em — corrupt head table?",
        );
        // The face must cover at least basic Latin — if `A` is
        // missing, the TTF is corrupted or wildly misvendored.
        assert!(
            font.glyph_index('A').is_some(),
            "{id:?} has no glyph for `A`",
        );
    }
}

#[test]
fn cyrillic_and_greek_covered_by_regular_cut() {
    // The bundled Latin/Greek/Cyrillic package promises these scripts.
    // Catching coverage loss here is cheaper than parsing a rendered
    // PDF for `.notdef` glyphs in the round-trip test.
    let font = EmbeddedFontId::Regular.data();
    for ch in ['П', 'р', 'и', 'в', 'е', 'т', 'Κ', 'α', 'λ'] {
        assert!(
            font.glyph_index(ch).is_some(),
            "Noto Sans Regular missing glyph for U+{:04X} ({ch:?})",
            u32::from(ch),
        );
    }
}
