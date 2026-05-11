//! Compile-time-baked TTF blobs must parse cleanly at runtime. The
//! library's `EmbeddedFontId::data()` resolves through a `LazyLock`
//! whose initializer asserts parse success; if any bundled cut were
//! corrupted at vendor time or by an LFS misconfiguration, the first
//! shaping call would panic. This test pulls every cut up front so a
//! parse regression surfaces as a CI failure on the PR that
//! introduced it, not as a runtime panic in production.

use mosaic_fonts::EmbeddedFontId;

/// The fallback glyphs promised by the `hello` example.
const EXAMPLE_GLYPHS: &[char] = &['≤', '≥', '≠', '√', '∂', '∑', '−'];

/// Extra math glyphs the bundled fallback should keep covering.
const ADDITIONAL_MATH_GLYPHS: &[char] = &['Δ', '∆', '◊', '⁄'];

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
fn cyrillic_and_greek_covered_by_every_cut() {
    // The bundled Latin/Greek/Cyrillic package promises these scripts
    // for every style slot — a user writing `**Привет**` or `*Καλημέρα*`
    // routes through Bold or Italic, so coverage gaps there would
    // silently render as `.notdef` boxes inside emphasis runs.
    // Catching coverage loss here is cheaper than parsing a rendered
    // PDF for `.notdef` glyphs in the round-trip test.
    for id in EmbeddedFontId::ALL {
        let font = id.data();
        for ch in ['П', 'р', 'и', 'в', 'е', 'т', 'Κ', 'α', 'λ'] {
            assert!(
                font.glyph_index(ch).is_some(),
                "{id:?} missing glyph for U+{:04X} ({ch:?})",
                u32::from(ch),
            );
        }
    }
}

#[test]
fn math_cut_covers_documented_operators() {
    // The Math cut is wired as Noto Sans's lone fallback face. Every
    // glyph the workaround example used to pin Helvetica for must
    // resolve here, otherwise the fallback chain returns `.notdef` and
    // the user sees boxes instead of operators.
    let math = EmbeddedFontId::Math.data();
    for &ch in EXAMPLE_GLYPHS {
        assert!(
            math.glyph_index(ch).is_some(),
            "Math cut missing U+{:04X} ({ch:?}) — promised by the hello example",
            u32::from(ch),
        );
    }
    for &ch in ADDITIONAL_MATH_GLYPHS {
        assert!(
            math.glyph_index(ch).is_some(),
            "Math cut missing U+{:04X} ({ch:?}) — expected fallback coverage",
            u32::from(ch),
        );
    }
}

#[test]
fn primary_noto_sans_lacks_additional_math_operators() {
    // If a future re-vendor adds math operators directly to Noto Sans
    // Regular, the per-glyph fallback machinery becomes dead code for
    // these characters — shape_with_fallback would never see `.notdef`
    // clusters to retry. That's a footgun (CJK + emoji land in the
    // same code path), so we pin the contract: at least one broader
    // math operator must be absent from the primary face.
    let regular = EmbeddedFontId::Regular.data();
    let any_missing = ADDITIONAL_MATH_GLYPHS
        .iter()
        .any(|&ch| regular.glyph_index(ch).is_none());
    assert!(
        any_missing,
        "Noto Sans Regular now covers every additional math glyph — \
         the fallback path is no longer exercised by this pin; \
         pick a new pinning glyph or remove this assertion.",
    );
}
