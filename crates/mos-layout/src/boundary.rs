//! Page boundary signatures (issue #70; design note
//! `docs/incremental-dependencies.md` §4.5, §6, §7).
//!
//! Reflow and fixpoint work (manifest §33) needs to detect *where* pagination
//! changed between two layout runs without re-diffing whole pages or
//! serializing PDF output. A [`PageBoundarySignature`] is a compact, stable
//! digest of one laid-out [`Page`]'s break-defining content; a
//! [`PageGraphSignature`] is the ordered per-page list, so the first index
//! where two graphs disagree is exactly where the page breaks diverged.
//!
//! This is the §4.5 `PageOutputHash` ("did the laid-out page actually change?")
//! reduced to the layout primitives that exist today (text runs and image
//! placements). It is identity/comparison only: no `DepNode` graph, no
//! `CacheKey` wiring, no reflow loop; those consume these signatures later.
//!
//! # What feeds a signature
//!
//! Per page, in order: the page number, the quantized page box, then each text
//! run (quantized position + size, a backend-neutral font identity, text) and
//! each image placement (intrinsic pixel dimensions + quantized rectangle). Run
//! and image counts are folded too, so adding or removing one shifts the digest.
//!
//! Deliberately **excluded**, per the determinism rules (§5) and the §4.2/§4.3
//! carve-outs:
//!
//! - **Shaped glyphs** on a run: derived from text + font + shaper, so folding
//!   them would bind the signature to a transcoder version it should not care
//!   about. The authored text and font identity stand in for them.
//! - **The PDF resource name** of a font (`F1`..): a backend emitter slot;
//!   layout must not depend on it, so a backend-neutral font identity is folded
//!   instead (see `font_identity`).
//! - **Decoded image pixels** (`rgb8`): an asset-content concern (§4.3),
//!   addressed by the asset's own hash, not the page boundary.
//! - **`resolved_path`** on an image handle: an absolute filesystem path, which
//!   §5 rule 1 forbids from any hash.
//! - **`handle.id`**: assigned in image-encounter order, so folding it would
//!   churn unrelated pages' signatures when an image is added earlier; the
//!   intrinsic pixel dimensions identify the asset's footprint instead.
//!
//! # Quantization
//!
//! Every `f32` dimension is snapped to the 1/64-pt grid (§6) before folding, so
//! two layouts that reach the same length through slightly different arithmetic
//! agree. The design note specifies an `i32` count of 1/64 pt; we snap to the
//! same grid and fold the canonical bit pattern of the integral count instead
//! (see [`quantize_units`]), which is equivalent for hashing and avoids a
//! lint-denied float-to-int cast. The §9.5 quantization newtype can later adopt
//! the `i32` form without changing this boundary's observable behavior.

use mos_core::{ContentHash, ContentHasher};
use mos_fonts::{Base14Font, EmbeddedFontId, Font};

use crate::types::{ImagePlacement, LayoutResult, Page, PageGraph, TextRun};

/// Domain tag separating this boundary from every other `H(...)` (§4).
const PAGE_DOMAIN: &[u8] = b"mos-layout/page-boundary/v1";

