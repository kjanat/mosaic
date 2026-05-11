//! End-to-end test: emit a PDF that contains a raster image, parse
//! it back with `lopdf`, and assert the Image `XObject`'s `/Width`,
//! `/Height`, `/ColorSpace`, and `/Filter` match what the layout
//! engine handed to the backend. Mirrors the embedded-font and
//! extended-latin round-trip tests in this crate.
//!
//! This guards the verification item in the slice's manifest:
//! "Image `XObject` in the PDF stream has the correct `/Width` /
//! `/Height` matching the source raster."

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::panic_in_result_fn,
    clippy::for_kv_map,
    reason = "integration test panics loudly on round-trip failures"
)]

use std::error::Error;
use std::sync::Arc;

use lopdf::{Document, Object};
use mosaic_layout::{ImageHandle, ImagePlacement, Page, PageGraph};
use mosaic_pdf::PdfMetadata;

type TestResult = Result<(), Box<dyn Error>>;

fn graph_with_image(width: u32, height: u32) -> PageGraph {
    let mut rgb8 = Vec::with_capacity((width * height * 3) as usize);
    for i in 0..(width * height) {
        rgb8.extend_from_slice(&[(i & 0xff) as u8, ((i >> 8) & 0xff) as u8, 0x80]);
    }
    let handle = ImageHandle {
        id: 0,
        resolved_path: "/tmp/probe.png".to_owned(),
        pixel_width: width,
        pixel_height: height,
        rgb8: Arc::from(rgb8),
    };
    PageGraph {
        pages: vec![Page {
            number: 1,
            width_pt: 595.276_f32,
            height_pt: 841.89_f32,
            runs: Vec::new(),
            images: vec![ImagePlacement {
                handle: handle.clone(),
                x_pt: 50.0,
                top_from_top_pt: 50.0,
                width_pt: 100.0,
                height_pt: 75.0,
            }],
        }],
        images: vec![handle],
    }
}

#[test]
fn image_xobject_round_trips_width_height_and_filter() -> TestResult {
    let graph = graph_with_image(8, 4);
    let tmp = std::env::temp_dir().join(format!(
        "mosaic-image-rt-{}.pdf",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos())
    ));
    let diags = mosaic_pdf::emit(&graph, &PdfMetadata::default(), &tmp)?;
    assert!(diags.is_empty(), "unexpected diagnostics: {diags:?}");

    let doc = Document::load(&tmp)?;

    // Walk every indirect object looking for the Image XObject. There
    // should be exactly one in this single-image document.
    let mut image_streams = 0;
    for (_object_id, obj) in &doc.objects {
        let Object::Stream(stream) = obj else {
            continue;
        };
        let dict = &stream.dict;
        let Ok(subtype) = dict.get(b"Subtype") else {
            continue;
        };
        let name = match subtype {
            Object::Name(n) => n.as_slice(),
            _ => continue,
        };
        if name != b"Image" {
            continue;
        }
        image_streams += 1;
        let w = dict.get(b"Width").unwrap().as_i64().unwrap();
        let h = dict.get(b"Height").unwrap().as_i64().unwrap();
        assert_eq!(w, 8, "Width mismatch");
        assert_eq!(h, 4, "Height mismatch");
        let bpc = dict.get(b"BitsPerComponent").unwrap().as_i64().unwrap();
        assert_eq!(bpc, 8, "BitsPerComponent mismatch");
        let cs = dict.get(b"ColorSpace").unwrap();
        assert!(
            matches!(cs, Object::Name(n) if n == b"DeviceRGB"),
            "ColorSpace should be /DeviceRGB, got {cs:?}"
        );
        let filter = dict.get(b"Filter").unwrap();
        let filter_name = match filter {
            Object::Name(n) => n.as_slice().to_vec(),
            // lopdf can also wrap single-entry filters in an array.
            Object::Array(arr) if arr.len() == 1 => match &arr[0] {
                Object::Name(n) => n.clone(),
                other => panic!("unexpected /Filter entry: {other:?}"),
            },
            other => panic!("unexpected /Filter: {other:?}"),
        };
        assert_eq!(filter_name, b"FlateDecode", "Filter should be FlateDecode");
    }
    assert_eq!(image_streams, 1, "expected exactly one Image XObject");
    std::fs::remove_file(&tmp).ok();
    Ok(())
}
