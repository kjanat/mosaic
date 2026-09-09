# adobe-font-metrics

Focused, zero-dependency parser for horizontal metrics in Adobe Font Metrics (AFM) v4.x files. Its
format reference is [Adobe Tech Note 5004][afm-spec]. In Mosaic it sits below `pdf-base14-metrics`,
which bakes Core-14 PDF font metrics for the font/layout/PDF pipeline.

This crate is published to crates.io with the rest of the Mosaic workspace.

> [!WARNING]
> While this crate is in the `0.0.x` line, Mosaic treats it as pre-alpha. Breaking changes are
> acceptable between patch releases. If you depend on this crate, pin an exact version such as
> `=0.0.2`, or accept the risk of API breakage.

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
- Character metrics: `C` / `CH` codes, `N` names, optional `B` bounding boxes, and one horizontal
  advance from `WX`, `W0X`, or the x component of `W` / `W0`. Direction-1 widths, y components,
  vertical-origin vectors, and ligatures are discarded.
- Kerning: `KPX`, `KPY`, `KP` inside `StartKernPairs` / `StartKernPairs0`. Only x adjustment is
  exposed; `KPY` retains the named pair with zero x adjustment. Both operands of `KP` and the y
  operand of `KPY` are validated. `KPH` and `StartKernPairs1` pairs are discarded.
- Direction blocks: `0` and `2` update the same flat fields; `1` is skipped. The selector `2`
  describes metrics shared by both directions in AFM, but the returned type has no direction tag.
- Composite definitions, track kerning, comments, and unknown keys are discarded. Their acceptance
  does not imply validation or preservation.

Required fields are currently `FontName` and `FontBBox`.

Missing optional string/numeric fields become empty strings/zero; missing `IsFixedPitch` becomes
`false`. A character without a name, horizontal advance, or box gets `""`, `0.0`, or `None`,
respectively. Global `CharWidth` is ignored: it neither supplies missing advances nor infers
`IsFixedPitch`. Values use `f32`, with its usual precision limits.

See the [coverage audit and scope decision][scope-audit] for the field matrix, validation limits,
test evidence, and criteria for future expansion.

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

`ParseError` reports missing headers, unsupported versions, missing required fields, and invalid
operands detected in the modeled subset. Source-originating errors carry 1-based line numbers. The
parser is a metric extractor with partial validation: successful parsing does not certify AFM
conformance. Counts are allocation hints, closing markers are not required at EOF, and text after
`EndFontMetrics` is ignored. `W` / `W0` validate only their first operand; discarded records are
generally unchecked. See the audit for additional lexical and structural limits.

## Non-Goals

- No AFM v3 compatibility claim until real fixtures validate it.
- No complete AFM authoring, serialization, or round-trip model. `into_owned()` preserves parsed
  fields, not discarded source information. Input is `&str`; there is no byte or streaming API.
- No ACFM/AMFM model, CID interpretation, or multiple-master interpolation.
- No font shaping, glyph outline loading, encoding conversion, or PDF emission.
- No vertical kerning API: `KPY` is validated but stores `adjust = 0.0`; `KP` exposes only x adjust.
- No composite glyph or track kerning model yet; those blocks are intentionally ignored.
- No dependency on higher Mosaic crates. Dependency direction stays boring: `adobe-font-metrics` ->
  `pdf-base14-metrics` -> `mos-fonts`.

[afm-spec]: https://adobe-type-tools.github.io/font-tech-notes/pdfs/5004.AFM_Spec.pdf
[scope-audit]: https://github.com/kjanat/mosaic/blob/master/docs/afm-parser-scope.md
