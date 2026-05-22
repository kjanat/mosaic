# adobe-font-metrics

Pure-Rust, zero-dependency parser for Adobe Font Metrics (AFM) v4.x files, per Adobe Tech Note 5004.
In Mosaic it sits below `pdf-base14-metrics`, which bakes Core-14 PDF font metrics for the
font/layout/PDF pipeline.

This crate is workspace-internal today: `publish = false`.

## Purpose

Parse `.afm` text into typed font metrics without pulling in runtime dependencies. The main entry
point is `parse(&str) -> Result<FontMetrics<'_>, ParseError>`.

The parser returns borrowed string data where possible via `Cow<'_, str>`. Parsed character and
kerning arrays are allocated as vectors, but glyph names and kerning operands borrow from the source
slice. Use `FontMetrics::into_owned()` when metrics must outlive the input string, be cached, baked
into generated tables, or sent across threads.

## Supported AFM Data

- Header: `StartFontMetrics` with AFM `4.x`; older/newer versions are rejected.
- Global fields: `FontName`, `FullName`, `FamilyName`, `Weight`, `ItalicAngle`, `IsFixedPitch`,
  `FontBBox`, `UnderlinePosition`, `UnderlineThickness`, `CapHeight`, `XHeight`, `Ascender`,
  `Descender`, `EncodingScheme`.
- Character metrics: `C`, `CH`, `WX`, `W0X`, `W`, `W0`, `N`, `B`.
- Kerning: `KPX`, `KPY`, `KP` inside `StartKernPairs` / `StartKernPairs0`.
- Direction blocks: direction `0` and `2` are read; direction `1` is skipped.
- Composite, track-kern, unknown, and out-of-scope records are accepted and ignored unless they are
  malformed records the parser explicitly models.

Required fields are currently `FontName` and `FontBBox`.

## Example

```rust
use adobe_font_metrics::parse;

fn main() -> Result<(), adobe_font_metrics::ParseError> {
    let src = "StartFontMetrics 4.1\n\
    FontName Demo\n\
    FontBBox 0 0 1000 1000\n\
    StartCharMetrics 1\n\
    C 65 ; WX 667 ; N A ; B 8 0 660 718 ;\n\
    EndCharMetrics\n\
    EndFontMetrics\n";

    let metrics = parse(src)?;

    assert_eq!(metrics.font_name, "Demo");
    assert_eq!(metrics.character_metrics[0].name, "A");
    Ok(())
}
```

Owned conversion:

```rust
use adobe_font_metrics::{OwnedFontMetrics, parse};

fn main() -> Result<(), adobe_font_metrics::ParseError> {
    let src = "StartFontMetrics 4.1\nFontName Demo\nFontBBox 0 0 1000 1000\nEndFontMetrics\n";
    let owned: OwnedFontMetrics = parse(src)?.into_owned();

    assert_eq!(owned.font_bbox.urx, 1000.0);
    Ok(())
}
```

## Errors

`ParseError` reports missing headers, unsupported versions, missing required fields, invalid
numbers, and malformed modelled records. Source-originating errors carry 1-based line numbers. Bad
AFM input should return an error, not panic.

## Non-Goals

- No AFM v3 compatibility claim until real fixtures validate it.
- No font shaping, glyph outline loading, encoding conversion, or PDF emission.
- No vertical kerning API: `KPY` is validated but stores `adjust = 0.0`; `KP` exposes only x adjust.
- No composite glyph or track kerning model yet; those blocks are intentionally ignored.
- No dependency on higher Mosaic crates. Dependency direction stays boring: `adobe-font-metrics` ->
  `pdf-base14-metrics` -> `mos-fonts`.
