// PDF WinAnsiEncoding byte → PostScript glyph name mapping.
//
// Source: PDF 1.7 Annex D.2, Table D.2 ("Latin Character Set and
// Encodings"), column "WIN". This is NOT Microsoft CP1252 — the two
// differ at codes 0x7F, 0x81, 0x8D, 0x8F, 0x90, 0x9D (gaps in PDF;
// assorted glyphs in CP1252). The PDF WinAnsi table is what every
// conformant PDF reader uses when a font's /Encoding is
// /WinAnsiEncoding, so it's what the Mosaic PDF backend must agree
// with.
//
// This file is shared with build.rs via `include!` and consumed by
// `src/lib.rs` via `mod winansi_table;` — single source of truth.
// Regular `//` comments only — inner doc comments (`//!`) would
// break the `include!` consumer because they'd land mid-file in
// build.rs's binary.
//
// Per PDF 1.7 Annex D.2 the encoding pins two aliasing rules:
//   - 0xA0 (non-breaking space) renders with the same glyph as 0x20
//     (`space`).
//   - 0xAD (soft hyphen) renders with the same glyph as 0x2D
//     (`hyphen`).
// Both aliases are represented by listing the same glyph name at
// both codes, so byte-indexed lookups Just Work.

// 256-entry table mapping each byte to its PostScript glyph name,
// or `None` for unmapped slots (control characters 0x00..=0x1F and
// the six WinAnsi gaps).
pub(crate) const WINANSI_TABLE: [Option<&str>; 256] = [
    // 0x00..=0x1F: C0 control characters — unmapped in PDF WinAnsi.
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None, // 0x00..=0x07
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None, // 0x08..=0x0F
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None, // 0x10..=0x17
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None, // 0x18..=0x1F
    // 0x20..=0x2F: punctuation and digits-precursor.
    Some("space"),       // 0x20
    Some("exclam"),      // 0x21
    Some("quotedbl"),    // 0x22
    Some("numbersign"),  // 0x23
    Some("dollar"),      // 0x24
    Some("percent"),     // 0x25
    Some("ampersand"),   // 0x26
    Some("quotesingle"), // 0x27  — PDF WinAnsi uses `quotesingle`, not `quoteright`
    Some("parenleft"),   // 0x28
    Some("parenright"),  // 0x29
    Some("asterisk"),    // 0x2A
    Some("plus"),        // 0x2B
    Some("comma"),       // 0x2C
    Some("hyphen"),      // 0x2D
    Some("period"),      // 0x2E
    Some("slash"),       // 0x2F
    // 0x30..=0x39: digits.
    Some("zero"),
    Some("one"),
    Some("two"),
    Some("three"),
    Some("four"),
    Some("five"),
    Some("six"),
    Some("seven"),
    Some("eight"),
    Some("nine"),
    // 0x3A..=0x40: punctuation.
    Some("colon"),     // 0x3A
    Some("semicolon"), // 0x3B
    Some("less"),      // 0x3C
    Some("equal"),     // 0x3D
    Some("greater"),   // 0x3E
    Some("question"),  // 0x3F
    Some("at"),        // 0x40
    // 0x41..=0x5A: uppercase A..Z.
    Some("A"),
    Some("B"),
    Some("C"),
    Some("D"),
    Some("E"),
    Some("F"),
    Some("G"),
    Some("H"),
    Some("I"),
    Some("J"),
    Some("K"),
    Some("L"),
    Some("M"),
    Some("N"),
    Some("O"),
    Some("P"),
    Some("Q"),
    Some("R"),
    Some("S"),
    Some("T"),
    Some("U"),
    Some("V"),
    Some("W"),
    Some("X"),
    Some("Y"),
    Some("Z"),
    // 0x5B..=0x60: punctuation.
    Some("bracketleft"),  // 0x5B
    Some("backslash"),    // 0x5C
    Some("bracketright"), // 0x5D
    Some("asciicircum"),  // 0x5E
    Some("underscore"),   // 0x5F
    Some("grave"),        // 0x60  — PDF WinAnsi: `grave`, not `quoteleft`
    // 0x61..=0x7A: lowercase a..z.
    Some("a"),
    Some("b"),
    Some("c"),
    Some("d"),
    Some("e"),
    Some("f"),
    Some("g"),
    Some("h"),
    Some("i"),
    Some("j"),
    Some("k"),
    Some("l"),
    Some("m"),
    Some("n"),
    Some("o"),
    Some("p"),
    Some("q"),
    Some("r"),
    Some("s"),
    Some("t"),
    Some("u"),
    Some("v"),
    Some("w"),
    Some("x"),
    Some("y"),
    Some("z"),
    // 0x7B..=0x7E: closing punctuation.
    Some("braceleft"),  // 0x7B
    Some("bar"),        // 0x7C
    Some("braceright"), // 0x7D
    Some("asciitilde"), // 0x7E
    // 0x7F: gap in PDF WinAnsi (CP1252 also leaves it as DEL).
    None,
    // 0x80..=0x9F: Windows-extended block.
    Some("Euro"),           // 0x80
    None,                   // 0x81  — gap (PDF WinAnsi differs from CP1252 here)
    Some("quotesinglbase"), // 0x82
    Some("florin"),         // 0x83
    Some("quotedblbase"),   // 0x84
    Some("ellipsis"),       // 0x85
    Some("dagger"),         // 0x86
    Some("daggerdbl"),      // 0x87
    Some("circumflex"),     // 0x88
    Some("perthousand"),    // 0x89
    Some("Scaron"),         // 0x8A
    Some("guilsinglleft"),  // 0x8B
    Some("OE"),             // 0x8C
    None,                   // 0x8D  — gap
    Some("Zcaron"),         // 0x8E
    None,                   // 0x8F  — gap
    None,                   // 0x90  — gap
    Some("quoteleft"),      // 0x91
    Some("quoteright"),     // 0x92
    Some("quotedblleft"),   // 0x93
    Some("quotedblright"),  // 0x94
    Some("bullet"),         // 0x95
    Some("endash"),         // 0x96
    Some("emdash"),         // 0x97
    Some("tilde"),          // 0x98
    Some("trademark"),      // 0x99
    Some("scaron"),         // 0x9A
    Some("guilsinglright"), // 0x9B
    Some("oe"),             // 0x9C
    None,                   // 0x9D  — gap
    Some("zcaron"),         // 0x9E
    Some("Ydieresis"),      // 0x9F
    // 0xA0..=0xAF: Latin-1 punctuation. 0xA0 aliases `space`; 0xAD aliases `hyphen`.
    Some("space"),         // 0xA0  — alias of 0x20 per PDF 1.7 Annex D.2
    Some("exclamdown"),    // 0xA1
    Some("cent"),          // 0xA2
    Some("sterling"),      // 0xA3
    Some("currency"),      // 0xA4
    Some("yen"),           // 0xA5
    Some("brokenbar"),     // 0xA6
    Some("section"),       // 0xA7
    Some("dieresis"),      // 0xA8
    Some("copyright"),     // 0xA9
    Some("ordfeminine"),   // 0xAA
    Some("guillemotleft"), // 0xAB
    Some("logicalnot"),    // 0xAC
    Some("hyphen"),        // 0xAD  — alias of 0x2D per PDF 1.7 Annex D.2
    Some("registered"),    // 0xAE
    Some("macron"),        // 0xAF
    // 0xB0..=0xBF.
    Some("degree"),         // 0xB0
    Some("plusminus"),      // 0xB1
    Some("twosuperior"),    // 0xB2
    Some("threesuperior"),  // 0xB3
    Some("acute"),          // 0xB4
    Some("mu"),             // 0xB5
    Some("paragraph"),      // 0xB6
    Some("periodcentered"), // 0xB7
    Some("cedilla"),        // 0xB8
    Some("onesuperior"),    // 0xB9
    Some("ordmasculine"),   // 0xBA
    Some("guillemotright"), // 0xBB
    Some("onequarter"),     // 0xBC
    Some("onehalf"),        // 0xBD
    Some("threequarters"),  // 0xBE
    Some("questiondown"),   // 0xBF
    // 0xC0..=0xDF: uppercase accented Latin.
    Some("Agrave"),
    Some("Aacute"),
    Some("Acircumflex"),
    Some("Atilde"),
    Some("Adieresis"),
    Some("Aring"),
    Some("AE"),
    Some("Ccedilla"),
    Some("Egrave"),
    Some("Eacute"),
    Some("Ecircumflex"),
    Some("Edieresis"),
    Some("Igrave"),
    Some("Iacute"),
    Some("Icircumflex"),
    Some("Idieresis"),
    Some("Eth"),
    Some("Ntilde"),
    Some("Ograve"),
    Some("Oacute"),
    Some("Ocircumflex"),
    Some("Otilde"),
    Some("Odieresis"),
    Some("multiply"),
    Some("Oslash"),
    Some("Ugrave"),
    Some("Uacute"),
    Some("Ucircumflex"),
    Some("Udieresis"),
    Some("Yacute"),
    Some("Thorn"),
    Some("germandbls"),
    // 0xE0..=0xFF: lowercase accented Latin.
    Some("agrave"),
    Some("aacute"),
    Some("acircumflex"),
    Some("atilde"),
    Some("adieresis"),
    Some("aring"),
    Some("ae"),
    Some("ccedilla"),
    Some("egrave"),
    Some("eacute"),
    Some("ecircumflex"),
    Some("edieresis"),
    Some("igrave"),
    Some("iacute"),
    Some("icircumflex"),
    Some("idieresis"),
    Some("eth"),
    Some("ntilde"),
    Some("ograve"),
    Some("oacute"),
    Some("ocircumflex"),
    Some("otilde"),
    Some("odieresis"),
    Some("divide"),
    Some("oslash"),
    Some("ugrave"),
    Some("uacute"),
    Some("ucircumflex"),
    Some("udieresis"),
    Some("yacute"),
    Some("thorn"),
    Some("ydieresis"),
];
