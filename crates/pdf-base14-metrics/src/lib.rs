//! Pre-parsed Adobe Core 14 PDF font metrics.
//!
//! The 14 PostScript faces every PDF 1.7-conformant viewer ships
//! built-in — Helvetica × 4, Times × 4, Courier × 4, Symbol,
//! `ZapfDingbats` — exposed as `&'static FontMetrics<'static>` constants
//! that cost nothing at runtime. The AFM files are vendored from
//! [`tecnickcom/tc-font-core14-afms`] under `data/`, parsed by the
//! sibling [`adobe-font-metrics`] crate at build time (see `build.rs`),
//! and baked into Rust statics in `$OUT_DIR/baked.rs`.
//!

//! [`tecnickcom/tc-font-core14-afms`]: https://github.com/tecnickcom/tc-font-core14-afms
//! [`adobe-font-metrics`]: https://crates.io/crates/adobe-font-metrics
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
//! `data/afm/` ship under Adobe's permissive Core 14 AFM license
//! (`APAFML`) — see `LICENSE-APAFML` in the crate root. The combined
//! SPDX expression is `MIT AND APAFML`.

#![deny(missing_docs)]

pub use adobe_font_metrics::{BBox, CharacterMetric, FontMetrics, KerningPair};

use std::borrow::Cow;

mod agl_subset;
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
    /// goes through a pre-baked O(1) table. For the Latin Core 12
    /// faces, [`Self::glyph_width_by_name`] goes through a baked
    /// sorted index instead and is O(log n).
    #[must_use]
    pub fn glyph_width(self, name: &str) -> Option<f32> {
        self.metrics()
            .character_metrics
            .iter()
            .find(|c| c.name == name)
            .map(|c| c.width_x)
    }

    /// Width of the glyph with the given PostScript name, looked up
    /// through a baked sorted index. O(log n), allocation-free,
    /// safe to call once per character per PDF page in tight loops.
    ///
    /// Returns `None` for [`Self::Symbol`] and [`Self::ZapfDingbats`]
    /// — their AFMs are intentionally unindexed because those faces
    /// don't participate in `/Differences`-style remapping. Callers
    /// that need Symbol/Dingbat widths must use [`Self::glyph_width`].
    #[must_use]
    pub fn glyph_width_by_name(self, name: &str) -> Option<f32> {
        let table = self.name_width_table()?;
        table
            .binary_search_by(|(n, _)| (*n).cmp(name))
            .ok()
            .map(|i| table[i].1)
    }

    /// Returns the baked `(name, width)` index for Latin Core 12
    /// faces, or `None` for `Symbol`/`ZapfDingbats`.
    fn name_width_table(self) -> Option<&'static [(&'static str, f32)]> {
        match self {
            Self::Symbol | Self::ZapfDingbats => None,
            Self::Helvetica => Some(HELVETICA_NAME_WIDTHS),
            Self::HelveticaBold => Some(HELVETICA_BOLD_NAME_WIDTHS),
            Self::HelveticaOblique => Some(HELVETICA_OBLIQUE_NAME_WIDTHS),
            Self::HelveticaBoldOblique => Some(HELVETICA_BOLDOBLIQUE_NAME_WIDTHS),
            Self::TimesRoman => Some(TIMES_ROMAN_NAME_WIDTHS),
            Self::TimesBold => Some(TIMES_BOLD_NAME_WIDTHS),
            Self::TimesItalic => Some(TIMES_ITALIC_NAME_WIDTHS),
            Self::TimesBoldItalic => Some(TIMES_BOLDITALIC_NAME_WIDTHS),
            Self::Courier => Some(COURIER_NAME_WIDTHS),
            Self::CourierBold => Some(COURIER_BOLD_NAME_WIDTHS),
            Self::CourierOblique => Some(COURIER_OBLIQUE_NAME_WIDTHS),
            Self::CourierBoldOblique => Some(COURIER_BOLDOBLIQUE_NAME_WIDTHS),
        }
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
    /// font metrics. Hot enough for `mos-fonts::text_width` to
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
/// `mos-fonts`) can delegate to the canonical table rather than
/// maintain their own copy.
#[must_use]
pub fn winansi_glyph_name(code: u8) -> Option<&'static str> {
    winansi_table::WINANSI_TABLE[code as usize]
}

/// Returns the PDF `WinAnsiEncoding` byte that encodes `ch`, or
/// `None` if `ch` has no slot in `WinAnsi`.
///
/// The inverse of the byte→char mapping transcribed from
/// PDF 1.7 Annex D.2 Table D.2 into
/// `winansi_char_map::WINANSI_CHAR_MAP`. Returns `None` for:
///
/// - Characters that have no glyph in `WinAnsi` (Cyrillic, CJK,
///   most accented Vietnamese, etc.).
/// - The six `WinAnsi` gap bytes (`0x7F`, `0x81`, `0x8D`, `0x8F`,
///   `0x90`, `0x9D`).
///
/// O(n) scan over 256 slots — fine for callers that touch it once
/// per text run, sensible to memoize for hotter paths.
#[must_use]
pub fn winansi_byte(ch: char) -> Option<u8> {
    winansi_char_map::WINANSI_CHAR_MAP
        .iter()
        .position(|&c| c == Some(ch))
        .and_then(|i| u8::try_from(i).ok())
}

// Test-only visibility shim for `tests/winansi_vendor.rs`. The const
// is `#[doc(hidden)]` so it doesn't leak into the public API surface,
// and lives here only so the integration test can re-derive the same
// map from the Adobe Glyph List at test runtime and assert
// byte-for-byte equality.
#[doc(hidden)]
pub const __WINANSI_CHAR_MAP: [Option<char>; 256] = winansi_char_map::WINANSI_CHAR_MAP;

