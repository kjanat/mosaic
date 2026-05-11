// PDF `WinAnsiEncoding` byte → Unicode `char` mapping, transcribed
// directly from PDF 1.7 Annex D.2 Table D.2 (column "WIN"). This is
// the source of truth scanned by `winansi_byte` to find the
// `WinAnsi` byte for a Unicode `char`.
//
// Why a hand-written table rather than deriving from the Adobe Glyph
// List at build time: the AGL data is BSD-3-Clause and would force
// that leg onto the crate's SPDX expression. Transcribing the 256
// slots from PDF 1.7 — a normative spec, not someone else's data —
// keeps the published artifact MIT + APAFML only. The
// `winansi_vendor` integration test re-derives the same map from AGL
// at test time and asserts byte-for-byte equality, so any
// transcription error here is caught by CI before it can ship.
//
// This file is `mod`-included by `src/lib.rs` only. It is deliberately
// NOT pulled into `build.rs` (unlike its sibling `winansi_table.rs`)
// because the build script doesn't need it — keeping it out of build.rs
// avoids a `dead_code` warning in the build-script binary.
//
// Per PDF 1.7 Annex D.2 the encoding pins two aliasing rules:
//   - 0xA0 (non-breaking space) renders with the `space` glyph
//     ⇒ Unicode U+0020 (regular ASCII space, not U+00A0 NBSP).
//   - 0xAD (soft hyphen) renders with the `hyphen` glyph
//     ⇒ Unicode U+002D (regular ASCII hyphen-minus, not U+00AD SHY).
// This matches how PDF readers actually paint these bytes; it is NOT
// the same as Latin-1 / CP1252 round-tripping.

