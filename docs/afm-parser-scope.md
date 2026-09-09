# AFM parser coverage and scope

Decision for [#132](https://github.com/kjanat/mosaic/issues/132), audited on 2026-09-09 against the
parser at `2a5e7d935d8fe9390795cf58418ff00fee70e49c`. The accompanying changes clarify docs and add
boundary tests; parsing behavior and public types stay unchanged.

## Decision

Keep `adobe-font-metrics` as a focused, zero-dependency AFM v4.x horizontal metric extractor. Keep
its crate name and Rust 1.85 floor; make the package description, README, and rustdoc state the
scope explicitly. The public structs serve the existing Core-14/PDF consumer well, but they discard
information needed by a general AFM editor, validator, or serializer.

No missing format surface is selected for implementation in this exploration. Expansion needs a
consumer, representative fixtures, and an explicit data-model decision. The current downstream
consumer provides no need for vertical metrics, ligatures, composites, track kerning, or AFM v3.
Consequently this decision creates no expansion issues.

The issue's README `publish = false` finding was already fixed in
[#154](https://github.com/kjanat/mosaic/pull/154). The current README and package metadata both
describe a published crate. External consumers should pin an exact pre-alpha patch version, as
described in the [versioning policy](versioning.md).

## Evidence and terminology

The format reference is Adobe's [Tech Note 5004, AFM specification 4.1, 7 October 1998][spec].
Relevant sections are §3 (syntax), §4 (AFM structure), §§5–6 (ACFM/AMFM), §7 (global and directional
metrics), §8 (character metrics), §9 (kerning), and §10 (composites). Page references below use the
printed page numbers. AFM itself is line-oriented; this parser's limitation is the subset of
information and validation it implements.

Implementation evidence comes from [`parse`, `ParseAccumulator`, and the record parsers][parser],
the existing [parser regressions][parser-tests], the new [scope tests][scope-tests], and the
downstream [static-table generator][baker]. In the matrix, **retained** means exposed in public
types, **projected** means only part survives, **discarded** means no public representation, and
**rejected** means the entry point cannot read that format. Acceptance alone is not full support.

## Coverage matrix

| AFM surface                                                                                                                                                               | Current result                                                                                                                                | Consumer consequence                                                                                                                                   |
| ------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `StartFontMetrics` (§4, p. 12)                                                                                                                                            | Accepts `4.` followed by one or more ASCII digits; rejects other versions. Requires nonempty `FontName` and a parsed `FontBBox`.              | Tested real fixtures are 4.1; the 4.x header gate is broader than the fixture evidence.                                                                |
| `MetricsSets` (§4.1)                                                                                                                                                      | Discarded. This is the specification's plural spelling; the issue lists `MetricsSet`.                                                         | No validation of declared directions or completeness.                                                                                                  |
| `FontName`, `FullName`, `FamilyName`, `Weight`, `EncodingScheme` (§7.1)                                                                                                   | Retained as borrowed, trimmed strings.                                                                                                        | Only `FontName` is required among these; missing optional values are empty.                                                                            |
| `FontBBox`, `CapHeight`, `XHeight`, `Ascender`, `Descender` (§7.1)                                                                                                        | Retained as `f32`; box has four coordinates.                                                                                                  | Required font box; optional scalar metrics default to zero. No missing-versus-zero distinction.                                                        |
| `Version`, `Notice`, `Comment`, `MappingScheme`, `EscChar`, `CharacterSet`, `Characters`, `IsBaseFont`, `IsCIDFont`, `VVector`, `IsFixedV`, `StdHW`, `StdVW` (§§3.1, 7.1) | Discarded. Multiple-master metadata such as `BlendAxisTypes`, `BlendDesignPositions`, `BlendDesignMap`, and `WeightVector` is also unmodeled. | No notices/comments, encoding interpretation, CID model, vertical-origin model, stem metrics, or interpolation data in the result.                     |
| `StartDirection 0` / `2` (§7.2, p. 29)                                                                                                                                    | Both read into the same flat fields; later assignments overwrite earlier ones.                                                                | Selector `2` is AFM's shared-metrics case, not a third independent direction. No direction identity is retained.                                       |
| `StartDirection 1`                                                                                                                                                        | Contents skipped through `EndDirection`.                                                                                                      | Direction-1 values do not overwrite horizontal values.                                                                                                 |
| `UnderlinePosition`, `UnderlineThickness`, `ItalicAngle`, `IsFixedPitch` (§7.2)                                                                                           | Retained outside skipped blocks; missing numbers/boolean become zero/false.                                                                   | No validation of consistency with other font metrics.                                                                                                  |
| Global `CharWidth` (§7.2, p. 30)                                                                                                                                          | Discarded.                                                                                                                                    | Does not fill missing glyph advances or imply `is_fixed_pitch = true`. A fixed-width font relying on this global default will not yield usable widths. |
| Character `C`, `CH`, `N`, `B` (§8, pp. 31–32)                                                                                                                             | Code, name, and optional box retained in input order. `C` is `i32`; `CH` parses hexadecimal into `i32`.                                       | No code-to-Unicode mapping or sorting. Missing `N`/`B` becomes an empty name/`None`; codes beyond `i32` fail.                                          |
| `WX`, `W0X`, `W`, `W0`                                                                                                                                                    | Projected into one `width_x`: scalar value or vector's first operand. Last recognized width wins.                                             | No original spelling or y value survives.                                                                                                              |
| `WY`, `W0Y`, `W1X`, `W1Y`, `W1`, `VV`, `L`                                                                                                                                | Discarded.                                                                                                                                    | No vertical width, direction-1 width, per-glyph origin vector, or ligature model. If no recognized horizontal width is present, `width_x` stays zero.  |
| `StartKernPairs` / `StartKernPairs0` (§9.2, pp. 35–36)                                                                                                                    | Share one ordered vector of named pairs.                                                                                                      | No source-block identity, deduplication, or sorting.                                                                                                   |
| `KPX`, `KP`, `KPY`                                                                                                                                                        | `KPX` retains x. `KP` validates both components and retains x. `KPY` validates y and retains a pair with x adjustment zero.                   | Vertical kerning is unavailable; zero-adjustment rows can exist in the output.                                                                         |
| `KPH`, `StartKernPairs1`                                                                                                                                                  | Hexadecimal pairs are discarded; direction-1 pair rows are skipped.                                                                           | No hexadecimal-pair conversion or directional kerning API.                                                                                             |
| `StartTrackKern`, `TrackKern`, `EndTrackKern` (§9.1)                                                                                                                      | Keywords ignored, with no track-kern state or model.                                                                                          | No tracking curve or operand validation.                                                                                                               |
| `StartComposites` through `EndComposites`, including `CC` / `PCC` (§10)                                                                                                   | Block contents discarded.                                                                                                                     | Composite glyphs can still have ordinary character metrics, but their construction recipe is lost.                                                     |
| ACFM/AMFM (§§5–6), AFM v3                                                                                                                                                 | Their headers are rejected.                                                                                                                   | No composite-font hierarchy or multiple-master family model; reading a master's supported AFM fields does not implement interpolation.                 |
| Unknown keys                                                                                                                                                              | Ignored at top level and within recognized character records.                                                                                 | Extension data is lost; unknown records are not validated.                                                                                             |

## Parsing and validation limits

The entry point accepts `&str`. ASCII AFM text fits this API, but there is no decoding, byte-input,
streaming, writing, or comment-preservation API. `Cow` avoids copying retained names and strings;
the character and kerning vectors are allocated. `into_owned()` detaches this already reduced model
from the source. It cannot recover discarded information or reproduce the original file. All metric
numbers use `f32`, so decimal precision is bounded.

The following are observations of the current reader, **not guarantees of AFM conformance** or a
recommendation to produce such input:

- Blank lines and `Comment` lines before the header are accepted. Parsing stops at `EndFontMetrics`
  and ignores later text. EOF does not require that marker or closed sections.
- Character/pair counts are parsed as `usize` and used for fallible capacity reservation. They are
  not compared with actual row counts. Counts on discarded sections are generally unchecked.
- Section order and nesting are not comprehensively checked. Recognized global keys can be read
  inside metric sections. Pair rows can be read directly after `StartKernData` without a pair
  subsection. Character rows are recognized only when their first keyword is `C` or `CH` and the
  parser is in character-metric state.
- `W` and `W0` check only the first operand: missing, nonnumeric, or extra y-side text can pass.
  This differs from `KP`, which checks both values and exact arity. `KPX` and `KPY` also check exact
  arity, and both font and character boxes require exactly four numbers.
- `CH` strips angle brackets permissively; it does not enforce paired delimiters. Name/string syntax
  is not fully validated. `f32` parsing accepts non-finite values, and ASCII restrictions on strings
  are not enforced. There is no cross-field consistency validation.
- Duplicate fields overwrite earlier values, while repeated character/pair rows remain in order.
  Missing optional values generally collapse to defaults. `ParseError` provides line context for
  detected source errors, not a comprehensive validation report.

These limits explain why the public docs promise extraction of selected metrics. A consumer that
needs strict validation must account for them; this audit does not silently tighten accepted input.

## Downstream fit and expansion gates

`pdf-base14-metrics/build.rs` parses fourteen vendored Core-14 AFMs and emits static `FontMetrics`,
character arrays, kerning arrays, and horizontal WinAnsi/name-width tables. The new tests use local
synthetic inputs; the existing independently vendored Helvetica, Courier, and Times-Roman fixtures
remain in the parser crate. No new dependency, fixture sharing, generated table edit, or public
field is needed for this decision.

| Possible change                                              | Decision and evidence required before implementation                                                                                                                                         |
| ------------------------------------------------------------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Vertical/directional metrics and global `CharWidth` defaults | Defer until a consumer needs fonts that rely on these records. Specify direction identity, default precedence, and horizontal compatibility together; use fixtures with distinct directions. |
| Ligatures                                                    | Defer until a consumer needs AFM ligature rules. Preserve multiple `L` entries per glyph with fixtures; keep shaping policy in its owning layer.                                             |
| Composites or track kerning                                  | Defer until a consumer needs the construction recipes or tracking function. Each needs a separate model and fixtures.                                                                        |
| AFM v3                                                       | Defer until real v3 fixtures establish the supported differences and error behavior. Similar syntax alone is insufficient evidence.                                                          |
| Byte input                                                   | Defer until a concrete input/encoding problem requires it; specify decoding and borrowing behavior first.                                                                                    |
| Strict validation                                            | Defer a policy change until caller requirements establish whether it is opt-in or the default. Counts, terminators, vector arity, finite numbers, and compatibility need explicit tests.     |
| Full authoring/round-trip API                                | Outside the selected scope. It needs source preservation and a richer model with a demonstrated consumer.                                                                                    |
| Rename or add dependencies                                   | Keep the existing name and zero dependencies. More precise description/docs address the current adoption concern.                                                                            |

## Validation

The [scope tests][scope-tests] exercise horizontal width aliases alongside discarded directional
fields, optional character defaults, missing global-width inference, direction-0/shared-field
projection, x-only kerning in both supported pair sections, ignored data, borrowed strings, and the
v3/ACFM/AMFM rejection boundary. The existing malformed-input regressions continue to check detected
errors and line context. Permissive validation gaps above are documented observations, not new
promises enforced by those regression tests.

Run from the repository root:

```sh
cargo test -p adobe-font-metrics -p pdf-base14-metrics
cargo +1.85.0 test -p adobe-font-metrics -p pdf-base14-metrics
cargo lint
cargo doc -p adobe-font-metrics --no-deps
```

The downstream suite checks all fourteen baked faces and the WinAnsi mapping. No PDF layout or
emission code changes in this audit, so the relevant output checks are the existing metric tables.

[spec]: https://adobe-type-tools.github.io/font-tech-notes/pdfs/5004.AFM_Spec.pdf
[parser]: ../crates/adobe-font-metrics/src/lib.rs
[parser-tests]: ../crates/adobe-font-metrics/tests/parser.rs
[scope-tests]: ../crates/adobe-font-metrics/tests/scope.rs
[baker]: ../crates/pdf-base14-metrics/build.rs
