//! Raster image resolution and decode for `#image(...)` directives.
//!
//! The lowerer hands us a relative path (typed as `"path.png"` in the
//! source) plus the source file's location. We resolve it to an
//! absolute path, read the bytes, and decode them through the `image`
//! crate's PNG / JPEG decoders. The decoded pixels are flattened to
//! `RGB8` (alpha channels are composited onto white) so the PDF
//! backend can emit them as a `/DeviceRGB` Image `XObject` without
//! threading a soft-mask through the emit path.
//!
//! Diagnostics:
//!
//! - `MOS0037`: `#image(...)` called without a path string.
//! - `MOS0012`: cannot read the file on disk.
//! - `MOS0029`: cannot decode the bytes as PNG/JPEG.

use std::path::{Path, PathBuf};

use mos_core::{Diagnostic, DiagnosticAnnotation, SourceSpan, codes};

/// One decoded raster image, ready to be lowered onto a
/// [`mos_core::NodeKind::Image`] node.
#[derive(Debug, Clone)]
pub(crate) struct DecodedImage {
    /// Decoded width in pixels.
    pub width: u32,
    /// Decoded height in pixels.
    pub height: u32,
    /// Flat RGB8 byte buffer (`3 * width * height` bytes). Alpha
    /// channels are composited onto an opaque white background during
    /// decode so the PDF emit path can ship `/DeviceRGB` directly.
    pub rgb8: Vec<u8>,
}

/// Resolve `src_path` (as written in the source) relative to `source_file`
/// (the `.mos` file currently being lowered), then read + decode it.
///
/// Returns `Err(Diagnostic)` on I/O or decode failure; the resolver
/// surfaces these to the user without aborting the rest of the
/// document so a broken `#image(...)` still produces a partial PDF.
pub(crate) fn load(
    src_path: &str,
    source_file: &Path,
    call_span: &SourceSpan,
) -> Result<(PathBuf, DecodedImage), Box<Diagnostic>> {
    let resolved = mos_core::resolve_source_path(src_path, source_file);
    let bytes = std::fs::read(&resolved).map_err(|err| {
        Box::new(
            Diagnostic::simple(
                &codes::MOS0012,
                None,
                format!(
                    "cannot read image `{}`: {err}",
                    mos_core::display_path(&resolved)
                ),
            )
            .with_span(call_span.clone()),
        )
    })?;
    let decoded = decode(&bytes).map_err(|err| {
        Box::new(
            Diagnostic::simple(
                &codes::MOS0029,
                None,
                format!(
                    "cannot decode `{}`: {err}",
                    mos_core::display_path(&resolved)
                ),
            )
            .with_span(call_span.clone())
            .with_annotation(DiagnosticAnnotation::Note(
                "supported formats are PNG and JPEG".to_owned(),
            )),
        )
    })?;
    Ok((resolved, decoded))
}

#[allow(
    clippy::many_single_char_names,
    reason = "r/g/b/a are conventional pixel channel names"
)]
fn decode(bytes: &[u8]) -> Result<DecodedImage, String> {
    // `image::load_from_memory` picks the format from the magic bytes;
    // we leave format detection to it so PNG-with-.jpg-extension still
    // works. The `default-features = false` build only links the PNG
    // and JPEG decoders, so an unsupported format errors cleanly here.
    let img = image::load_from_memory(bytes).map_err(|err| err.to_string())?;
    let rgba = img.to_rgba8();
    let (w, h) = rgba.dimensions();
    let pixels = rgba.into_raw();
    // Composite alpha onto white: `out = src*alpha + white*(1-alpha)`.
    // The PDF emit path ships /DeviceRGB without a soft-mask in this
    // slice, so any partial alpha would otherwise punch through to the
    // page background. Fully transparent pixels render as pure white,
    // which is the conventional default for figure backgrounds.
    let pixel_count = pixels.len() / 4;
    let mut rgb8 = Vec::with_capacity(pixel_count * 3);
    for chunk in pixels.chunks_exact(4) {
        let [r, g, b, a] = [chunk[0], chunk[1], chunk[2], chunk[3]];
        if a == 255 {
            rgb8.push(r);
            rgb8.push(g);
            rgb8.push(b);
        } else if a == 0 {
            rgb8.extend_from_slice(&[255, 255, 255]);
        } else {
            rgb8.push(composite(r, a));
            rgb8.push(composite(g, a));
            rgb8.push(composite(b, a));
        }
    }
    Ok(DecodedImage {
        width: w,
        height: h,
        rgb8,
    })
}

