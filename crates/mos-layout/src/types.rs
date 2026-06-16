use std::collections::BTreeMap;
use std::sync::Arc;

use mos_core::Diagnostic;
use mos_fonts::{Font, FontFamily, ShapedGlyph};

/// A4 page width in PDF points (1pt = 1/72 inch). Kept as a public
/// constant so external callers can still read the default; the layout
/// engine now consults `PageStyle` instead of these directly.
pub const A4_WIDTH_PT: f32 = 595.276;
/// A4 page height in PDF points.
pub const A4_HEIGHT_PT: f32 = 841.890;
/// Default page margin in points (24mm × 72/25.4).
pub const MARGIN_PT: f32 = 68.031;

/// Default body font size (manifest §22.1).
pub(crate) const BODY_SIZE_PT: f32 = 11.0;
/// Default body leading multiplier (line height = size × leading).
pub(crate) const BODY_LEADING: f32 = 1.35;

/// Page geometry resolved from `#set page(...)`. `width_pt`/`height_pt`
/// describe the full media box; `margin_pt` is symmetric on all four
/// sides for MVP 1.5 (per-side margins are deferred).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PageStyle {
    pub width_pt: f32,
    pub height_pt: f32,
    pub margin_pt: f32,
}

impl Default for PageStyle {
    fn default() -> Self {
        Self {
            width_pt: A4_WIDTH_PT,
            height_pt: A4_HEIGHT_PT,
            margin_pt: MARGIN_PT,
        }
    }
}

/// Body text style resolved from `#set text(...)`. `leading` applies
/// to body paragraphs only; headings keep their own multiplier so a
/// `#set text(leading: 2.0)` doesn't balloon section titles.
///
/// `family` is the resolved [`FontFamily`] from `#set text(font: ...)`.
/// Headings use the family's bold cut; `*emphasis*` uses italic;
/// `` `raw` `` uses monospace; everything else is `family.regular`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextStyle {
    pub size_pt: f32,
    pub leading: f32,
    pub family: FontFamily,
}

impl Default for TextStyle {
    fn default() -> Self {
        Self {
            size_pt: BODY_SIZE_PT,
            leading: BODY_LEADING,
            family: FontFamily::noto_sans(),
        }
    }
}

/// Decoded raster image data shared between every page that places
/// the same source image. Held by [`Arc`] so a single PNG referenced
/// from multiple `#image(...)` directives shares one buffer end-to-end.
#[derive(Clone, Debug)]
pub struct ImageHandle {
    /// Stable index assigned by the layout engine. The PDF backend
    /// uses it as the suffix on the `/Im<n>` resource-dict key and the
    /// `XObject`'s indirect ref allocation order, so callers don't have
    /// to hash the path themselves.
    pub id: u32,
    /// Resolved absolute path from the lowerer. Used as the dedup key
    /// across multiple `#image(...)` calls with the same source.
    pub resolved_path: String,
    /// Decoded pixel width.
    pub pixel_width: u32,
    /// Decoded pixel height.
    pub pixel_height: u32,
    /// Flat RGB8 pixel buffer (`3 * pixel_width * pixel_height` bytes).
    /// Shared via `Arc<[u8]>` so cloning a handle (e.g. when the same
    /// image is referenced from multiple `#image`/`#figure` directives,
    /// or when a future caching layer hands the document back) is cheap.
    /// The eval crate hands ownership over as an `Arc<[u8]>` already
    /// (see `AttrValue::Bytes`), so the slice never gets copied here.
    pub rgb8: Arc<[u8]>,
}

/// One image placement on a page. The PDF backend emits this as a
/// `q ... cm /Im<id> Do Q` block in the content stream.
#[derive(Clone, Debug)]
pub struct ImagePlacement {
    pub handle: ImageHandle,
    /// X coordinate of the image's left edge, measured from the page's
    /// left edge in points.
    pub x_pt: f32,
    /// Y coordinate of the image's **top** edge, measured from the page's
    /// **top** edge in points. The PDF backend flips to bottom-origin
    /// once when emitting (same convention as [`TextRun`]).
    pub top_from_top_pt: f32,
    /// Rendered width in points.
    pub width_pt: f32,
    /// Rendered height in points.
    pub height_pt: f32,
}

/// A single horizontal run of text on a page. The MVP 0 emitter
/// produces one run per word; coalescing same-font neighbours is an
/// MVP 2 optimisation.
#[derive(Clone, Debug)]
pub struct TextRun {
    /// X coordinate of the run's left edge, measured from the page's
    /// left edge in points.
    pub x_pt: f32,
    /// Y coordinate of the run's baseline, measured from the page's
    /// **top** edge in points. The PDF backend flips to bottom-origin
    /// once when emitting.
    pub baseline_from_top_pt: f32,
    /// Font size in points.
    pub size_pt: f32,
    /// Font face for this run.
    pub font: Font,
    /// Original UTF-8 text. Used by the PDF backend's `/ToUnicode`
    /// `CMap` (so copy-paste from the rendered PDF round-trips back to
    /// the source) and by the Base14 emit path (which encodes through
    /// `WinAnsiEncoding` + per-document `/Differences`).
    pub text: String,
    /// Optional semantic replacement text for PDF `/ActualText`.
    /// Raw blocks use this when the painted text must differ from
    /// source text, e.g. tabs painted as spaces but copied as tabs.
    pub actual_text: Option<String>,
    /// Shaped glyph stream for embedded-font runs. Empty for Base14
    /// runs, which emit through the byte-encoded `WinAnsi` path
    /// instead.
    pub glyphs: Vec<ShapedGlyph>,
}

/// One laid-out page.
#[derive(Clone, Debug)]
pub struct Page {
    pub number: u32,
    pub width_pt: f32,
    pub height_pt: f32,
    pub runs: Vec<TextRun>,
    /// Raster image placements on this page. Stored as a separate
    /// vector so PDF emit can walk every placement without filtering
    /// the text-run stream; the two streams are independent.
    pub images: Vec<ImagePlacement>,
}

/// The paginated output graph (manifest §6 stage 7).
#[derive(Clone, Debug, Default)]
pub struct PageGraph {
    pub pages: Vec<Page>,
    /// Master list of every unique image referenced anywhere in
    /// `pages`, ordered by [`ImageHandle::id`]. The PDF backend walks
    /// this once to emit `XObject`s; per-page [`ImagePlacement::handle`]
    /// references are just thin pointers into the same table.
    pub images: Vec<ImageHandle>,
}

/// Result of laying out a [`mos_core::Document`]: a [`PageGraph`] plus
/// any warnings the engine emitted. Mirrors `mos_eval::LowerResult` so
/// the CLI can render diagnostics uniformly.
#[derive(Debug)]
pub struct LayoutResult {
    pub graph: PageGraph,
    pub diagnostics: Vec<Diagnostic>,
    /// Map from a declared label to the 1-based number of the page its
    /// target first lands on (issue #72). Built during layout as each
    /// labelled block commits its first content; first placement wins, so a
    /// label on a block that spans pages maps to its *start* page, and a
    /// labelled block that produced no content is absent.
    ///
    /// This is the layout-side half of page-reference resolution: the
    /// resolve↔layout fixpoint feeds this map back into the resolver so
    /// `@page(label)` can render the target's printed page number. It lives on
    /// the result rather than the [`PageGraph`] because it feeds the resolver,
    /// not the PDF backend, which consumes only `graph`.
    pub label_pages: BTreeMap<String, u32>,
}
