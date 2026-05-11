//! Pre-parsed Adobe Core 14 PDF font metrics.
//!
//! The 14 PostScript faces every PDF 1.7-conformant viewer ships
//! built-in — Helvetica × 4, Times × 4, Courier × 4, Symbol,
//! `ZapfDingbats` — exposed as `&'static FontMetrics<'static>` constants
//! that cost nothing at runtime. The AFM files are vendored from
//! [`tecnickcom/tc-font-core14-afms`] under `data/`, parsed by the
//! sibling [`afm`] crate at build time (see `build.rs`), and baked
//! into Rust statics in `$OUT_DIR/baked.rs`.
//!
//! [`tecnickcom/tc-font-core14-afms`]: https://github.com/tecnickcom/tc-font-core14-afms
//! [`afm`]: https://crates.io/crates/afm
//!
//! # Quick start
//!
//! ```
//! use pdf_base14_metrics::Base14Font;
//!
//! // Look up a glyph width by PostScript name.
//! assert_eq!(Base14Font::Helvetica.glyph_width("A"), Some(667.0));
//!
//! // Or via PDF `WinAnsiEncoding` byte (Latin faces only).
//! assert_eq!(Base14Font::Helvetica.winansi_width(b'A'), Some(667.0));
//!
//! // Iterate every Core 14 face in stable order.
//! for f in Base14Font::ALL {
//!     let m = f.metrics();
//!     assert!(!m.character_metrics.is_empty());
//! }
//! ```
//!
//! # Encoding caveat — Symbol and `ZapfDingbats`
//!
//! [`Base14Font::winansi_width`] returns `None` for [`Base14Font::Symbol`]
//! and [`Base14Font::ZapfDingbats`]: those fonts use their own
//! PostScript encodings (Greek/math operators and named dingbats
//! respectively), not `WinAnsi`. Querying them through a Latin-1 byte
//! would be a category error — the byte `0x41` is `"A"` in `WinAnsi`
//! but `"Alpha"` in Symbol. Callers must reach for the per-glyph
//! [`Base14Font::glyph_width`] API for those two fonts.
//!
//! # License
//!
//! The crate's Rust source is MIT. The 14 vendored AFM files in
//! `data/` ship under Adobe's permissive Core 14 AFM license — see
//! `LICENSE-Adobe-Core14-AFM` in the crate root. The combined SPDX
//! expression is `MIT AND LicenseRef-Adobe-Core14-AFM`.

#![deny(missing_docs)]

pub use afm::{BBox, CharacterMetric, FontMetrics, KerningPair};

use std::borrow::Cow;

mod winansi_char_map;
mod winansi_table;

// The generated file references `BBox`, `CharacterMetric`,
// `FontMetrics`, `KerningPair`, and `Cow` unqualified — all are in
// scope via the `pub use` and `use` above.
include!(concat!(env!("OUT_DIR"), "/baked.rs"));

/// One of the 14 standard PDF fonts every conformant PDF reader
/// ships built in (PDF 1.7 §9.6.2.2).
///
/// Variants are listed in the canonical PDF order: the four
/// Helvetica weights, four Times weights, four Courier weights,
/// then Symbol and `ZapfDingbats`. [`Self::ALL`] iterates them in
/// this order.
#[derive(Copy, Clone, Eq, PartialEq, Debug, Hash)]
pub enum Base14Font {
    /// Helvetica (regular).
    Helvetica,
    /// Helvetica Bold.
    HelveticaBold,
    /// Helvetica Oblique (regular weight, slanted).
    HelveticaOblique,
    /// Helvetica Bold Oblique.
    HelveticaBoldOblique,
    /// Times Roman (regular).
    TimesRoman,
    /// Times Bold.
    TimesBold,
    /// Times Italic.
    TimesItalic,
    /// Times Bold Italic.
    TimesBoldItalic,
    /// Courier (regular, monospace).
    Courier,
    /// Courier Bold (monospace).
    CourierBold,
    /// Courier Oblique (monospace, slanted).
    CourierOblique,
    /// Courier Bold Oblique (monospace).
    CourierBoldOblique,
    /// Adobe Symbol (Greek letters, math operators).
    Symbol,
    /// ITC Zapf Dingbats (decorative glyphs).
    ZapfDingbats,
}

