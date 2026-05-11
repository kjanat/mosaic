//! Embedded TrueType faces + shaping.
//!
//! [`EmbeddedFont`] holds a bundled TTF's bytes plus a pre-parsed
//! `rustybuzz::Face` and the FontDescriptor-relevant metrics the PDF
//! emit path needs. [`shape`] runs `rustybuzz` over a UTF-8 string and
//! returns a [`ShapedGlyph`] stream. [`subset`] reduces a face to just
//! the glyph IDs used in one document and returns the trimmed bytes
//! suitable for a `/FontFile2` stream.

use rustybuzz::{Face, UnicodeBuffer};

/// One glyph in a shaped run. Cluster values are byte offsets into the
/// source UTF-8 string. Offsets are in font units (1/`units_per_em` of
/// the em square) and are zero for non-positioning shaping (most LTR
/// Latin) but non-zero for marks (Cyrillic accent stacks, Vietnamese
/// tone marks).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct ShapedGlyph {
    /// Glyph ID into the source font. Becomes the CID in the emitted
    /// PDF (we use `/CIDToGIDMap /Identity`).
    pub gid: u16,
    /// Horizontal advance, in font units.
    pub advance_units: i32,
    /// Horizontal offset to apply before drawing this glyph, in font
    /// units. Does not affect the line advance. Non-zero for marks.
    pub x_offset_units: i32,
    /// Vertical offset to apply before drawing this glyph, in font
    /// units. Does not affect the line advance.
    pub y_offset_units: i32,
    /// Byte offset of this glyph's grapheme cluster in the source
    /// string. Monotonically non-decreasing across a LTR run.
    pub cluster: u32,
}

/// A bundled `TrueType` face: the raw bytes plus the metrics and
/// parsed `rustybuzz::Face` needed to shape text and emit a PDF
/// `FontDescriptor`.
///
/// Created via [`Self::from_static`] from a `&'static [u8]` (the
/// bundled `include_bytes!`-loaded TTF). The crate's user-facing
/// surface is the [`crate::EmbeddedFontId`] enum; this struct is the
/// per-cut data block those ids resolve through.
pub struct EmbeddedFont {
    /// Raw TTF bytes. Held statically so the parsed `Face<'static>`
    /// can borrow them.
    pub bytes: &'static [u8],
    /// `HarfBuzz`/`rustybuzz` face. Borrows `bytes`.
    pub face: Face<'static>,
    /// Pre-parsed `ttf-parser` face. The PDF backend reads
    /// `FontDescriptor` fields (italic angle, bbox, …) through this;
    /// `rustybuzz` wraps it but doesn't re-expose every getter.
    pub ttf: ttf_parser::Face<'static>,
    /// `PostScript` name (from the `name` table, ID 6). Becomes the
    /// `/BaseFont` entry's suffix after the six-letter subset tag.
    pub postscript_name: &'static str,
    /// `head.unitsPerEm`. Typically 1000 (CFF) or a power of two for
    /// `TrueType` outlines.
    pub units_per_em: u16,
    /// `hhea.ascender` (font units).
    pub ascender: i16,
    /// `hhea.descender` (font units, typically negative).
    pub descender: i16,
    /// `OS/2.sCapHeight` if present, else `ascender * 7 / 10` as a
    /// PDF-conventional fallback.
    pub cap_height: i16,
    /// `OS/2.sxHeight` if present, else `ascender * 1 / 2` as a
    /// fallback.
    pub x_height: i16,
    /// `post.italicAngle` in degrees, negated to match PDF convention
    /// (PDF expects negative for italic slanted right).
    pub italic_angle: f32,
    /// `head` font bounding box (xMin, yMin, xMax, yMax). Becomes
    /// `FontDescriptor` `/FontBBox`.
    pub bbox: (i16, i16, i16, i16),
    /// Heuristic stem-vertical width for `/StemV`: 80 for regular,
    /// 120 for bold. `ttf-parser` doesn't surface a reliable `StemV`;
    /// most fonts don't ship it in `OS/2`. PDF validators accept the
    /// heuristic.
    pub stem_v: i16,
    /// PDF `FontDescriptor` `/Flags`. Nonsymbolic (bit 6, value 32) for
    /// Latin/Cyrillic/Greek fonts; the italic bit (bit 7, value 64)
    /// is OR'd in for italic cuts.
    pub flags: u32,
}

