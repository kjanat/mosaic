# Mosaic diagnostic code catalog

Authoritative mirror of the diagnostic registry in
[`crates/mos-core/src/codes.rs`](../crates/mos-core/src/codes.rs). That file — not this document —
is the source of truth; a drift test (`crates/mos/tests/catalog.rs`) fails CI if the two disagree.

## The contract

- **Identity is opaque.** A diagnostic code is a stable, namespaced, severity-free identifier
  rendered as `MOS0010`. The number is just a number — it does **not** encode severity, owner crate,
  category, phase, or lint group. Numbers are globally unique and stable; new codes get the next
  free integer regardless of what they describe.
- **Severity, category, owner, and summary are metadata** on `DiagnosticDef`. The catalog groups by
  metadata (this document organises by category for human scanning), never by numeric range — so a
  rule that moves phase (parser → eval, fonts → text shaping) keeps its stable ID and just updates
  its `category`.
- **Codes are minted in one place.** `DiagnosticCode` and `DiagnosticDef` have private fields and
  crate-private constructors; the `define_codes!` macro is the only mint site, so no crate can forge
  a code or disagree with its registered severity.
- **Severity is rendered, not encoded.** The CLI prints the instance severity as `error[MOS0030]`,
  `warning[MOS0028]`, `notice[MOS0018]`. A future config layer remaps the instance severity without
  touching identity.

## Severities

| Severity  | Meaning                                                             |
| --------- | ------------------------------------------------------------------- |
| `Error`   | Failing. The CLI exits non-zero at the next phase barrier.          |
| `Warning` | Surfaced, but the build continues.                                  |
| `Notice`  | Informational (substitutions, auto-decisions). The build continues. |

`note` / `help` / `hint` are *not* severities — they are `DiagnosticAnnotation` sub-message kinds
attached to a diagnostic, alongside `Related` (a secondary span).

## Codes

Grouped by `DiagnosticCategory` for human scanning. Numeric order has no meaning — current numbers
are intentionally interleaved across categories, and a code's number is just an opaque key. Future
codes use the next free integer.

### Syntax

| Code    | Slug                       | Default severity | Owner crate | Summary                                                                    |
| ------- | -------------------------- | ---------------- | ----------- | -------------------------------------------------------------------------- |
| MOS0010 | set-missing-identifier     | Error            | mos-parse   | syntax: #set not followed by an identifier                                 |
| MOS0013 | directive-missing-paren    | Error            | mos-parse   | syntax: directive missing opening parenthesis                              |
| MOS0016 | directive-unterminated     | Error            | mos-parse   | syntax: unterminated directive block                                       |
| MOS0019 | directive-trailing-content | Error            | mos-parse   | syntax: unexpected trailing content after directive                        |
| MOS0022 | directive-malformed-arg    | Error            | mos-parse   | syntax: malformed directive argument value                                 |
| MOS0025 | arglist-shape              | Error            | mos-parse   | syntax: malformed argument list                                            |
| MOS0028 | unterminated-strong        | Warning          | mos-parse   | syntax: unterminated **strong** run; treated as text                       |
| MOS0031 | unterminated-emphasis      | Warning          | mos-parse   | syntax: unterminated *emphasis* run; treated as text                       |
| MOS0034 | unterminated-code          | Warning          | mos-parse   | syntax: unterminated `code` run; treated as text                           |
| MOS0036 | stray-at-sign              | Warning          | mos-parse   | syntax: stray @ not followed by a label; treated as text                   |
| MOS0038 | lone-trailing-backslash    | Warning          | mos-parse   | syntax: lone trailing backslash at end of input; treated as text           |
| MOS0039 | malformed-citation         | Warning          | mos-parse   | syntax: malformed citation group; treated as text                          |
| MOS0043 | bibtex-parse-failed        | Error            | mos-bib     | syntax: BibTeX database could not be parsed                                |
| MOS0044 | csl-parse-failed           | Error            | mos-csl     | syntax: CSL style could not be parsed                                      |
| MOS0048 | heading-label-not-trailing | Warning          | mos-parse   | syntax: heading label is not the last element on the line; treated as text |

### Semantic

