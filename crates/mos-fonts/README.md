# mos-fonts

Font identity, metrics, shaping, fallback, and bundled font data for Mosaic.

`mos-fonts` sits between `pdf-base14-metrics` and `mos-layout`. Layout asks this crate for font
families, shaped runs, and widths; `mos-pdf` uses the same font IDs and shaped glyph data when it
builds PDF resources and embedded subsets. Keep file emission in `mos-pdf`; keep font internals
here.

## What It Ships

- PDF Base-14 support through `pdf-base14-metrics`: Helvetica, Times, Courier, Symbol, and
  ZapfDingbats variants.
- Bundled Noto faces under `data/`: Noto Sans Regular/Bold/Italic/BoldItalic, Noto Sans Mono
  Regular, and Noto Sans Math Regular.
- `rustybuzz` shaping for embedded fonts.
- `subsetter` integration for embedded TTF subsets used by the PDF backend.
- Stable PDF resource names: Base-14 uses `F1`..`F14`; embedded Noto cuts use `F15`..`F20`.

## Base-14 vs Noto

Base-14 fonts do not embed outlines. PDF readers provide the glyphs. Mosaic measures them with baked
Adobe AFM data and emits WinAnsi bytes plus a small `/Differences` remap for supported extended
glyphs. Codepoints outside that path, such as Cyrillic, CJK, and emoji, silently measure and render
as `?` today.

Embedded Noto fonts use real bundled TTF bytes. They shape with `rustybuzz`, emit as PDF Type 0 CID
fonts, subset used glyphs, and write `/ToUnicode` maps so copy/paste can round-trip covered Unicode.
Noto Sans Math is currently a fallback face for math codepoints missing from Noto Sans.

## Main APIs

- `Font`: either `Font::Base14(Base14Font)` or `Font::Embedded(EmbeddedFontId)`.
- `EmbeddedFontId`: stable IDs for bundled Noto cuts: `Regular`, `Bold`, `Italic`, `BoldItalic`,
  `Mono`, `Math`.
- `FontFamily`: four styled slots, one monospace slot, and an embedded fallback chain.
- `FontFamily::resolve`: resolves `Helvetica`, `Times`/`Times-Roman`, `Courier`, and `Noto Sans`;
  unknown names emit `MOS0018` notices and fall back to Noto Sans.
- `text_width`, `glyph_width`, `ascent`, `descent`: layout metrics in points.
- `shape_text`: shapes one run; Base-14 returns width only and no glyph stream.
- `shape_with_fallback`: shapes embedded text cluster-by-cluster and retries `.notdef` clusters
  against configured fallback faces.
- `shape`: low-level embedded TTF shaping.
- `subset`: low-level embedded TTF subsetting for PDF `/FontFile2` streams.

## Module Layout

- `font.rs`: `Font`, `EmbeddedFontId`, PDF base/resource names.
- `family.rs`: family construction and user-facing font-name resolution.
- `metrics.rs`: width, ascent, descent, and font-unit conversion helpers.
- `shape.rs`: high-level shaped runs and cluster-granular fallback.
- `embedded.rs`: bundled `EmbeddedFont`, `ShapedGlyph`, `rustybuzz` shaping, TTF subsetting.
- `resources.rs`: lazy `include_bytes!` loading and generated resource-name table.

## Examples

```rust
use mos_fonts::{Base14Font, Font, text_width};

let width = text_width(Font::Base14(Base14Font::Helvetica), 10.0, "A");
assert_eq!(width, 6.67);
```

```rust
use mos_fonts::{EmbeddedFontId, FontFamily};

let family = FontFamily::noto_sans();
assert_eq!(family.fallbacks, &[EmbeddedFontId::Math]);
```

```rust
use mos_fonts::{EmbeddedFontId, Font, shape_with_fallback};

let runs = shape_with_fallback(
    Font::Embedded(EmbeddedFontId::Regular),
    &[EmbeddedFontId::Math],
    12.0,
    "a ≤ b",
);
assert!(!runs.is_empty());
```

## Known Non-Goals For Now

- No system font discovery.
- No arbitrary user font files.
- No variable fonts.
- No rich script/language itemization.
- No RTL shaping support; shaping currently forces LTR.
- No full GPOS positioning in PDF output; embedded shaping keeps glyph selection but normalizes
  advances to `hmtx` and zeroes offsets so layout matches current simple CID-string emission.
- No diagnostics for Base-14 missing Unicode; unsupported codepoints become `?`.
