# Mosaic diagnostic code catalog

Authoritative mirror of the diagnostic registry in
[`crates/mos-core/src/codes.rs`](../crates/mos-core/src/codes.rs). That file, not this document, is
the source of truth; a drift test (`crates/mos/tests/catalog.rs`) fails CI if the two disagree.

## The contract

- **Semantic IDs are canonical.** Each rule has a fixed `namespace.kebab-case-slug` ID, such as
  `semantic.label-missing`. The namespace is assigned explicitly in the registry; changing category,
  severity, or owner does not change either identity.
- **Numeric codes remain stable compatibility aliases.** `MOS####` numbers are opaque and never
  reused. Existing Rust constants, `DiagnosticCode`, `code()`, and `slug()` remain available.
  `codes::lookup` accepts the exact semantic ID or numeric alias and returns the same definition.
- **Codes are minted in one place.** The registry macro registers both identities, metadata, and
  summary. Registry tests enforce uniqueness and catalog tests enforce documentation parity.
- **Severity is separate.** The CLI prints `error[semantic.label-missing (MOS0033)]`. LSP `code`
  holds the semantic ID, `data.legacyCode` carries the alias, and `codeDescription.href` links to an
  explicit anchor in this catalog.

See [the migration decision](semantic-diagnostic-identifiers.md) for compatibility and naming rules.

## Severities

| Severity  | Meaning                                                             |
| --------- | ------------------------------------------------------------------- |
| `Error`   | Failing. The CLI exits non-zero at the next phase barrier.          |
| `Warning` | Surfaced, but the build continues.                                  |
| `Notice`  | Informational (substitutions, auto-decisions). The build continues. |

`note` / `help` / `hint` are *not* severities: they are `DiagnosticAnnotation` sub-message kinds
attached to a diagnostic, alongside `Related` (a secondary span).

## Codes

Grouped by `DiagnosticCategory` for human scanning. Numeric order has no meaning: current numbers
are intentionally interleaved across categories, and a code's number is just an opaque key. Future
codes use the next free integer.

### Syntax

