# mos-pdf

PDF backend for Mosaic. `mos-pdf` consumes a `mos_layout::PageGraph` and writes a PDF file; it does
not parse `.mos`, lower documents, or decide layout policy.

## Purpose

- Public entry point: `emit(&PageGraph, &PdfMetadata, &Path)`.
- Creates the output parent directory when needed.
- Returns PDF-emission diagnostics, currently including Base-14 extended-glyph budget warnings.
- Writes `PdfMetadata.title` and `PdfMetadata.author` to the PDF Info dictionary.
- Captures `PdfMetadata.language`, but does not emit catalog `/Lang` yet.

## Emitted PDF Support

- Pages, page tree, catalog, content streams, and per-page resources.
- Text runs from layout, including top-origin layout coordinates converted to PDF bottom-origin
  coordinates.
- Raster image XObjects and page placements.
- `/ActualText` spans for replacement runs, used by layout for source text round-tripping.
- Empty page graphs still produce a valid PDF.

## Fonts, Images, Encoding

- Declares all 14 standard PDF Base-14 fonts in stable resource order.
- Uses `/WinAnsiEncoding` for Base-14 Latin faces when no extended glyph remapping is needed.
- Plans per-document `/Differences` encodings for extended Latin/Core-14 glyphs that fit in the
  single-byte budget, with matching `/ToUnicode` CMaps for copy/paste and search.
- Unsupported Base-14 characters can render as `?`; documents needing broader coverage should use
  the bundled embedded Noto Sans path provided by layout/fonts.
- Emits used embedded faces as Type 0 CID-keyed fonts with subsetted `/FontFile2`, Identity-H,
  widths, and `/ToUnicode` CMaps.
- Emits decoded image data as RGB8 `/DeviceRGB` image XObjects compressed with `/FlateDecode`.
- PNG/JPEG decoding and image deduplication happen before this crate, in earlier pipeline stages.
- Alpha/soft masks are not emitted; upstream image handling composites to opaque RGB.

## Module Layout

- `src/lib.rs`: public API, object planning, page/resource/font/image object emission, metadata.
- `src/content.rs`: per-page content streams for text and image placements.
- `src/encoding.rs`: Base-14 `/Differences` and `/ToUnicode` planning.
- `src/embedded.rs`: embedded Type 0/CID font subset object emission.
- `src/images.rs`: image XObject resources, Flate compression, placement operators.

## Deterministic Behavior

Byte stability matters. The emitter uses fixed ordering for Base-14 fonts, embedded font IDs, image
handles, page refs, encoding refs, and resource dictionaries. Avoid hash-map iteration when it would
affect allocation or emission order. Tests prefer structural PDF checks over byte-for-byte
snapshots.

## Examples

From the workspace root:

```sh
cargo mos build examples/hello/main.mos
cargo mos build examples/math/main.mos
```

These write `build/main.pdf` through the CLI pipeline: parse, lower, resolve, layout, then `mos-pdf`
emission.

Direct crate usage starts from an existing `PageGraph`:

```rust
use std::path::Path;

use mos_pdf::PdfMetadata;

let metadata = PdfMetadata {
    title: Some("Demo".to_owned()),
    author: Some("Mosaic".to_owned()),
    language: None,
};

let diagnostics = mos_pdf::emit(&graph, &metadata, Path::new("build/main.pdf"))?;
```

## Known Non-goals

- No source parsing, semantic lowering, line breaking, page breaking, or image decoding here.
- No hyperlinks, bookmarks, tagged PDF, PDF/A, vector graphics, SVG page emission, or debug layout
  backend.
- No catalog language metadata yet.
- No font shaping invented here; layout/fonts provide shaped embedded glyph runs.
- No manifest-roadmap PDF features unless current code and tests prove them.