/// A backend-neutral, stable identity for a font face.
///
/// Deliberately *not* `Font::pdf_resource_name` (`F1`..): that is a PDF emitter
/// resource slot, and a layout signature must not depend on backend layout. The
/// returned tag is owned by this boundary and stable forever; the exhaustive
/// match means a new bundled face fails to compile here until it is assigned a
/// tag, so the identity can never silently alias.
fn font_identity(font: Font) -> &'static [u8] {
    match font {
        Font::Base14(Base14Font::Helvetica) => b"b14/helvetica",
        Font::Base14(Base14Font::HelveticaBold) => b"b14/helvetica-bold",
        Font::Base14(Base14Font::HelveticaOblique) => b"b14/helvetica-oblique",
        Font::Base14(Base14Font::HelveticaBoldOblique) => b"b14/helvetica-boldoblique",
        Font::Base14(Base14Font::TimesRoman) => b"b14/times-roman",
        Font::Base14(Base14Font::TimesBold) => b"b14/times-bold",
        Font::Base14(Base14Font::TimesItalic) => b"b14/times-italic",
        Font::Base14(Base14Font::TimesBoldItalic) => b"b14/times-bolditalic",
        Font::Base14(Base14Font::Courier) => b"b14/courier",
        Font::Base14(Base14Font::CourierBold) => b"b14/courier-bold",
        Font::Base14(Base14Font::CourierOblique) => b"b14/courier-oblique",
        Font::Base14(Base14Font::CourierBoldOblique) => b"b14/courier-boldoblique",
        Font::Base14(Base14Font::Symbol) => b"b14/symbol",
        Font::Base14(Base14Font::ZapfDingbats) => b"b14/zapfdingbats",
        Font::Embedded(EmbeddedFontId::Regular) => b"emb/noto-sans-regular",
        Font::Embedded(EmbeddedFontId::Bold) => b"emb/noto-sans-bold",
        Font::Embedded(EmbeddedFontId::Italic) => b"emb/noto-sans-italic",
        Font::Embedded(EmbeddedFontId::BoldItalic) => b"emb/noto-sans-bolditalic",
        Font::Embedded(EmbeddedFontId::Mono) => b"emb/noto-sans-mono",
        Font::Embedded(EmbeddedFontId::Math) => b"emb/noto-sans-math",
    }
}

/// Snap a point measurement to the 1/64-pt grid (§6) and return the canonical
/// bit pattern of that integral count.
///
/// Snapping first means two dimensions within one shaper unit fold equal.
/// Dividing is avoided entirely: `(pt * 64).round()` is already the integral
/// count, and while that count stays below `2^24` (i.e. `pt < 262144`, ~five
/// orders of magnitude above any real page coordinate) it is represented
/// exactly by `f32`, so its bit pattern is a canonical, collision-free encoding
/// of that integer. Past `2^24` consecutive counts would alias: out of the
/// domain layout produces, but the bound the `i32` form (§9.5) would lift.
/// `-0.0`, `NaN`, and non-finite values normalize to `0` so they cannot create
/// spurious distinctions.
fn quantize_units(pt: f32) -> u32 {
    let units = (pt * 64.0).round();
    let canonical = if units.is_finite() && units != 0.0 {
        units
    } else {
        0.0
    };
    canonical.to_bits()
}

/// The number of items folded as a count, saturating so an absurd length cannot
/// silently wrap.
fn fold_count(hasher: &mut ContentHasher, len: usize) {
    hasher.u32(u32::try_from(len).unwrap_or(u32::MAX));
}

fn fold_run(hasher: &mut ContentHasher, run: &TextRun) {
    hasher
        .u32(quantize_units(run.x_pt))
        .u32(quantize_units(run.baseline_from_top_pt))
        .u32(quantize_units(run.size_pt))
        .field(font_identity(run.font))
        .field(run.text.as_bytes());
    // Optional `/ActualText`: fold a presence flag so `Some("")` and `None`
    // stay distinct, then the bytes when present.
    match &run.actual_text {
        Some(actual) => {
            hasher.u32(1).field(actual.as_bytes());
        }
        None => {
            hasher.u32(0);
        }
    }
    // `glyphs` excluded: derived from text + font + shaper (§4.2 carve-out).
}

fn fold_image(hasher: &mut ContentHasher, image: &ImagePlacement) {
    hasher
        // Intrinsic pixel dimensions identify the asset's layout footprint.
        .u32(image.handle.pixel_width)
        .u32(image.handle.pixel_height)
        .u32(quantize_units(image.x_pt))
        .u32(quantize_units(image.top_from_top_pt))
        .u32(quantize_units(image.width_pt))
        .u32(quantize_units(image.height_pt));
    // `handle.id` is excluded: it is assigned in image-encounter order
    // (`intern_image` uses `image_handles.len()`), so folding it would shift
    // later images' signatures when an unrelated image is added earlier,
    // wrecking the page-locality `first_divergence` relies on. `handle.rgb8`
    // (asset content, §4.3) and `handle.resolved_path` (absolute path, §5
    // rule 1) are excluded too.
}