impl Base14Font {
    /// Every Core 14 face in stable PDF order.
    pub const ALL: [Self; 14] = [
        Self::Helvetica,
        Self::HelveticaBold,
        Self::HelveticaOblique,
        Self::HelveticaBoldOblique,
        Self::TimesRoman,
        Self::TimesBold,
        Self::TimesItalic,
        Self::TimesBoldItalic,
        Self::Courier,
        Self::CourierBold,
        Self::CourierOblique,
        Self::CourierBoldOblique,
        Self::Symbol,
        Self::ZapfDingbats,
    ];

    /// Borrows the pre-parsed Adobe AFM metrics for this face.
    #[must_use]
    pub fn metrics(self) -> &'static FontMetrics<'static> {
        match self {
            Self::Helvetica => &HELVETICA,
            Self::HelveticaBold => &HELVETICA_BOLD,
            Self::HelveticaOblique => &HELVETICA_OBLIQUE,
            Self::HelveticaBoldOblique => &HELVETICA_BOLDOBLIQUE,
            Self::TimesRoman => &TIMES_ROMAN,
            Self::TimesBold => &TIMES_BOLD,
            Self::TimesItalic => &TIMES_ITALIC,
            Self::TimesBoldItalic => &TIMES_BOLDITALIC,
            Self::Courier => &COURIER,
            Self::CourierBold => &COURIER_BOLD,
            Self::CourierOblique => &COURIER_OBLIQUE,
            Self::CourierBoldOblique => &COURIER_BOLDOBLIQUE,
            Self::Symbol => &SYMBOL,
            Self::ZapfDingbats => &ZAPFDINGBATS,
        }
    }

    /// PDF `/BaseFont` name per PDF 1.7 §9.6.2.2. These are the
    /// exact bytes a conformant PDF writer puts after `/BaseFont`
    /// in a font resource dictionary.
    #[must_use]
    pub fn pdf_base_name(self) -> &'static str {
        match self {
            Self::Helvetica => "Helvetica",
            Self::HelveticaBold => "Helvetica-Bold",
            Self::HelveticaOblique => "Helvetica-Oblique",
            Self::HelveticaBoldOblique => "Helvetica-BoldOblique",
            Self::TimesRoman => "Times-Roman",
            Self::TimesBold => "Times-Bold",
            Self::TimesItalic => "Times-Italic",
            Self::TimesBoldItalic => "Times-BoldItalic",
            Self::Courier => "Courier",
            Self::CourierBold => "Courier-Bold",
            Self::CourierOblique => "Courier-Oblique",
            Self::CourierBoldOblique => "Courier-BoldOblique",
            Self::Symbol => "Symbol",
            Self::ZapfDingbats => "ZapfDingbats",
        }
    }

    /// Width of the glyph with the given PostScript name, in 1/1000
    /// em. Returns `None` if no such glyph exists in this font.
    ///
    /// This is an O(n) linear scan over the font's character metrics
    /// (~315 entries for the Latin faces). Prefer
    /// [`Self::winansi_width`] when querying by byte — that path
    /// goes through a pre-baked O(1) table.
    #[must_use]
    pub fn glyph_width(self, name: &str) -> Option<f32> {
        self.metrics()
            .character_metrics
            .iter()
            .find(|c| c.name == name)
            .map(|c| c.width_x)
    }

    /// Width of the glyph at PDF `WinAnsiEncoding` byte `code`, in
    /// 1/1000 em. Returns `None` when:
    ///
    /// - `code` is unmapped by PDF `WinAnsi` (control characters
    ///   `0x00..=0x1F`, the gaps `0x7F` / `0x81` / `0x8D` / `0x8F`
    ///   / `0x90` / `0x9D`); or
    /// - `self` is [`Self::Symbol`] or [`Self::ZapfDingbats`] —
    ///   those fonts do not use `WinAnsi` (see the crate-level docs).
    ///
    /// Implemented as a single `[Option<f32>; 256]` indexed load
    /// per call: the table is baked at build time alongside the
    /// font metrics. Hot enough for `mosaic-fonts::text_width` to
    /// call once per character per typeset paragraph.
    #[must_use]
    pub fn winansi_width(self, code: u8) -> Option<f32> {
        self.winansi_table().and_then(|t| t[code as usize])
    }

    /// The pre-baked `WinAnsi` width table, or `None` for fonts whose
    /// canonical encoding isn't `WinAnsi`.
    fn winansi_table(self) -> Option<&'static [Option<f32>; 256]> {
        match self {
            Self::Symbol | Self::ZapfDingbats => None,
            Self::Helvetica => Some(&HELVETICA_WINANSI),
            Self::HelveticaBold => Some(&HELVETICA_BOLD_WINANSI),
            Self::HelveticaOblique => Some(&HELVETICA_OBLIQUE_WINANSI),
            Self::HelveticaBoldOblique => Some(&HELVETICA_BOLDOBLIQUE_WINANSI),
            Self::TimesRoman => Some(&TIMES_ROMAN_WINANSI),
            Self::TimesBold => Some(&TIMES_BOLD_WINANSI),
            Self::TimesItalic => Some(&TIMES_ITALIC_WINANSI),
            Self::TimesBoldItalic => Some(&TIMES_BOLDITALIC_WINANSI),
            Self::Courier => Some(&COURIER_WINANSI),
            Self::CourierBold => Some(&COURIER_BOLD_WINANSI),
            Self::CourierOblique => Some(&COURIER_OBLIQUE_WINANSI),
            Self::CourierBoldOblique => Some(&COURIER_BOLDOBLIQUE_WINANSI),
        }
    }
}

