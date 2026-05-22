# MOSAIC-FONTS KNOWLEDGE BASE

## OVERVIEW

`mos-fonts` owns font identity, metrics, shaping, and embedded font data. Layout asks for shaped
runs and widths; PDF asks for resource/subset details.

## CURRENT SCOPE

Implemented:

- Base-14 font metrics through `pdf-base14-metrics`.
- Bundled Noto Sans faces: regular, bold, italic, bold italic, mono.
- Embedded font loading from `data/*.ttf`.
- `rustybuzz` shaping for embedded fonts.
- Base-14 glyph names, WinAnsi helpers, extended glyph names.
- Width/ascent/descent helpers and simple family resolution.

Not implemented yet:

- System font discovery, arbitrary font files, fallback chains, variable fonts, Noto Sans Math, rich
  language/script itemization.

## WHERE TO LOOK

| Task               | Location                        | Notes                                      |
| ------------------ | ------------------------------- | ------------------------------------------ |
| Font enum/API      | `src/lib.rs`                    | `Font`, `EmbeddedFontId`, `FontFamily`.    |
| Bundled data       | `data/*.ttf`                    | Embedded at compile time.                  |
| Embedded internals | `src/embedded.rs`               | Shaping/subsetting helpers.                |
| Base-14 helpers    | `pdf-base14-metrics` re-exports | Glyph and width data.                      |
| Family resolution  | `FontFamily::resolve`           | Helvetica/Times/Courier/Noto Sans mapping. |
| Width/shaping      | `text_width`, `shape_text`      | Called by layout.                          |

## CONVENTIONS

- Keep shaping/metrics here; layout should not know font file internals.
- Keep PDF-specific object writing in `mos-pdf`, not here.
- Resource names are fixed and output-sensitive. Change only with tests.
- Unknown font family falls back with diagnostic upstream.

## GOTCHAS

- Base-14 non-Latin fallback can silently become `?` in current paths.
- Embedded Noto Sans covers much more, but CJK/emoji may still produce missing glyphs.
- `advance_units_to_pt` saturates through current unit choices.
- Manifest says use HarfBuzz/equivalent. Current equivalent is `rustybuzz` for embedded fonts.

## ANTI-PATTERNS

- Do not invent custom shaping.
- Do not assume every Unicode scalar has a glyph.
- Do not add system font probing without deterministic/reproducible build design.