impl std::fmt::Debug for EmbeddedFont {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EmbeddedFont")
            .field("postscript_name", &self.postscript_name)
            .field("units_per_em", &self.units_per_em)
            .field("ascender", &self.ascender)
            .field("descender", &self.descender)
            .field("italic_angle", &self.italic_angle)
            .field("bbox", &self.bbox)
            .finish()
    }
}

impl EmbeddedFont {
    /// Parse a bundled TTF blob into an [`EmbeddedFont`]. The blob
    /// must outlive the program (which it does — bundled cuts come
    /// from `include_bytes!` and are baked into the binary).
    ///
    /// `postscript_name`, `is_bold`, and `is_italic` are provided by
    /// the caller rather than read from the `name` table because the
    /// bundled cuts are known statics and parse-time string ownership
    /// would require allocating; the `name` table also ships
    /// platform-specific encodings we don't want to navigate.
    ///
    /// # Panics
    ///
    /// Panics if the bytes don't parse as a `TrueType` font. The four
    /// bundled cuts have been parse-verified at vendor time and are
    /// re-verified by `tests/parse_bundled.rs` on every CI run, so
    /// reaching this panic requires post-build corruption (e.g. a
    /// failed LFS pull or a truncated binary). Threading
    /// `Result`/`Option` through the dozens of downstream call sites
    /// to handle a case the compile-time `include_bytes!` already
    /// rules out would make the code materially worse — the lint
    /// suppression is the explicit CLAUDE.md exception, paired with
    /// the CI test that catches the only realistic failure mode.
    #[must_use]
    #[allow(
        clippy::expect_used,
        reason = "bundled bytes are include_bytes!-baked and CI-verified by \
                  tests/parse_bundled.rs; propagating Option would force every \
                  downstream caller fallible for an unreachable path"
    )]
    pub fn from_static(
        bytes: &'static [u8],
        postscript_name: &'static str,
        is_bold: bool,
        is_italic: bool,
    ) -> Self {
        let ttf = ttf_parser::Face::parse(bytes, 0)
            .expect("bundled font bytes failed to parse as TrueType — repository corruption?");
        let face = Face::from_face(ttf.clone());

        let units_per_em = ttf.units_per_em();
        let ascender = ttf.ascender();
        let descender = ttf.descender();
        let cap_height = ttf.capital_height().map_or(ascender * 7 / 10, i16::from);
        let x_height = ttf.x_height().map_or(ascender / 2, i16::from);
        let italic_angle = -ttf.italic_angle();
        let global_bbox = ttf.global_bounding_box();
        let bbox = (
            global_bbox.x_min,
            global_bbox.y_min,
            global_bbox.x_max,
            global_bbox.y_max,
        );

        // PDF FontDescriptor flag bits (PDF 1.7 §9.8.2 Table 123):
        //   bit 6 (value 32)  Nonsymbolic — character set is standard
        //                     Adobe-Latin (covers extended Latin and
        //                     anything Unicode-addressable that doesn't
        //                     deliberately use a symbol encoding).
        //   bit 7 (value 64)  Italic.
        // The Symbolic bit (bit 3, value 4) is mutually exclusive with
        // Nonsymbolic and only applies to faces like Symbol/Dingbats.
        let mut flags: u32 = 0x20;
        if is_italic {
            flags |= 0x40;
        }

        let stem_v: i16 = if is_bold { 120 } else { 80 };

        Self {
            bytes,
            face,
            ttf,
            postscript_name,
            units_per_em,
            ascender,
            descender,
            cap_height,
            x_height,
            italic_angle,
            bbox,
            stem_v,
            flags,
        }
    }

    /// Look up the GID for a Unicode codepoint, if the face covers it.
    /// Used by the layout engine's `glyph_width` shortcut when shaping
    /// a single codepoint would be wasteful.
    #[must_use]
    pub fn glyph_index(&self, ch: char) -> Option<u16> {
        self.ttf.glyph_index(ch).map(|g| g.0)
    }

    /// Horizontal advance for `gid` in font units, sourced from the
    /// `hmtx` table.
    #[must_use]
    pub fn advance_units(&self, gid: u16) -> u16 {
        self.ttf
            .glyph_hor_advance(ttf_parser::GlyphId(gid))
            .unwrap_or(0)
    }
}