/// Returns the PostScript glyph name assigned to PDF `WinAnsiEncoding`
/// byte `code`, or `None` for unmapped codes.
///
/// PDF `WinAnsi` is **not** Microsoft CP1252 — see PDF 1.7 Annex D.2
/// for the canonical table. The two encodings differ at codes
/// `0x7F`, `0x81`, `0x8D`, `0x8F`, `0x90`, and `0x9D` (gaps in PDF,
/// assorted glyphs or DEL in CP1252).
///
/// This is exposed primarily so downstream crates (e.g.
/// `mosaic-fonts`) can delegate to the canonical table rather than
/// maintain their own copy.
#[must_use]
pub fn winansi_glyph_name(code: u8) -> Option<&'static str> {
    winansi_table::WINANSI_TABLE[code as usize]
}

/// Returns the PDF `WinAnsiEncoding` byte that encodes `ch`, or
/// `None` if `ch` has no slot in `WinAnsi`.
///
/// The inverse of the byte→char mapping baked into
/// `WINANSI_CHAR_MAP` at build time from `WINANSI_TABLE`
/// (PDF 1.7 Annex D.2 Table D.2) cross-referenced with the
/// [Adobe Glyph List]. Returns `None` for:
///
/// - Characters that have no glyph in `WinAnsi` (Cyrillic, CJK,
///   most accented Vietnamese, etc.).
/// - The six `WinAnsi` gap bytes (`0x7F`, `0x81`, `0x8D`, `0x8F`,
///   `0x90`, `0x9D`).
///
/// O(n) scan over 256 slots — fine for callers that touch it once
/// per text run, sensible to memoize for hotter paths.
///
/// [Adobe Glyph List]: https://github.com/adobe-type-tools/agl-aglfn
#[must_use]
pub fn winansi_byte(ch: char) -> Option<u8> {
    WINANSI_CHAR_MAP
        .iter()
        .position(|&c| c == Some(ch))
        .and_then(|i| u8::try_from(i).ok())
}

// Test-only visibility shims for `tests/winansi_vendor.rs`. Both
// constants are `#[doc(hidden)]` so they don't leak into the public
// docs, and live here only so an integration test can prove the
// hand-curated table in `src/winansi_char_map.rs` matches the
// AGL-derived oracle baked at build time. If/when the AGL build path
// is retired, drop `__WINANSI_CHAR_MAP_AGL` along with the resolver.
#[doc(hidden)]
pub const __WINANSI_CHAR_MAP_AGL: [Option<char>; 256] = WINANSI_CHAR_MAP;
#[doc(hidden)]
pub const __WINANSI_CHAR_MAP_LITERAL: [Option<char>; 256] =
    winansi_char_map::WINANSI_CHAR_MAP_LITERAL;