/// A compact, deterministic digest of one laid-out [`Page`]'s break-defining
/// content (design note §4.5).
///
/// Equal for identical pagination of identical content, different when a page's
/// content or its break changes. It is the cache slot's staleness check, not a
/// human-readable description: use [`content_hash`](Self::content_hash) to read
/// the underlying value.
///
/// # Examples
///
/// ```
/// use mos_layout::{LayoutEngine, PageBoundarySignature};
/// use mos_core::Document;
/// use std::path::PathBuf;
///
/// let doc = Document::new(PathBuf::from("doc.mos"));
/// let result = LayoutEngine::new().layout(&doc);
/// // Re-signing the same page yields the same signature.
/// for page in &result.graph.pages {
///     assert_eq!(
///         PageBoundarySignature::of_page(page),
///         PageBoundarySignature::of_page(page),
///     );
/// }
/// ```
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
pub struct PageBoundarySignature(ContentHash);

impl PageBoundarySignature {
    /// Compute the boundary signature of one laid-out page.
    #[must_use]
    pub fn of_page(page: &Page) -> Self {
        let mut hasher = ContentHasher::new();
        hasher
            .field(PAGE_DOMAIN)
            .u32(page.number)
            .u32(quantize_units(page.width_pt))
            .u32(quantize_units(page.height_pt));
        fold_count(&mut hasher, page.runs.len());
        for run in &page.runs {
            fold_run(&mut hasher, run);
        }
        fold_count(&mut hasher, page.images.len());
        for image in &page.images {
            fold_image(&mut hasher, image);
        }
        Self(hasher.finish())
    }

    /// The underlying content hash.
    #[must_use]
    pub const fn content_hash(self) -> ContentHash {
        self.0
    }
}

/// The ordered per-page boundary signatures of a whole [`PageGraph`].
///
/// Comparing two graph signatures answers both "did pagination change?"
/// (inequality) and "where?" ([`first_divergence`](Self::first_divergence)).
///
/// # Examples
///
/// ```
/// use mos_layout::{LayoutEngine, PageGraphSignature};
/// use mos_core::Document;
/// use std::path::PathBuf;
///
/// let doc = Document::new(PathBuf::from("doc.mos"));
/// let result = LayoutEngine::new().layout(&doc);
/// let signature = PageGraphSignature::of_graph(&result.graph);
/// // An unchanged layout signs identically and diverges nowhere.
/// assert_eq!(signature, PageGraphSignature::of_graph(&result.graph));
/// assert_eq!(signature.first_divergence(&signature), None);
/// ```
#[derive(Clone, Eq, PartialEq, Hash, Debug, Default)]
pub struct PageGraphSignature(Vec<PageBoundarySignature>);

impl PageGraphSignature {
    /// Sign every page of `graph`, in page order.
    #[must_use]
    pub fn of_graph(graph: &PageGraph) -> Self {
        Self(
            graph
                .pages
                .iter()
                .map(PageBoundarySignature::of_page)
                .collect(),
        )
    }

    /// The per-page signatures, in page order.
    #[must_use]
    pub fn pages(&self) -> &[PageBoundarySignature] {
        &self.0
    }

    /// The index of the first page whose signature differs from `other`, or
    /// [`None`] if the two are identical.
    ///
    /// When one graph is a prefix of the other (a page was added or removed at
    /// the end), the divergence is the length of the shorter graph; the first
    /// page index that exists in one but not the other.
    ///
    /// # Examples
    ///
    /// ```
    /// use mos_layout::PageGraphSignature;
    ///
    /// let empty = PageGraphSignature::default();
    /// assert_eq!(empty.first_divergence(&empty), None);
    /// ```
    #[must_use]
    pub fn first_divergence(&self, other: &Self) -> Option<usize> {
        self.0
            .iter()
            .zip(other.0.iter())
            .position(|(a, b)| a != b)
            .or_else(|| (self.0.len() != other.0.len()).then_some(self.0.len().min(other.0.len())))
    }
}