/// Composite a single 8-bit colour channel onto opaque white using its
/// 8-bit alpha. `((c * a) + 255 * (255 - a)) / 255`, rounded to nearest.
fn composite(channel: u8, alpha: u8) -> u8 {
    let c = u32::from(channel);
    let a = u32::from(alpha);
    // `(c*a + 255*(255-a) + 127) / 255` rounds half away from zero;
    // operates entirely in u32 so no intermediate overflow on i32.
    let v = (c * a + 255 * (255 - a) + 127) / 255;
    u8::try_from(v.min(255)).unwrap_or(255)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    /// A 2×1 fully opaque PNG: red pixel + blue pixel. Hand-crafted so
    /// tests don't depend on filesystem access.
    fn red_blue_png() -> Vec<u8> {
        // Use `image` crate to produce the bytes — this is the same
        // round-trip we exercise in production.
        let mut buf = image::RgbaImage::new(2, 1);
        buf.put_pixel(0, 0, image::Rgba([255, 0, 0, 255]));
        buf.put_pixel(1, 0, image::Rgba([0, 0, 255, 255]));
        let mut out = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(buf)
            .write_to(&mut out, image::ImageFormat::Png)
            .unwrap();
        out.into_inner()
    }

    #[test]
    fn decode_opaque_png_round_trips_dimensions_and_colors() {
        let png = red_blue_png();
        let img = decode(&png).unwrap();
        assert_eq!((img.width, img.height), (2, 1));
        assert_eq!(img.rgb8, vec![255, 0, 0, 0, 0, 255]);
    }

    #[test]
    fn decode_transparent_pixel_composites_to_white() {
        let mut buf = image::RgbaImage::new(1, 1);
        buf.put_pixel(0, 0, image::Rgba([0, 128, 255, 0]));
        let mut out = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(buf)
            .write_to(&mut out, image::ImageFormat::Png)
            .unwrap();
        let img = decode(&out.into_inner()).unwrap();
        assert_eq!(img.rgb8, vec![255, 255, 255]);
    }

    #[test]
    fn decode_partial_alpha_composites_against_white() {
        // 50% alpha red on white → roughly (255, 127, 127).
        let mut buf = image::RgbaImage::new(1, 1);
        buf.put_pixel(0, 0, image::Rgba([255, 0, 0, 128]));
        let mut out = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(buf)
            .write_to(&mut out, image::ImageFormat::Png)
            .unwrap();
        let img = decode(&out.into_inner()).unwrap();
        // ((255 * 128) + (255 * 127) + 127) / 255 = 255 — red channel
        // stays at 255 (white is also 255 red). Green/blue go halfway
        // between 0 and 255.
        assert_eq!(img.rgb8[0], 255);
        assert!((i32::from(img.rgb8[1]) - 127).abs() <= 1);
        assert!((i32::from(img.rgb8[2]) - 127).abs() <= 1);
    }

    #[test]
    fn decode_bogus_bytes_returns_error() {
        let r = decode(b"not an image at all");
        assert!(r.is_err());
    }

    #[test]
    fn resolve_relative_path_uses_source_parent() {
        let src = Path::new("/tmp/proj/main.mos");
        let resolved = mos_core::resolve_source_path("img/scan.png", src);
        assert_eq!(resolved, PathBuf::from("/tmp/proj/img/scan.png"));
    }

    #[test]
    fn resolve_absolute_path_passes_through() {
        let src = Path::new("/tmp/proj/main.mos");
        let resolved = mos_core::resolve_source_path("/abs/path.png", src);
        assert_eq!(resolved, PathBuf::from("/abs/path.png"));
    }
}