/// Shape `text` against `font` using `rustybuzz`. Returns the glyph
/// stream in visual order (LTR for this slice). An empty `text`
/// returns an empty `Vec` without invoking the shaper.
#[must_use]
pub fn shape(font: &EmbeddedFont, text: &str) -> Vec<ShapedGlyph> {
    if text.is_empty() {
        return Vec::new();
    }
    let mut buffer = UnicodeBuffer::new();
    buffer.push_str(text);
    // We let rustybuzz infer script and language from the buffer's
    // codepoints; the slice is LTR-only so we force horizontal LTR
    // explicitly to avoid the inference picking RTL for an Arabic
    // word the user typed.
    buffer.set_direction(rustybuzz::Direction::LeftToRight);
    buffer.guess_segment_properties();
    // Force LTR back: guess_segment_properties may flip direction
    // based on script. This slice is LTR-only by scope.
    buffer.set_direction(rustybuzz::Direction::LeftToRight);
    let glyph_buffer = rustybuzz::shape(&font.face, &[], buffer);
    let infos = glyph_buffer.glyph_infos();
    let positions = glyph_buffer.glyph_positions();
    let mut out = Vec::with_capacity(infos.len());
    for (info, pos) in infos.iter().zip(positions.iter()) {
        // rustybuzz documents `glyph_id` as `<= u16::MAX`; the cast is
        // truncation-safe per that contract. Guard with a `try_from`
        // anyway so a future rustybuzz drift surfaces as gid 0
        // (rendered as `.notdef`) rather than silent wrap.
        let gid = u16::try_from(info.glyph_id).unwrap_or(0);
        out.push(ShapedGlyph {
            gid,
            advance_units: pos.x_advance,
            x_offset_units: pos.x_offset,
            y_offset_units: pos.y_offset,
            cluster: info.cluster,
        });
    }
    out
}

/// Subset `font` to just the glyph IDs in `gids` (always include GID 0,
/// `.notdef`, which the PDF spec mandates). Returns the trimmed TTF
/// bytes suitable for embedding as a `/FontFile2` stream.
///
/// # Errors
///
/// Returns an error if the font's tables are malformed or use features
/// the underlying [`subsetter`] crate doesn't support (CFF2). The
/// bundled Noto Sans cuts are TrueType-flavoured and exercise the
/// well-supported path.
pub fn subset(font: &EmbeddedFont, gids: &[u16]) -> Result<Vec<u8>, SubsetError> {
    let mut all = Vec::with_capacity(gids.len() + 1);
    all.push(0_u16);
    all.extend_from_slice(gids);
    let remapper = subsetter::GlyphRemapper::new_from_glyphs(&all);
    subsetter::subset(font.bytes, 0, &remapper).map_err(SubsetError)
}

/// Wraps [`subsetter::Error`] without exposing the dependency in the
/// public API. The PDF emit path bails on this error with a `Diagnostic`.
#[derive(Debug)]
pub struct SubsetError(pub subsetter::Error);

impl std::fmt::Display for SubsetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "font subsetting failed: {:?}", self.0)
    }
}

impl std::error::Error for SubsetError {}
