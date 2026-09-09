# adobe-font-metrics

Zero-dependency, borrowing parser for Adobe Font Metrics (AFM) v4.x files, using
[Adobe Tech Note 5004][afm-spec] as its format reference. The public model covers font metadata,
both writing directions, character vectors and ligatures, pair and track kerning, composites,
comments, and extension records.

This independently published crate also supplies Mosaic's `pdf-base14-metrics` build-time parser.
Its AFM data model includes metrics that Mosaic does not consume.

> [!WARNING]
> While this crate is in the `0.0.x` line, Mosaic treats it as pre-alpha. Breaking changes are
> acceptable between patch releases. If you depend on this crate, pin an exact version such as
> `=0.0.2`, or accept the risk of API breakage.

## Parsing and ownership

Use `parse(&str)` for text or `parse_bytes(&[u8])` for file bytes. Both return
`Result<FontMetrics<'_>, ParseError>` and apply the same ASCII validation. Byte parsing borrows a
validated text view without lossy decoding. LF and CRLF line endings are accepted.

Strings borrow the input through `Cow`. Parsed collections allocate in proportion to the records
actually read. `FontMetrics::into_owned()` detaches every string and nested collection so the result
can outlive its source. The model also supports borrowed static arrays for generated tables.

```rust
use adobe_font_metrics::{Direction, Vector, parse_bytes};

fn main() -> Result<(), adobe_font_metrics::ParseError> {
    let source = b"StartFontMetrics 4.1\n\
        FontName Demo\n\
        FontBBox 0 0 1000 1000\n\
        CharWidth 600 0\n\
        StartCharMetrics 1\n\
        C 65 ; N A ; B 8 0 590 718 ;\n\
        EndCharMetrics\n\
        EndFontMetrics\n";
    let font = parse_bytes(source)?;
    let character = &font.character_metrics[0];

    assert_eq!(character.name.as_deref(), Some("A"));
    assert_eq!(character.advances[0], None); // No authored character width.
    assert_eq!(
        font.advance(character, Direction::Zero),
        Some(Vector { x: 600.0, y: 0.0 })
    );
    assert!(font.direction(Direction::Zero).fixed_pitch());
    assert_eq!(font.notice, None); // Absence remains distinguishable from an empty notice.

    let owned = font.into_owned();
    assert_eq!(owned.font_name, "Demo");
    Ok(())
}
```

## Supported data

| AFM surface              | Public representation                                                                                                                                                |
| ------------------------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Header and font identity | `afm_version`, required `font_name` and `font_bbox`; optional full/family names, weight, font version, notice, encoding and character-set descriptions               |
| Global metadata          | `metrics_sets`, `mapping_scheme`, `esc_char`, `characters`, base/CID flags, `v_vector`, `is_fixed_v`, capital/x heights, ascender/descender and standard stem widths |
| Writing directions       | Two `DirectionMetrics` entries with optional underline metrics, italic angle, `char_width`, and fixed-pitch flag; `StartDirection 2` applies to both                 |
| Character metrics        | `CharacterCode::Decimal` / `Hex`, optional name/box, two optional advance vectors, optional `VV`, and every ligature rule                                            |
| Advance spellings        | `WX` / `W0X`, `WY` / `W0Y`, `W1X`, `W1Y`, `W` / `W0`, and `W1`; full vectors retained                                                                                |
| Pair kerning             | `KP`, `KPX`, `KPY`, `KPH`, complete adjustment vectors, named or hexadecimal operands, and direction identity                                                        |
| Track kerning            | Degree and both point-size/adjustment endpoints                                                                                                                      |
| Composites               | Composite name and all ordered `PCC` component names/offsets                                                                                                         |
| Comments and extensions  | Ordered `source_records` with line, section or parent-record context, keyword, and trimmed operand text                                                              |

Optional authored fields use `Option`. `FontMetrics::advance` resolves missing character advances
against the selected direction's `CharWidth`; `vertical_origin` resolves `VV` against `VVector`.
Explicit character values take precedence, including zero vectors. `DirectionMetrics::fixed_pitch`
and `FontMetrics::fixed_v` expose the flags implied by global vectors when the flags are absent.

Unnumbered kerning sections mean direction 0. Shared direction blocks update both entries in source
order; later assignments to the same field replace earlier assignments. Scalar width aliases imply
zero on the other axis. Hexadecimal codes preserve their digits and leading zeroes without guessing
an encoding; `CharacterCode::as_u32()` is an optional bounded numeric conversion. Numeric metrics
retain the existing `f32` representation and its precision limits.

## Validation

The parser rejects invalid bytes, unsupported headers, non-finite or malformed modeled numbers,
incorrect operand arity, incompatible section nesting, mismatched declared record/component counts,
unclosed sections, a missing `EndFontMetrics`, and nonblank trailing data. Declared counts never
control allocation capacity. Source errors carry one-based line numbers; invalid-byte errors also
include the zero-based byte offset. Missing required fields are identified by name.

`FontName` and `FontBBox` are required. `MappingScheme 3` requires `EscChar`. Global `CharWidth` or
`VVector` cannot accompany an explicitly false corresponding fixed flag. Pair and track sections may
appear outside `StartKernData` for compatibility with existing callers.

Parsing does not certify full AFM conformance: it does not resolve glyph references, enforce all
cross-field or direction-declaration constraints, validate unknown extension operands, or preserve
every source spelling. Repeated scalar fields use the last value; repeated records retain their
order. See the [coverage and migration guide][scope-audit] for the full boundary.

## API migration

This expansion changes public struct fields, including the types re-exported by
`pdf-base14-metrics`. Callers using struct literals must migrate with the parser:

- Replace `character.width_x` with `font.advance(character, Direction::Zero).map(|v| v.x)` for an
  effective horizontal width, or inspect `character.advances` for authored vectors.
- Read flat direction fields through `font.direction(Direction::Zero)`.
- Handle optional metadata and glyph names explicitly instead of assuming empty strings or zeroes.
- Match `CharacterCode` and `KerningOperands`; kerning now uses `adjustment` and `direction`.

The existing `parse`, `ParseError`, `BBox`, `FontMetrics`, and `OwnedFontMetrics` entry points
remain. Existing error variants remain available, with `InvalidByte` added.

## Remaining format work

AFM v3 needs independently vendored real fixtures and a compatibility audit before support can be
claimed. ACFM/AMFM containers and typed multiple-master arrays are not implemented; AFM-resident
unmodeled metadata is retained in `source_records`. Semantic serialization, exact source
round-tripping, and streaming input remain future work.

Font shaping, outline loading, encoding conversion, interpolation, and PDF emission belong to
consumers. This crate has no dependency on higher Mosaic crates.

[afm-spec]: https://adobe-type-tools.github.io/font-tech-notes/pdfs/5004.AFM_Spec.pdf
[scope-audit]: https://github.com/kjanat/mosaic/blob/master/docs/afm-parser-scope.md