/// Returns the PostScript glyph name for `ch` *if and only if* `ch`
/// is in the **extended** tier — i.e. a Core 14 AFM glyph that has
/// no `WinAnsi` byte and therefore must be reached through a custom
/// `/Encoding` `/Differences` slot. The extended tier covers:
///
/// - most of Latin Extended-A (`Ł`, `ł`, `Ě`, `ě`, `Ő`, `ő`, …,
///   excluding those that already live in `WinAnsi` like
///   `š`/`Š`/`ž`/`Ž`);
/// - the Latin Extended-B comma-below set `Ș`/`ș`/`Ț`/`ț`;
/// - the spacing diacritics `˘ˇ˙˝˛˚`;
/// - the math operators `−≤≥≠√∂∑∆◊`;
/// - the `fraction` slash `⁄` and the `fi`/`fl` ligatures.
///
/// Returns `None` for **two distinct cases that callers must
/// distinguish**:
///
/// 1. **`WinAnsi` natives** — `š` (U+0161), `ž` (U+017E), `Š`, `Ž`,
///    the accented Latin-1 alphabet, `€`, `“`, ... These *do* have
///    PostScript glyph names in the AFM, but this function returns
///    `None` for them because they're reachable through
///    [`winansi_byte`] instead and don't need a `/Differences` slot.
///    Callers querying "what's the AFM glyph name for `é`?" should
///    use [`Base14Font::glyph_width_by_name`] on the result of
///    [`winansi_glyph_name`]`(`[`winansi_byte`]`(ch)?)`, or just
///    measure widths through [`Base14Font::winansi_width`].
/// 2. **Unmappable codepoints** with no glyph in any Core 14 font
///    (Cyrillic, CJK, emoji, most non-European scripts). The PDF
///    backend silently substitutes these to `?` for Base14 runs;
///    real coverage requires the bundled embedded family that
///    `mos-fonts` provides.
///
/// The name `extended_glyph_name` is deliberately chosen over the
/// shorter `glyph_name` to avoid surprising readers who reach for
/// the function expecting "AFM name for any char." For *any-tier*
/// AFM lookup the two-step (`winansi_glyph_name` ∘ `winansi_byte`)
/// then-fallback-to-`extended_glyph_name` composition is the way.
///
/// Used by the PDF backend's `/Differences`-based encoding planner
/// to allocate slots for the extended tier.
#[must_use]
pub fn extended_glyph_name(ch: char) -> Option<&'static str> {
    agl_subset::agl_glyph_name(ch)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glyph_width_by_name_matches_linear_scan_for_every_helvetica_glyph() {
        let face = Base14Font::Helvetica;
        for c in face.metrics().character_metrics.iter() {
            let by_name = face.glyph_width_by_name(c.name.as_ref());
            assert_eq!(
                by_name,
                Some(c.width_x),
                "by-name mismatch for {:?}",
                c.name
            );
        }
    }

    #[test]
    fn glyph_width_by_name_resolves_non_winansi_glyphs() {
        // Helvetica.adobe-font-metrics:  C -1 ; WX 222 ; N lslash ; ...  (well, lslash
        // is actually encoded at C 248 in AdobeStandardEncoding, but
        // either way the width is the same.) The PDF spec lets us
        // address it through /Differences.
        let face = Base14Font::Helvetica;
        assert_eq!(face.glyph_width_by_name("lslash"), Some(222.0));
        assert_eq!(face.glyph_width_by_name("Lslash"), Some(556.0));
        assert_eq!(face.glyph_width_by_name("ecaron"), Some(556.0));
        assert_eq!(face.glyph_width_by_name("rcaron"), Some(333.0));
    }

    #[test]
    fn glyph_width_by_name_returns_none_for_unknown_glyph() {
        assert_eq!(Base14Font::Helvetica.glyph_width_by_name(""), None);
        assert_eq!(
            Base14Font::Helvetica.glyph_width_by_name("notarealglyph"),
            None
        );
    }

    #[test]
    fn glyph_width_by_name_returns_none_for_symbol_and_dingbats() {
        // Documented contract: those faces don't participate in
        // /Differences-based remapping.
        assert_eq!(Base14Font::Symbol.glyph_width_by_name("A"), None);
        assert_eq!(Base14Font::ZapfDingbats.glyph_width_by_name("A"), None);
    }

    #[test]
    fn courier_carries_the_same_extended_glyph_set_as_helvetica() {
        // The 12 Latin Core 14 faces share an identical 315-name glyph
        // inventory (verified by `diff` on the AFM CharSets); the
        // planner can rely on "if Helvetica has it, Courier does too"
        // when deciding whether to remap a slot.
        for name in &["lslash", "ecaron", "tcommaaccent", "ohungarumlaut"] {
            assert!(
                Base14Font::Courier.glyph_width_by_name(name).is_some(),
                "Courier missing {name}"
            );
        }
    }

    #[test]
    fn extended_glyph_name_resolves_polish_and_czech() {
        assert_eq!(extended_glyph_name('ł'), Some("lslash"));
        assert_eq!(extended_glyph_name('Ł'), Some("Lslash"));
        assert_eq!(extended_glyph_name('ě'), Some("ecaron"));
        // ž is a WinAnsi native, not in the extended tier — by
        // contract `extended_glyph_name` returns `None` even though
        // the AFM does carry a `zcaron` glyph (reachable through
        // `winansi_byte` / `winansi_glyph_name` instead).
        assert_eq!(extended_glyph_name('ž'), None);
        // 'A' is also a WinAnsi native and returns None.
        assert_eq!(extended_glyph_name('A'), None);
    }
}