pub(crate) const WINANSI_CHAR_MAP: [Option<char>; 256] = [
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
    // 0x20..=0x7E: printable ASCII (identity mapping).
    Some(' '),
    Some('!'),
    Some('"'),
    Some('#'), // 0x20..=0x23
    Some('$'),
    Some('%'),
    Some('&'),
    Some('\''), // 0x24..=0x27
    Some('('),
    Some(')'),
    Some('*'),
    Some('+'), // 0x28..=0x2B
    Some(','),
    Some('-'),
    Some('.'),
    Some('/'), // 0x2C..=0x2F
    Some('0'),
    Some('1'),
    Some('2'),
    Some('3'), // 0x30..=0x33
    Some('4'),
    Some('5'),
    Some('6'),
    Some('7'), // 0x34..=0x37
    Some('8'),
    Some('9'),
    Some(':'),
    Some(';'), // 0x38..=0x3B
    Some('<'),
    Some('='),
    Some('>'),
    Some('?'), // 0x3C..=0x3F
    Some('@'),
    Some('A'),
    Some('B'),
    Some('C'), // 0x40..=0x43
    Some('D'),
    Some('E'),
    Some('F'),
    Some('G'), // 0x44..=0x47
    Some('H'),
    Some('I'),
    Some('J'),
    Some('K'), // 0x48..=0x4B
    Some('L'),
    Some('M'),
    Some('N'),
    Some('O'), // 0x4C..=0x4F
    Some('P'),
    Some('Q'),
    Some('R'),
    Some('S'), // 0x50..=0x53
    Some('T'),
    Some('U'),
    Some('V'),
    Some('W'), // 0x54..=0x57
    Some('X'),
    Some('Y'),
    Some('Z'),
    Some('['), // 0x58..=0x5B
    Some('\\'),
    Some(']'),
    Some('^'),
    Some('_'), // 0x5C..=0x5F
    Some('`'),
    Some('a'),
    Some('b'),
    Some('c'), // 0x60..=0x63
    Some('d'),
    Some('e'),
    Some('f'),
    Some('g'), // 0x64..=0x67
    Some('h'),
    Some('i'),
    Some('j'),
    Some('k'), // 0x68..=0x6B
    Some('l'),
    Some('m'),
    Some('n'),
    Some('o'), // 0x6C..=0x6F
    Some('p'),
    Some('q'),
    Some('r'),
    Some('s'), // 0x70..=0x73
    Some('t'),
    Some('u'),
    Some('v'),
    Some('w'), // 0x74..=0x77
    Some('x'),
    Some('y'),
    Some('z'),
    Some('{'), // 0x78..=0x7B
    Some('|'),
    Some('}'),
    Some('~'), // 0x7C..=0x7E
    None,      // 0x7F unassigned
    // 0x80..=0x9F: Windows-1252 extensions (with WinAnsi-specific gaps).
    Some('\u{20AC}'), // 0x80 Euro
    None,             // 0x81 unassigned
    Some('\u{201A}'), // 0x82 quotesinglbase
    Some('\u{0192}'), // 0x83 florin
    Some('\u{201E}'), // 0x84 quotedblbase
    Some('\u{2026}'), // 0x85 ellipsis
    Some('\u{2020}'), // 0x86 dagger
    Some('\u{2021}'), // 0x87 daggerdbl
    Some('\u{02C6}'), // 0x88 circumflex
    Some('\u{2030}'), // 0x89 perthousand
    Some('\u{0160}'), // 0x8A Scaron
    Some('\u{2039}'), // 0x8B guilsinglleft
    Some('\u{0152}'), // 0x8C OE
    None,             // 0x8D unassigned
    Some('\u{017D}'), // 0x8E Zcaron
    None,             // 0x8F unassigned
    None,             // 0x90 unassigned
    Some('\u{2018}'), // 0x91 quoteleft
    Some('\u{2019}'), // 0x92 quoteright
    Some('\u{201C}'), // 0x93 quotedblleft
    Some('\u{201D}'), // 0x94 quotedblright
    Some('\u{2022}'), // 0x95 bullet
    Some('\u{2013}'), // 0x96 endash
    Some('\u{2014}'), // 0x97 emdash
    Some('\u{02DC}'), // 0x98 tilde
    Some('\u{2122}'), // 0x99 trademark
    Some('\u{0161}'), // 0x9A scaron
    Some('\u{203A}'), // 0x9B guilsinglright
    Some('\u{0153}'), // 0x9C oe
    None,             // 0x9D unassigned
    Some('\u{017E}'), // 0x9E zcaron
    Some('\u{0178}'), // 0x9F Ydieresis
    // 0xA0..=0xAF: Latin-1 punctuation. 0xA0 → space, 0xAD → hyphen.
    Some(' '),        // 0xA0 nbspace → space glyph (U+0020)
    Some('\u{00A1}'), // 0xA1 exclamdown
    Some('\u{00A2}'), // 0xA2 cent
    Some('\u{00A3}'), // 0xA3 sterling
    Some('\u{00A4}'), // 0xA4 currency
    Some('\u{00A5}'), // 0xA5 yen
    Some('\u{00A6}'), // 0xA6 brokenbar
    Some('\u{00A7}'), // 0xA7 section
    Some('\u{00A8}'), // 0xA8 dieresis
    Some('\u{00A9}'), // 0xA9 copyright
    Some('\u{00AA}'), // 0xAA ordfeminine
    Some('\u{00AB}'), // 0xAB guillemotleft
    Some('\u{00AC}'), // 0xAC logicalnot
    Some('-'),        // 0xAD sfthyphen → hyphen glyph (U+002D)
    Some('\u{00AE}'), // 0xAE registered
    Some('\u{00AF}'), // 0xAF macron
    // 0xB0..=0xFF: Latin-1 supplement (identity with U+00B0..=U+00FF).
    Some('\u{00B0}'),
    Some('\u{00B1}'),
    Some('\u{00B2}'),
    Some('\u{00B3}'), // 0xB0..=0xB3
    Some('\u{00B4}'),
    Some('\u{00B5}'),
    Some('\u{00B6}'),
    Some('\u{00B7}'), // 0xB4..=0xB7
    Some('\u{00B8}'),
    Some('\u{00B9}'),
    Some('\u{00BA}'),
    Some('\u{00BB}'), // 0xB8..=0xBB
    Some('\u{00BC}'),
    Some('\u{00BD}'),
    Some('\u{00BE}'),
    Some('\u{00BF}'), // 0xBC..=0xBF
    Some('\u{00C0}'),
    Some('\u{00C1}'),
    Some('\u{00C2}'),
    Some('\u{00C3}'), // 0xC0..=0xC3
    Some('\u{00C4}'),
    Some('\u{00C5}'),
    Some('\u{00C6}'),
    Some('\u{00C7}'), // 0xC4..=0xC7
    Some('\u{00C8}'),
    Some('\u{00C9}'),
    Some('\u{00CA}'),
    Some('\u{00CB}'), // 0xC8..=0xCB
    Some('\u{00CC}'),
    Some('\u{00CD}'),
    Some('\u{00CE}'),
    Some('\u{00CF}'), // 0xCC..=0xCF
    Some('\u{00D0}'),
    Some('\u{00D1}'),
    Some('\u{00D2}'),
    Some('\u{00D3}'), // 0xD0..=0xD3
    Some('\u{00D4}'),
    Some('\u{00D5}'),
    Some('\u{00D6}'),
    Some('\u{00D7}'), // 0xD4..=0xD7
    Some('\u{00D8}'),
    Some('\u{00D9}'),
    Some('\u{00DA}'),
    Some('\u{00DB}'), // 0xD8..=0xDB
    Some('\u{00DC}'),
    Some('\u{00DD}'),
    Some('\u{00DE}'),
    Some('\u{00DF}'), // 0xDC..=0xDF
    Some('\u{00E0}'),
    Some('\u{00E1}'),
    Some('\u{00E2}'),
    Some('\u{00E3}'), // 0xE0..=0xE3
    Some('\u{00E4}'),
    Some('\u{00E5}'),
    Some('\u{00E6}'),
    Some('\u{00E7}'), // 0xE4..=0xE7
    Some('\u{00E8}'),
    Some('\u{00E9}'),
    Some('\u{00EA}'),
    Some('\u{00EB}'), // 0xE8..=0xEB
    Some('\u{00EC}'),
    Some('\u{00ED}'),
    Some('\u{00EE}'),
    Some('\u{00EF}'), // 0xEC..=0xEF
    Some('\u{00F0}'),
    Some('\u{00F1}'),
    Some('\u{00F2}'),
    Some('\u{00F3}'), // 0xF0..=0xF3
    Some('\u{00F4}'),
    Some('\u{00F5}'),
    Some('\u{00F6}'),
    Some('\u{00F7}'), // 0xF4..=0xF7
    Some('\u{00F8}'),
    Some('\u{00F9}'),
    Some('\u{00FA}'),
    Some('\u{00FB}'), // 0xF8..=0xFB
    Some('\u{00FC}'),
    Some('\u{00FD}'),
    Some('\u{00FE}'),
    Some('\u{00FF}'), // 0xFC..=0xFF
];
