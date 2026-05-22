# pdf-base14-metrics

Pre-parsed Adobe Core 14 PDF font metrics for Mosaic's PDF/font stack.

This crate vendors the 14 Adobe Core PDF AFM files under `data/afm/`, parses them at build time with
the sibling `adobe-font-metrics` crate, and bakes them into `$OUT_DIR/baked.rs` as
`&'static FontMetrics<'static>` data. It sits below `mos-fonts` in the workspace graph.

## What It Provides

- `Base14Font`: enum for Helvetica, Times, Courier, Symbol, and `ZapfDingbats` faces.
- `Base14Font::ALL`: all 14 faces in stable PDF order.
- `Base14Font::metrics()`: borrowed parsed AFM metrics.
- `Base14Font::pdf_base_name()`: exact PDF `/BaseFont` name.
- `Base14Font::glyph_width(name)`: linear PostScript glyph-name width lookup.
- `Base14Font::glyph_width_by_name(name)`: baked O(log n) lookup for the 12 Latin faces.
- `Base14Font::winansi_width(byte)`: baked O(1) PDF `WinAnsiEncoding` width lookup for the 12 Latin
  faces.
- `winansi_glyph_name(byte)` and `winansi_byte(char)`: canonical PDF WinAnsi helpers.
- `extended_glyph_name(char)`: glyph names for Core 14 glyphs not reachable through WinAnsi, for
  `/Encoding` `/Differences` planning.

## Data Model

The AFM files are the source of truth for font metrics. `build.rs` generates Rust constants for
glyph metrics, kerning pairs, per-Latin-font WinAnsi width tables, and sorted glyph-name width
indexes.

The Adobe Glyph List is also checked in under `data/agl/`, but only as a test oracle for
`tests/winansi_vendor.rs`. Cargo excludes it from published crates; the build script uses the
hand-transcribed PDF 1.7 Annex D.2 WinAnsi table in `src/winansi_char_map.rs` /
`src/winansi_table.rs`.

## Examples

```rust
use pdf_base14_metrics::{Base14Font, extended_glyph_name, winansi_byte, winansi_glyph_name};

assert_eq!(Base14Font::Helvetica.pdf_base_name(), "Helvetica");
assert_eq!(Base14Font::Helvetica.glyph_width("A"), Some(667.0));
assert_eq!(Base14Font::Helvetica.winansi_width(b'A'), Some(667.0));
assert_eq!(
    Base14Font::Helvetica.glyph_width_by_name("Lslash"),
    Some(556.0)
);

assert_eq!(winansi_byte('é'), Some(0xE9));
assert_eq!(winansi_glyph_name(0xE9), Some("eacute"));
assert_eq!(extended_glyph_name('Ł'), Some("Lslash"));
```

`Symbol` and `ZapfDingbats` do not use WinAnsi. For those faces, `winansi_width` and
`glyph_width_by_name` return `None`; use `glyph_width` with the face's PostScript glyph names.

## Known Non-Goals

- No font discovery, shaping, fallback, or embedding. `mos-fonts` handles higher-level font policy.
- No Unicode coverage beyond what Core 14 AFMs expose. Missing Base-14 glyphs may become `?`
  downstream; real coverage needs embedded fonts.
- No generated-file editing. Change `data/afm/`, `build.rs`, or the source tables; never edit
  `$OUT_DIR/baked.rs`.
- No CP1252 aliasing. Helpers model PDF `WinAnsiEncoding`, including its gap bytes.
- No HTML/EPUB/layout behavior. This crate is only the vendored metrics layer.

## Licenses

Rust source is MIT. Vendored Core 14 AFM data is Adobe PostScript AFM License (`APAFML`). SPDX:
`MIT AND APAFML`.

The checked-in Adobe Glyph List test data is BSD-3-Clause, excluded from published crate artifacts.