| Code    | Slug                        | Default severity | Owner crate | Summary                                                             |
| ------- | --------------------------- | ---------------- | ----------- | ------------------------------------------------------------------- |
| MOS0011 | set-unknown-target          | Error            | mos-eval    | semantic: unknown #set target                                       |
| MOS0015 | unknown-kwarg               | Error            | mos-eval    | semantic: unknown keyword argument                                  |
| MOS0020 | arg-type-mismatch           | Error            | mos-eval    | semantic: argument type mismatch or non-positive length             |
| MOS0024 | set-positional-rejected     | Error            | mos-eval    | semantic: #set rejects positional argument                          |
| MOS0027 | set-sanity-floor            | Warning          | mos-eval    | semantic: #set value trips a sanity floor; value still applied      |
| MOS0030 | label-duplicate             | Error            | mos-eval    | semantic: label declared more than once                             |
| MOS0033 | label-missing               | Error            | mos-eval    | semantic: @reference to a label that does not exist                 |
| MOS0037 | image-missing-path          | Error            | mos-eval    | semantic: #image/#figure missing a path argument                    |
| MOS0040 | bibliography-missing-path   | Error            | mos-eval    | semantic: #bibliography missing a path argument                     |
| MOS0042 | bibliography-duplicate-path | Error            | mos-eval    | semantic: #bibliography path argument declared more than once       |
| MOS0045 | citation-missing            | Error            | mos-eval    | semantic: citation key does not exist in bibliography records       |
| MOS0046 | bibliography-duplicate-key  | Error            | mos-eval    | semantic: citation key appears in more than one bibliography source |

### Layout

| Code    | Slug                         | Default severity | Owner crate | Summary                                                                   |
| ------- | ---------------------------- | ---------------- | ----------- | ------------------------------------------------------------------------- |
| MOS0017 | paper-size-unknown           | Error            | mos-layout  | layout: unknown paper size                                                |
| MOS0023 | geometry-breaks-page         | Error            | mos-layout  | layout: value breaks page geometry; previous value retained               |
| MOS0035 | image-skipped-no-pixels      | Warning          | mos-layout  | layout: image reached layout without decoded pixels; skipped              |
| MOS0047 | page-fixpoint-nonconvergence | Warning          | mos-eval    | layout: page references did not converge; last computed page numbers used |

### Text

| Code    | Slug                    | Default severity | Owner crate | Summary                                                     |
| ------- | ----------------------- | ---------------- | ----------- | ----------------------------------------------------------- |
| MOS0018 | font-family-substituted | Notice           | mos-fonts   | text: substituted bundled Noto Sans for unknown font family |
| MOS0032 | glyph-budget-exhausted  | Warning          | mos-pdf     | text: Base-14 /Differences glyph budget exhausted           |

### Pdf

| Code    | Slug               | Default severity | Owner crate | Summary                                           |
| ------- | ------------------ | ---------------- | ----------- | ------------------------------------------------- |
| MOS0014 | pdf-io-failed      | Error            | mos-pdf     | pdf: backend I/O failure                          |
| MOS0026 | font-subset-failed | Error            | mos-pdf     | pdf: font subsetting failure for an embedded face |

### Io

| Code    | Slug                        | Default severity | Owner crate | Summary                                         |
| ------- | --------------------------- | ---------------- | ----------- | ----------------------------------------------- |
| MOS0012 | image-read-failed           | Error            | mos-eval    | io: image file cannot be read from disk         |
| MOS0029 | image-decode-failed         | Error            | mos-eval    | io: image file cannot be decoded                |
| MOS0041 | bibliography-source-missing | Warning          | mos-eval    | io: declared bibliography source file not found |

### Internal

| Code    | Slug                       | Default severity | Owner crate | Summary                                               |
| ------- | -------------------------- | ---------------- | ----------- | ----------------------------------------------------- |
| MOS0021 | internal-missing-font-plan | Error            | mos-pdf     | internal: missing embedded font plan for a shaped run |

## CLI rendering

`mos check` and `mos build` render every diagnostic through
`crates/mos/src/main.rs::render_diagnostic`:

```console
error[MOS0030]: label `intro` is declared more than once
  --> main.mos:3:1
   |
 3 | = B <intro>
   | ^^^^^^^^^^^
  note: first declaration of `intro` is here (main.mos:1:1)
notice[MOS0018]: substituted bundled Noto Sans for unknown family `Helvetica`
```

The leading word is the instance severity; the bracketed token is `diagnostic.def().code()`.
Attached `Related` spans render as `note: … (file:line:col)`; `Note` / `Help` / `Hint` annotations
render as `note:` / `help:` / `hint:` rows.
