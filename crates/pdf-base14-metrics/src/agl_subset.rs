// Hand-curated Unicode → PostScript glyph name table.
//
// Scope: the 99 glyph names that appear in the Adobe Core 14 Latin
// AFMs (Helvetica/Times/Courier × 4) but have no PDF
// `WinAnsiEncoding` byte. These are the glyphs reachable through a
// custom `/Encoding /Differences` array on a Core 14 font: Latin
// Extended-A in full, the Polish/Czech/Hungarian/Romanian/Turkish
// long tail, the `fi`/`fl` ligatures, the math operators
// (−, ≤, ≥, ≠, √, ∂, ∑, ∆, ◊), and the spacing diacritics
// (˘ ˇ ˙ ˝ ˛ ˚).
//
// Encoded as a `&[(char, &'static str)]` sorted by `char` so callers
// can `binary_search_by_key`. The names are exactly those in the
// Adobe AFM (e.g. `Tcommaaccent`, not `Tcedilla`): what the PDF
// reader looks up to find the glyph outline.
//
// The Romanian comma-below codepoints `U+0218`..`U+021B` and the
// historical cedilla-below variants `U+015E`/`U+015F`/`U+0162`/`U+0163`
// both resolve to the same glyph names in the AFM
// (`Scommaaccent`/`scommaaccent`/`Tcommaaccent`/`tcommaaccent`,
// `Scedilla`/`scedilla` only for S: Helvetica's AFM has both
// `Scedilla` and `Scommaaccent` as distinct glyphs but only one
// `Tcommaaccent`). The mapping below picks the AGLFN canonical name
// for each codepoint.
//
// `WinAnsi` natives (`á`, `ß`, `€`, `“`, ...) are NOT in this table:
// look them up through `winansi_byte` instead.
//
// Source: the Helvetica.adobe-font-metrics `CharSet` minus the 216 names in
// `WINANSI_TABLE`, cross-referenced with the Adobe Glyph List for New
// Fonts (AGLFN). License-clean: the 99 entries below are derivative
// of the vendored AFM `CharSet` (Adobe APAFML) plus public PDF/Unicode
// standards, not a reproduction of the AGL data file.

// (char, AFM glyph name). Sorted by `char` for binary search.
const AGL_SUBSET: &[(char, &str)] = &[
    // Latin Extended-A
    ('\u{0100}', "Amacron"),
    ('\u{0101}', "amacron"),
    ('\u{0102}', "Abreve"),
    ('\u{0103}', "abreve"),
    ('\u{0104}', "Aogonek"),
    ('\u{0105}', "aogonek"),
    ('\u{0106}', "Cacute"),
    ('\u{0107}', "cacute"),
    ('\u{010C}', "Ccaron"),
    ('\u{010D}', "ccaron"),
    ('\u{010E}', "Dcaron"),
    ('\u{010F}', "dcaron"),
    ('\u{0110}', "Dcroat"),
    ('\u{0111}', "dcroat"),
    ('\u{0112}', "Emacron"),
    ('\u{0113}', "emacron"),
    ('\u{0116}', "Edotaccent"),
    ('\u{0117}', "edotaccent"),
    ('\u{0118}', "Eogonek"),
    ('\u{0119}', "eogonek"),
    ('\u{011A}', "Ecaron"),
    ('\u{011B}', "ecaron"),
    ('\u{011E}', "Gbreve"),
    ('\u{011F}', "gbreve"),
    ('\u{0122}', "Gcommaaccent"),
    ('\u{0123}', "gcommaaccent"),
    ('\u{012A}', "Imacron"),
    ('\u{012B}', "imacron"),
    ('\u{012E}', "Iogonek"),
    ('\u{012F}', "iogonek"),
    ('\u{0130}', "Idotaccent"),
    ('\u{0131}', "dotlessi"),
    ('\u{0136}', "Kcommaaccent"),
    ('\u{0137}', "kcommaaccent"),
    ('\u{0139}', "Lacute"),
    ('\u{013A}', "lacute"),
    ('\u{013B}', "Lcommaaccent"),
    ('\u{013C}', "lcommaaccent"),
    ('\u{013D}', "Lcaron"),
    ('\u{013E}', "lcaron"),
    ('\u{0141}', "Lslash"),
    ('\u{0142}', "lslash"),
    ('\u{0143}', "Nacute"),
    ('\u{0144}', "nacute"),
    ('\u{0145}', "Ncommaaccent"),
    ('\u{0146}', "ncommaaccent"),
    ('\u{0147}', "Ncaron"),
    ('\u{0148}', "ncaron"),
    ('\u{014C}', "Omacron"),
    ('\u{014D}', "omacron"),
    ('\u{0150}', "Ohungarumlaut"),
    ('\u{0151}', "ohungarumlaut"),
    ('\u{0154}', "Racute"),
    ('\u{0155}', "racute"),
    ('\u{0156}', "Rcommaaccent"),
    ('\u{0157}', "rcommaaccent"),
    ('\u{0158}', "Rcaron"),
    ('\u{0159}', "rcaron"),
    ('\u{015A}', "Sacute"),
    ('\u{015B}', "sacute"),
    ('\u{015E}', "Scedilla"),
    ('\u{015F}', "scedilla"),
    // U+0162 / U+0163: historical T-cedilla codepoints. The AFM only
    // ships `Tcommaaccent` (no `Tcedilla`); modern Romanian uses the
    // comma-below codepoints U+021A/U+021B below, but Unicode data
    // shipped before ~2000 commonly stores ţ as U+0163, so we route
    // both to the same glyph.
    ('\u{0162}', "Tcommaaccent"),
    ('\u{0163}', "tcommaaccent"),
    ('\u{0164}', "Tcaron"),
    ('\u{0165}', "tcaron"),
    ('\u{016A}', "Umacron"),
    ('\u{016B}', "umacron"),
    ('\u{016E}', "Uring"),
    ('\u{016F}', "uring"),
    ('\u{0170}', "Uhungarumlaut"),
    ('\u{0171}', "uhungarumlaut"),
    ('\u{0172}', "Uogonek"),
    ('\u{0173}', "uogonek"),
    ('\u{0179}', "Zacute"),
    ('\u{017A}', "zacute"),
    ('\u{017B}', "Zdotaccent"),
    ('\u{017C}', "zdotaccent"),
    // (`Zcaron`/`zcaron` are NOT here: they live in WinAnsi at 0x8E/0x9E.)
    // Latin Extended-B (Romanian comma-below: modern canonical).
    ('\u{0218}', "Scommaaccent"),
    ('\u{0219}', "scommaaccent"),
    ('\u{021A}', "Tcommaaccent"),
    ('\u{021B}', "tcommaaccent"),
    // Spacing modifier letters (PDF AFMs name these as plain spacing
    // accents: caron, breve, dotaccent, hungarumlaut, ogonek, ring).
    ('\u{02C7}', "caron"),
    ('\u{02D8}', "breve"),
    ('\u{02D9}', "dotaccent"),
    ('\u{02DA}', "ring"),
    ('\u{02DB}', "ogonek"),
    ('\u{02DD}', "hungarumlaut"),
    // Fraction slash (U+2044), math (− ≤ ≥ ≠ √ ∂ ∑ ∆), lozenge (◊),
    // and ligatures. Order matters: this slice is binary-searched.
    ('\u{2044}', "fraction"),
    ('\u{2202}', "partialdiff"),
    ('\u{2206}', "Delta"),
    ('\u{2211}', "summation"),
    ('\u{2212}', "minus"),
    ('\u{221A}', "radical"),
    ('\u{2260}', "notequal"),
    ('\u{2264}', "lessequal"),
    ('\u{2265}', "greaterequal"),
    ('\u{25CA}', "lozenge"),
    ('\u{FB01}', "fi"),
    ('\u{FB02}', "fl"),
];