| Alias   | Semantic ID                                                                     | Slug                       | Default severity | Owner crate | Summary                                                                    |
| ------- | ------------------------------------------------------------------------------- | -------------------------- | ---------------- | ----------- | -------------------------------------------------------------------------- |
| MOS0010 | <a id="syntax.set-missing-identifier"></a>syntax.set-missing-identifier         | set-missing-identifier     | Error            | mos-parse   | syntax: #set not followed by an identifier                                 |
| MOS0013 | <a id="syntax.directive-missing-paren"></a>syntax.directive-missing-paren       | directive-missing-paren    | Error            | mos-parse   | syntax: directive missing opening parenthesis                              |
| MOS0016 | <a id="syntax.directive-unterminated"></a>syntax.directive-unterminated         | directive-unterminated     | Error            | mos-parse   | syntax: unterminated directive block                                       |
| MOS0019 | <a id="syntax.directive-trailing-content"></a>syntax.directive-trailing-content | directive-trailing-content | Error            | mos-parse   | syntax: unexpected trailing content after directive                        |
| MOS0022 | <a id="syntax.directive-malformed-arg"></a>syntax.directive-malformed-arg       | directive-malformed-arg    | Error            | mos-parse   | syntax: malformed directive argument value                                 |
| MOS0025 | <a id="syntax.arglist-shape"></a>syntax.arglist-shape                           | arglist-shape              | Error            | mos-parse   | syntax: malformed argument list                                            |
| MOS0028 | <a id="syntax.unterminated-strong"></a>syntax.unterminated-strong               | unterminated-strong        | Warning          | mos-parse   | syntax: unterminated **strong** run; treated as text                       |
| MOS0031 | <a id="syntax.unterminated-emphasis"></a>syntax.unterminated-emphasis           | unterminated-emphasis      | Warning          | mos-parse   | syntax: unterminated *emphasis* run; treated as text                       |
| MOS0034 | <a id="syntax.unterminated-code"></a>syntax.unterminated-code                   | unterminated-code          | Warning          | mos-parse   | syntax: unterminated `code` run; treated as text                           |
| MOS0036 | <a id="syntax.stray-at-sign"></a>syntax.stray-at-sign                           | stray-at-sign              | Warning          | mos-parse   | syntax: stray @ not followed by a label; treated as text                   |
| MOS0038 | <a id="syntax.lone-trailing-backslash"></a>syntax.lone-trailing-backslash       | lone-trailing-backslash    | Warning          | mos-parse   | syntax: lone trailing backslash at end of input; treated as text           |
| MOS0039 | <a id="syntax.malformed-citation"></a>syntax.malformed-citation                 | malformed-citation         | Warning          | mos-parse   | syntax: malformed citation group; treated as text                          |
| MOS0043 | <a id="syntax.bibtex-parse-failed"></a>syntax.bibtex-parse-failed               | bibtex-parse-failed        | Error            | mos-bib     | syntax: BibTeX database could not be parsed                                |
| MOS0044 | <a id="syntax.csl-parse-failed"></a>syntax.csl-parse-failed                     | csl-parse-failed           | Error            | mos-csl     | syntax: CSL style could not be parsed                                      |
| MOS0048 | <a id="syntax.heading-label-not-trailing"></a>syntax.heading-label-not-trailing | heading-label-not-trailing | Warning          | mos-parse   | syntax: heading label is not the last element on the line; treated as text |
| MOS0050 | <a id="syntax.unterminated-block-comment"></a>syntax.unterminated-block-comment | unterminated-block-comment | Warning          | mos-parse   | syntax: unterminated /* block comment; consumed to end of input            |

### Resolution

| Alias   | Semantic ID                                                                           | Slug                        | Default severity | Owner crate | Summary                                                                       |
| ------- | ------------------------------------------------------------------------------------- | --------------------------- | ---------------- | ----------- | ----------------------------------------------------------------------------- |
| MOS0011 | <a id="semantic.set-unknown-target"></a>semantic.set-unknown-target                   | set-unknown-target          | Error            | mos-eval    | resolution: unknown #set target                                               |
| MOS0015 | <a id="semantic.unknown-kwarg"></a>semantic.unknown-kwarg                             | unknown-kwarg               | Error            | mos-eval    | resolution: unknown keyword argument                                          |
| MOS0020 | <a id="semantic.arg-type-mismatch"></a>semantic.arg-type-mismatch                     | arg-type-mismatch           | Error            | mos-eval    | resolution: argument type mismatch or non-positive length                     |
| MOS0024 | <a id="semantic.set-positional-rejected"></a>semantic.set-positional-rejected         | set-positional-rejected     | Error            | mos-eval    | resolution: #set rejects positional argument                                  |
| MOS0027 | <a id="semantic.set-sanity-floor"></a>semantic.set-sanity-floor                       | set-sanity-floor            | Warning          | mos-eval    | resolution: #set value trips a sanity floor; value still applied              |
| MOS0030 | <a id="semantic.label-duplicate"></a>semantic.label-duplicate                         | label-duplicate             | Error            | mos-eval    | resolution: label declared more than once                                     |
| MOS0033 | <a id="semantic.label-missing"></a>semantic.label-missing                             | label-missing               | Error            | mos-eval    | resolution: @reference to a label that does not exist                         |
| MOS0037 | <a id="semantic.image-missing-path"></a>semantic.image-missing-path                   | image-missing-path          | Error            | mos-eval    | resolution: #image/#figure missing a path argument                            |
| MOS0040 | <a id="semantic.bibliography-missing-path"></a>semantic.bibliography-missing-path     | bibliography-missing-path   | Error            | mos-eval    | resolution: #bibliography missing a path argument                             |
| MOS0042 | <a id="semantic.bibliography-duplicate-path"></a>semantic.bibliography-duplicate-path | bibliography-duplicate-path | Error            | mos-eval    | resolution: #bibliography path argument declared more than once               |
| MOS0045 | <a id="semantic.citation-missing"></a>semantic.citation-missing                       | citation-missing            | Error            | mos-eval    | resolution: citation key does not exist in bibliography records               |
| MOS0046 | <a id="semantic.bibliography-duplicate-key"></a>semantic.bibliography-duplicate-key   | bibliography-duplicate-key  | Error            | mos-eval    | resolution: citation key appears in more than one bibliography source         |
| MOS0049 | <a id="semantic.path-unsafe-segment"></a>semantic.path-unsafe-segment                 | path-unsafe-segment         | Error            | mos-eval    | resolution: path segment is not a portable name (manifest paths use `/` only) |

### Layout

| Alias   | Semantic ID                                                                         | Slug                         | Default severity | Owner crate | Summary                                                                   |
| ------- | ----------------------------------------------------------------------------------- | ---------------------------- | ---------------- | ----------- | ------------------------------------------------------------------------- |
| MOS0017 | <a id="layout.paper-size-unknown"></a>layout.paper-size-unknown                     | paper-size-unknown           | Error            | mos-layout  | layout: unknown paper size                                                |
| MOS0023 | <a id="layout.geometry-breaks-page"></a>layout.geometry-breaks-page                 | geometry-breaks-page         | Error            | mos-layout  | layout: value breaks page geometry; previous value retained               |
| MOS0035 | <a id="layout.image-skipped-no-pixels"></a>layout.image-skipped-no-pixels           | image-skipped-no-pixels      | Warning          | mos-layout  | layout: image reached layout without decoded pixels; skipped              |
| MOS0047 | <a id="layout.page-fixpoint-nonconvergence"></a>layout.page-fixpoint-nonconvergence | page-fixpoint-nonconvergence | Warning          | mos-eval    | layout: page references did not converge; last computed page numbers used |

### Text

| Alias   | Semantic ID                                                           | Slug                    | Default severity | Owner crate | Summary                                                     |
| ------- | --------------------------------------------------------------------- | ----------------------- | ---------------- | ----------- | ----------------------------------------------------------- |
| MOS0018 | <a id="text.font-family-substituted"></a>text.font-family-substituted | font-family-substituted | Notice           | mos-fonts   | text: substituted bundled Noto Sans for unknown font family |
| MOS0032 | <a id="text.glyph-budget-exhausted"></a>text.glyph-budget-exhausted   | glyph-budget-exhausted  | Warning          | mos-pdf     | text: Base-14 /Differences glyph budget exhausted           |

### Pdf

| Alias   | Semantic ID                                               | Slug               | Default severity | Owner crate | Summary                                           |
| ------- | --------------------------------------------------------- | ------------------ | ---------------- | ----------- | ------------------------------------------------- |
| MOS0014 | <a id="pdf.pdf-io-failed"></a>pdf.pdf-io-failed           | pdf-io-failed      | Error            | mos-pdf     | pdf: backend I/O failure                          |
| MOS0026 | <a id="pdf.font-subset-failed"></a>pdf.font-subset-failed | font-subset-failed | Error            | mos-pdf     | pdf: font subsetting failure for an embedded face |

### Io

| Alias   | Semantic ID                                                               | Slug                        | Default severity | Owner crate | Summary                                         |
| ------- | ------------------------------------------------------------------------- | --------------------------- | ---------------- | ----------- | ----------------------------------------------- |
| MOS0012 | <a id="io.image-read-failed"></a>io.image-read-failed                     | image-read-failed           | Error            | mos-eval    | io: image file cannot be read from disk         |
| MOS0029 | <a id="io.image-decode-failed"></a>io.image-decode-failed                 | image-decode-failed         | Error            | mos-eval    | io: image file cannot be decoded                |
| MOS0041 | <a id="io.bibliography-source-missing"></a>io.bibliography-source-missing | bibliography-source-missing | Warning          | mos-eval    | io: declared bibliography source file not found |

### Internal

| Alias   | Semantic ID                                                                                 | Slug                           | Default severity | Owner crate | Summary                                                    |
| ------- | ------------------------------------------------------------------------------------------- | ------------------------------ | ---------------- | ----------- | ---------------------------------------------------------- |
| MOS0021 | <a id="internal.internal-missing-font-plan"></a>internal.internal-missing-font-plan         | internal-missing-font-plan     | Error            | mos-pdf     | internal: missing embedded font plan for a shaped run      |
| MOS0051 | <a id="internal.internal-debug-layout-mismatch"></a>internal.internal-debug-layout-mismatch | internal-debug-layout-mismatch | Error            | mos-pdf     | internal: debug layout report does not match page geometry |

## CLI rendering

`mos check` and `mos build` render every diagnostic through
`crates/mos/src/main.rs::render_diagnostic`:

```console
error[semantic.label-duplicate (MOS0030)]: label `intro` is declared more than once
  --> main.mos:3:1
   |
 3 | = B <intro>
   | ^^^^^^^^^^^
  note: first declaration of `intro` is here (main.mos:1:1)
notice[text.font-family-substituted (MOS0018)]: substituted bundled Noto Sans for unknown family `Helvetica`
```

The leading word is the instance severity; the bracketed text is the semantic ID followed by its
numeric alias. Attached `Related` spans render as `note: … (file:line:col)`; `Note` / `Help` /
`Hint` annotations render as `note:` / `help:` / `hint:` rows.
