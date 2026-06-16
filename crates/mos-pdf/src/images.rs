//! Raster image `XObject` emission for the PDF backend.
//!
//! The layout engine hands us a [`mos_layout::PageGraph::images`]
//! table: one entry per unique on-disk image, dedup'd by resolved
//! path. For each entry we emit a single Image `XObject`
//! (`/Type /XObject /Subtype /Image`) with `/Width`, `/Height`,
//! `/BitsPerComponent`, `/ColorSpace /DeviceRGB`, and the raw RGB8
//! pixel buffer flate-compressed under `/Filter /FlateDecode`. Each
//! page's `/Resources /XObject` dict lists every image, so per-page
//! resource dicts stay byte-stable across pages.
//!
//! Per-page placements arrive as [`ImagePlacement`]s; the content
//! stream wraps each in `q  w 0 0 h x y cm /Im<id> Do Q` so the image
//! occupies the requested width/height rectangle at the requested
//! page-relative position (after the same top→bottom flip the text
//! emit path applies).
//!
//! Alpha/soft-mask support is deferred. The eval layer composites
//! every input image onto opaque white before handing the bytes off,
//! so the emit path can stay on a single `/DeviceRGB` /Filter
//! /`FlateDecode` codepath.

use flate2::Compression;
use flate2::write::ZlibEncoder;
use mos_layout::{ImageHandle, ImagePlacement};
use pdf_writer::{Content, Filter, Finish, Name, Pdf, Ref};
use std::io::Write;

/// Stable PDF resource name for the image with `handle.id`. Mirrors
/// the `/F<n>` convention used by the font emitter; every page's
/// resource dict declares `/Im<n>` so PDF readers can resolve the
/// references in the content stream.
#[must_use]
pub(crate) fn resource_name(handle: &ImageHandle) -> String {
    format!("Im{}", handle.id)
}

/// Compress a flat RGB8 byte buffer with zlib (the format PDF expects
/// behind `/FlateDecode`). Uses default compression for a reasonable
/// size/speed tradeoff; image `XObject` byte stability across runs is
/// preserved because `flate2`'s default settings are deterministic.
pub(crate) fn flate_compress(bytes: &[u8]) -> Vec<u8> {
    let mut encoder = ZlibEncoder::new(Vec::with_capacity(bytes.len() / 2), Compression::default());
    // `write_all` and `finish` on a `Vec<u8>` sink cannot fail except
    // under OOM (which aborts the process anyway). Both branches below
    // are unreachable in practice; the `debug_assert!`s fire in tests
    // if that invariant is ever violated. Returning uncompressed bytes
    // is *not* a correctness path: `emit_image_xobject` unconditionally
    // sets `/Filter /FlateDecode`, so an uncompressed fallback would
    // produce a PDF the reader chokes on. It exists purely as a
    // last-resort escape from a release-mode panic.
    if let Err(err) = encoder.write_all(bytes) {
        debug_assert!(false, "flate sink failed: {err}");
        return bytes.to_vec();
    }
    encoder.finish().unwrap_or_else(|err| {
        debug_assert!(false, "flate finish failed: {err}");
        bytes.to_vec()
    })
}

/// Emit one Image `XObject` (`/Subtype /Image`) for `handle` at `id`.
/// `compressed` is the zlib-compressed pixel stream produced by
/// [`flate_compress`]; passed in pre-compressed so the caller can hold
/// onto the bytes for byte-stability assertions in tests.
pub(crate) fn emit_image_xobject(pdf: &mut Pdf, id: Ref, handle: &ImageHandle, compressed: &[u8]) {
    let mut img = pdf.image_xobject(id, compressed);
    img.filter(Filter::FlateDecode);
    img.width(i32::try_from(handle.pixel_width).unwrap_or(i32::MAX));
    img.height(i32::try_from(handle.pixel_height).unwrap_or(i32::MAX));
    img.color_space_name(Name(b"DeviceRGB"));
    img.bits_per_component(8);
    img.finish();
}

/// Emit the `q w 0 0 h x y cm /Im<id> Do Q` block for one placement,
/// translated into PDF's bottom-origin coordinate space using
/// `page_height_pt`.
pub(crate) fn emit_placement(
    content: &mut Content,
    page_height_pt: f32,
    placement: &ImagePlacement,
) {
    let resource = resource_name(&placement.handle);
    let y_top_from_bottom = page_height_pt - placement.top_from_top_pt;
    let y_bottom = y_top_from_bottom - placement.height_pt;
    content.save_state();
    // PDF `cm` operator: [a b c d e f] places the unit square at
    // (e, f) scaled by (a, d). We want an axis-aligned image whose
    // bottom-left corner sits at (x, y_bottom) and whose extents are
    // (width_pt, height_pt). The image XObject's natural coordinate
    // system is [0,1]×[0,1], so the matrix is [w 0 0 h x y].
    content.transform([
        placement.width_pt,
        0.0,
        0.0,
        placement.height_pt,
        placement.x_pt,
        y_bottom,
    ]);
    content.x_object(Name(resource.as_bytes()));
    content.restore_state();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resource_name_uses_im_prefix() {
        let h = ImageHandle {
            id: 7,
            resolved_path: "/x.png".to_owned(),
            pixel_width: 1,
            pixel_height: 1,
            rgb8: std::sync::Arc::from(vec![0_u8; 3]),
        };
        assert_eq!(resource_name(&h), "Im7");
    }

    #[test]
    fn flate_compress_round_trips_short_input() {
        // Smoke test: the zlib stream we produce must decompress back
        // to the original bytes, otherwise PDF readers will choke on
        // /FlateDecode'd image streams.
        use flate2::read::ZlibDecoder;
        use std::io::Read;
        let raw = b"\x00\x01\x02\x03\xff\xfe\xfd repeat me repeat me repeat me";
        let compressed = flate_compress(raw);
        let mut out = Vec::new();
        ZlibDecoder::new(&compressed[..])
            .read_to_end(&mut out)
            .unwrap();
        assert_eq!(out.as_slice(), raw);
    }
}