impl LayoutResult {
    /// The page boundary signatures of this layout's [`PageGraph`] (design note
    /// §4.5). Convenience for cache/reflow consumers.
    #[must_use]
    pub fn page_boundary_signatures(&self) -> PageGraphSignature {
        PageGraphSignature::of_graph(&self.graph)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use mos_fonts::{Base14Font, Font, ShapedGlyph};

    use super::{PageBoundarySignature, PageGraphSignature, quantize_units};
    use crate::types::{ImageHandle, ImagePlacement, Page, PageGraph, TextRun};

    fn run(text: &str, x_pt: f32) -> TextRun {
        TextRun {
            x_pt,
            baseline_from_top_pt: 100.0,
            size_pt: 11.0,
            font: Font::Base14(Base14Font::Helvetica),
            text: text.to_owned(),
            actual_text: None,
            glyphs: Vec::new(),
        }
    }

    fn page(number: u32, runs: Vec<TextRun>) -> Page {
        Page {
            number,
            width_pt: 595.276,
            height_pt: 841.89,
            runs,
            images: Vec::new(),
        }
    }

    fn graph(pages: Vec<Page>) -> PageGraph {
        PageGraph {
            pages,
            images: Vec::new(),
        }
    }

    #[test]
    fn unchanged_page_signs_identically() {
        let a = page(1, vec![run("hello", 68.0), run("world", 110.0)]);
        let b = page(1, vec![run("hello", 68.0), run("world", 110.0)]);
        assert_eq!(
            PageBoundarySignature::of_page(&a),
            PageBoundarySignature::of_page(&b),
        );
    }

    #[test]
    fn changed_content_changes_the_signature() {
        let base = page(1, vec![run("hello", 68.0)]);
        let edited_text = page(1, vec![run("hallo", 68.0)]);
        let moved = page(1, vec![run("hello", 70.0)]);
        let extra = page(1, vec![run("hello", 68.0), run("more", 68.0)]);

        let base_sig = PageBoundarySignature::of_page(&base);
        assert_ne!(base_sig, PageBoundarySignature::of_page(&edited_text));
        assert_ne!(base_sig, PageBoundarySignature::of_page(&moved));
        assert_ne!(base_sig, PageBoundarySignature::of_page(&extra));
    }

    #[test]
    fn page_number_is_folded() {
        // Identical content on a differently-numbered page must sign
        // differently; the page number is part of the boundary.
        let runs = || vec![run("same", 68.0)];
        assert_ne!(
            PageBoundarySignature::of_page(&page(1, runs())),
            PageBoundarySignature::of_page(&page(2, runs())),
        );
    }

    #[test]
    fn sub_shaper_unit_position_changes_are_ignored() {
        // Two positions within one 1/64-pt unit snap to the same grid cell.
        let base = page(1, vec![run("x", 68.0)]);
        let nudged = page(1, vec![run("x", 68.0 + 0.001)]);
        assert_eq!(
            PageBoundarySignature::of_page(&base),
            PageBoundarySignature::of_page(&nudged),
        );
    }

    #[test]
    fn quantize_snaps_to_one_sixty_fourth_point() {
        // Within a grid cell: equal. Across a cell boundary: distinct.
        assert_eq!(quantize_units(10.0), quantize_units(10.001));
        assert_ne!(quantize_units(10.0), quantize_units(10.5));
        // -0.0, +0.0, and non-finite values all normalize to the zero cell so
        // they never create spurious distinctions.
        assert_eq!(quantize_units(0.0), quantize_units(-0.0));
        assert_eq!(quantize_units(f32::NAN), quantize_units(0.0));
        assert_eq!(quantize_units(f32::INFINITY), quantize_units(0.0));
        assert_eq!(quantize_units(f32::NEG_INFINITY), quantize_units(0.0));
    }

    #[test]
    fn shaped_glyphs_do_not_affect_the_signature() {
        // Glyphs are derived from text + font, so two runs that differ only in
        // their shaped-glyph stream must sign the same.
        let plain = page(1, vec![run("hi", 68.0)]);
        let mut shaped_run = run("hi", 68.0);
        shaped_run.glyphs = vec![ShapedGlyph {
            gid: 42,
            advance_units: 500,
            x_offset_units: 0,
            y_offset_units: 0,
            cluster: 0,
        }];
        let shaped = page(1, vec![shaped_run]);
        assert_eq!(
            PageBoundarySignature::of_page(&plain),
            PageBoundarySignature::of_page(&shaped),
        );
    }

    #[test]
    fn image_content_path_and_unstable_id_are_excluded_but_footprint_is_not() {
        let place = |handle: ImageHandle| Page {
            number: 1,
            width_pt: 595.276,
            height_pt: 841.89,
            runs: Vec::new(),
            images: vec![ImagePlacement {
                handle,
                x_pt: 68.0,
                top_from_top_pt: 100.0,
                width_pt: 200.0,
                height_pt: 150.0,
            }],
        };
        let handle = |id: u32, path: &str, rgb8: &[u8]| ImageHandle {
            id,
            resolved_path: path.to_owned(),
            pixel_width: 2,
            pixel_height: 1,
            rgb8: Arc::from(rgb8.to_vec()),
        };
        // Different encounter-order id, different absolute path, and different
        // decoded bytes, but the same layout footprint: the signature must not
        // change. None of those belong in a deterministic, locality-preserving
        // page boundary.
        let a = place(handle(7, "/home/alice/fig.png", &[1, 2, 3, 4, 5, 6]));
        let b = place(handle(8, "/home/bob/fig.png", &[9, 9, 9, 9, 9, 9]));
        assert_eq!(
            PageBoundarySignature::of_page(&a),
            PageBoundarySignature::of_page(&b),
        );
        // The intrinsic pixel size *is* part of the footprint (the signature
        // reads dimensions, not the pixel buffer), so a differently-sized asset
        // signs differently.
        let mut resized = handle(7, "/home/alice/fig.png", &[1, 2, 3, 4, 5, 6]);
        resized.pixel_width = 4;
        assert_ne!(
            PageBoundarySignature::of_page(&a),
            PageBoundarySignature::of_page(&place(resized)),
        );
    }

    #[test]
    fn graph_signature_localizes_a_pagination_change() {
        // Page 1 unchanged; page 2 gains a run (a break moved). The graph
        // signatures must diverge first at index 1.
        let before = graph(vec![
            page(1, vec![run("a", 68.0)]),
            page(2, vec![run("b", 68.0)]),
        ]);
        let after = graph(vec![
            page(1, vec![run("a", 68.0)]),
            page(2, vec![run("b", 68.0), run("c", 110.0)]),
        ]);
        let before_sig = PageGraphSignature::of_graph(&before);
        let after_sig = PageGraphSignature::of_graph(&after);

        assert_ne!(before_sig, after_sig);
        assert_eq!(before_sig.first_divergence(&after_sig), Some(1));
        assert_eq!(before_sig.first_divergence(&before_sig), None);
    }

    #[test]
    fn graph_signature_flags_an_added_trailing_page() {
        let short = graph(vec![page(1, vec![run("a", 68.0)])]);
        let long = graph(vec![
            page(1, vec![run("a", 68.0)]),
            page(2, vec![run("b", 68.0)]),
        ]);
        let short_sig = PageGraphSignature::of_graph(&short);
        let long_sig = PageGraphSignature::of_graph(&long);
        // The shared page 0 matches; divergence is the new trailing index.
        assert_eq!(short_sig.first_divergence(&long_sig), Some(1));
        assert_eq!(short_sig.pages().len(), 1);
        assert_eq!(long_sig.pages().len(), 2);
    }
}