/// Return the non-`WinAnsi` Core 14 glyph name for `ch`.
///
/// Returns `None` for `WinAnsi` natives and for codepoints with no glyph in any
/// Core 14 font.
#[must_use]
pub fn agl_glyph_name(ch: char) -> Option<&'static str> {
    AGL_SUBSET
        .binary_search_by_key(&ch, |&(c, _)| c)
        .ok()
        .map(|i| AGL_SUBSET[i].1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_is_sorted_by_char() {
        for w in AGL_SUBSET.windows(2) {
            assert!(
                w[0].0 < w[1].0,
                "AGL_SUBSET out of order: {:?} >= {:?}",
                w[0],
                w[1]
            );
        }
    }

    #[test]
    fn polish_lslash_resolves() {
        assert_eq!(agl_glyph_name('\u{0141}'), Some("Lslash"));
        assert_eq!(agl_glyph_name('\u{0142}'), Some("lslash"));
    }

    #[test]
    fn czech_caron_glyphs_resolve() {
        assert_eq!(agl_glyph_name('\u{011B}'), Some("ecaron"));
        assert_eq!(agl_glyph_name('\u{0159}'), Some("rcaron"));
        assert_eq!(agl_glyph_name('\u{010F}'), Some("dcaron"));
    }

    #[test]
    fn romanian_comma_below_resolves_to_commaaccent() {
        assert_eq!(agl_glyph_name('\u{0219}'), Some("scommaaccent"));
        assert_eq!(agl_glyph_name('\u{021B}'), Some("tcommaaccent"));
    }

    #[test]
    fn ligatures_resolve() {
        assert_eq!(agl_glyph_name('\u{FB01}'), Some("fi"));
        assert_eq!(agl_glyph_name('\u{FB02}'), Some("fl"));
    }

    #[test]
    fn winansi_native_returns_none() {
        // 'A' (U+0041) is in WinAnsi at byte 0x41; not our table.
        assert_eq!(agl_glyph_name('A'), None);
        // 'é' (U+00E9) is in WinAnsi at 0xE9.
        assert_eq!(agl_glyph_name('é'), None);
        // 'ž' (U+017E) IS in WinAnsi at 0x9E.
        assert_eq!(agl_glyph_name('ž'), None);
    }

    #[test]
    fn cjk_and_cyrillic_return_none() {
        assert_eq!(agl_glyph_name('П'), None);
        assert_eq!(agl_glyph_name('日'), None);
    }
}
