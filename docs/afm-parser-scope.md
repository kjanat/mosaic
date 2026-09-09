# AFM parser coverage and API expansion

`adobe-font-metrics` models AFM metric data independently of Mosaic's rendering requirements. The
scope review in [#132](https://github.com/kjanat/mosaic/issues/132) resulted in implementation of
the metadata, directional metrics, ligatures, kerning, and composites previously discarded by the
horizontal extractor. The crate remains independently buildable, borrowing, and zero-dependency.

The format reference is [Adobe Tech Note 5004, AFM specification v4.1][spec]. AFM's spelling is
`MetricsSets`; selector 2 means both writing directions. Current fixtures cover v4.1. The header
accepts `4.` followed by decimal digits, without claiming features introduced by an untested future
version.

## Implemented coverage

| Format area                    | Representation and behavior                                                                                                                                                                                                                                          |
| ------------------------------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Header (§4)                    | Retains the format version; requires nonempty `FontName`, `FontBBox`, and a closing font marker.                                                                                                                                                                     |
| Identity and metadata (§7.1)   | Retains full/family names, weight, font version, notice, encoding/character-set descriptions, mapping scheme, escape byte, total character count, base/CID flags, capital/x heights, ascender/descender, and standard stem widths. Optional fields preserve absence. |
| Direction declaration (§4.1)   | `Option<MetricsSets>` retains the authored declaration; absence implies direction 0.                                                                                                                                                                                 |
| Direction metrics (§7.2)       | Both directions retain underline position/thickness, italic angle, `CharWidth`, and `IsFixedPitch`. `StartDirection 2` populates both entries. Unscoped direction metrics apply to direction 0.                                                                      |
| Origin vectors (§§7.1, 8)      | Global `VVector`, `IsFixedV`, and per-character `VV` remain available; an accessor resolves the character vector against the global value.                                                                                                                           |
| Character codes (§8)           | Decimal `C` and hexadecimal `CH` remain distinct. `-1` is unencoded. Hex digits retain leading zeroes and arbitrary length; bounded conversion to `u32` is optional. Names and boxes are optional.                                                                   |
| Character advances (§8)        | Both axes and writing directions survive. `WX`/`W0X`, `WY`/`W0Y`, `W1X`, `W1Y`, `W`/`W0`, and `W1` are modeled and validated. Scalar aliases imply zero on the other axis.                                                                                           |
| Ligatures (§8)                 | Every `L successor ligature` rule remains attached to its character in source order.                                                                                                                                                                                 |
| Pair kerning (§9.2)            | Full `KP`, `KPX`, `KPY`, and `KPH` vectors are retained. Named operands and hexadecimal encoded operands have different enum variants. Every pair carries its writing direction; unnumbered sections mean direction 0.                                               |
| Track kerning (§9.1)           | Retains degree, minimum/maximum point sizes, and both adjustment endpoints.                                                                                                                                                                                          |
| Composites (§10)               | Retains every `CC` definition and its ordered `PCC` component names and displacement vectors.                                                                                                                                                                        |
| Comments and extensions (§3.1) | `SourceRecord` retains comments and uninterpreted keywords with one-based line, section/parent context, keyword, and trimmed operand text, in source order. Unmodeled multiple-master arrays remain inspectable here.                                                |
| File bytes                     | `parse_bytes(&[u8])` validates and borrows an ASCII text view; `parse(&str)` uses the same validation. No lossy decoding. LF and CRLF are accepted.                                                                                                                  |
| Ownership                      | `into_owned()` detaches all strings and nested collections, including encoded operands, ligatures, composites, and extension records. Static borrowed arrays remain supported.                                                                                       |

## Authored values and defaults

Optional numeric/string/boolean fields retain absence. For example, a glyph with no authored width
has `None` in its advance array, while a zero width is `Some(Vector { x: 0.0, y: 0.0 })`.
`FontMetrics::advance(character, direction)` uses the character vector when present and otherwise
uses that direction's global `CharWidth`. `vertical_origin(character)` similarly resolves `VV`
against `VVector`.

`DirectionMetrics::fixed_pitch()` infers true from a global width when the flag is absent;
`FontMetrics::fixed_v()` applies the corresponding global-vector rule. Authored false flags that
conflict with those global vectors are rejected. Shared direction blocks assign only their specified
fields to both entries. Repeated assignments replace the earlier value in source order;
character/pair/track/composite collections retain all records in order.

Coordinates and scalar metrics remain `f32`, preserving the existing consumer representation. They
must be finite but retain ordinary binary floating-point precision limits. The parser does not
preserve original decimal spellings or distinguish equivalent width keyword aliases.

## Input and validation contract

The reader has explicit font, direction, character, kerning-pair, track, and composite states, with
an optional kerning container. It validates modeled operand arity and numeric syntax, recognized
records in incompatible sections, balanced section terminators, declared record counts, and
composite component counts. An incomplete file fails instead of returning partial metrics. Extra
nonblank data after `EndFontMetrics` also fails.

Counts are checked against actual parsed records and never used to reserve allocation capacity.
Input bytes may contain printable ASCII, tab, CR, LF, or ESC; other bytes produce `InvalidByte` with
the byte value, zero-based offset, and one-based line. `FontName` and `FontBBox` are required;
`MappingScheme 3` additionally requires `EscChar`. Invalid modeled operands retain line and field
context. Missing required fields identify the field without inventing a source location.

The reader accepts standalone pair/track sections for compatibility with existing consumers. It
preserves unknown records without validating their operands. Success is not a certificate of full
AFM conformance: glyph references, duplicate names, all direction-declaration constraints, and
consistency between individual glyph values and global vectors are not comprehensively checked.
Single-valued duplicate fields use the last value. Character records require exactly one `C` or
`CH`; their semicolon fields may occur in any order.

## Public API migration and Core-14 integration

This is a breaking pre-alpha public-model change. Consumers should pin exact patch versions as
described in [the versioning policy](versioning.md). The existing parser/type entry points remain,
but callers constructing or reading their fields must migrate:

| Previous field/API                       | Expanded model                                                                                                           |
| ---------------------------------------- | ------------------------------------------------------------------------------------------------------------------------ |
| `CharacterMetric::width_x`               | `advances[direction.index()]` for authored vectors; `font.advance(character, direction)` for a resolved optional vector. |
| Flat underline/italic/fixed-pitch fields | `font.direction(Direction::Zero)` or `Direction::One`, retaining optional authored values.                               |
| Empty/zero optional metadata             | `Option` preserves absence; consumers explicitly choose any fallback.                                                    |
| Integer character code                   | `CharacterCode::Decimal` or `Hex`; optional `as_u32()` conversion.                                                       |
| Required character name                  | `Option<Cow<str>>`; inspect with `as_deref()`.                                                                           |
| Pair `left`, `right`, `adjust`           | `KerningOperands`, complete `adjustment: Vector`, and `direction: Direction`.                                            |

[`pdf-base14-metrics/build.rs`][baker] now emits the complete model, including metadata, ligatures,
and retained source records. Its WinAnsi and glyph-name width indexes explicitly select direction 0
and the x component of the effective advance. Runtime width helpers use the same projection.
`mos-fonts` explicitly defaults absent ascender/descender values to zero, preserving its existing
behavior. The font crate does not start applying vertical layout, kerning, ligatures, or composites
merely because their source data is now available.

## Remaining format work

- **AFM v3:** rejected until real, independently vendored fixtures and a version-difference audit
  support a compatibility implementation.
- **Multiple-master arrays and related containers:** AFM-resident arrays remain uninterpreted source
  records; ACFM/AMFM containers, CID hierarchy resolution, and interpolation have no typed API here.
- **Serialization and source reproduction:** no writer or exact round-trip contract. Comments and
  extensions retain context, but original whitespace, recognized keyword spellings, and duplicate
  scalar history do not survive the semantic model.
- **Streaming and full conformance validation:** the input is a complete borrowed slice; the
  validation contract above deliberately lists the remaining checks.

These boundaries follow implemented format coverage. Shaping, encoding conversion, outlines,
interpolation, and PDF emission remain consumer responsibilities.

## Validation

The parser's local [`Extended.afm` fixture][fixture] exercises both writing directions, metadata,
scalar/global defaults, hexadecimal codes, ligatures, full pair vectors, tracks, composites, and
extensions. [Coverage tests][scope-tests] verify ownership after dropping the source buffer, LF/CRLF
byte parsing, invalid byte locations, malformed newly modeled records, section/count errors, and
every proper line-boundary truncation of the fixture. Existing [parser regressions][parser-tests]
continue to cover real Adobe AFMs and malformed records.

The downstream suite compares all fourteen complete baked font models to fresh parses of their
vendored AFMs, alongside existing glyph-width and WinAnsi oracles. An implementation-time comparison
against the previous parser also matched all 23,232 lines of projected font/glyph/horizontal-kerning
data across the fourteen fonts.

Run from the repository root:

```sh
cargo test -p adobe-font-metrics -p pdf-base14-metrics
cargo +1.85.0 test -p adobe-font-metrics -p pdf-base14-metrics
cargo tw --quiet
cargo lint
RUSTDOCFLAGS='-D warnings' cargo doc -p adobe-font-metrics -p pdf-base14-metrics --no-deps
```

The two standalone metric crates retain their Rust 1.85 floor. Workspace consumers follow the
workspace MSRV.

[spec]: https://adobe-type-tools.github.io/font-tech-notes/pdfs/5004.AFM_Spec.pdf
[parser-tests]: ../crates/adobe-font-metrics/tests/parser.rs
[scope-tests]: ../crates/adobe-font-metrics/tests/scope.rs
[fixture]: ../crates/adobe-font-metrics/tests/fixtures/Extended.afm
[baker]: ../crates/pdf-base14-metrics/build.rs
