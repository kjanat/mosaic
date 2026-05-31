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
- **Severity is rendered, not encoded.** The CLI prints the instance severity as `error[MOS0026]`,
  `warning[MOS0016]`, `notice[MOS0034]`. A future config layer remaps the instance severity without
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

Grouped by `DiagnosticCategory` for human scanning. Numeric order has no meaning — a code's number
is just an opaque key, allocated as the next free integer when the code was added.

### Syntax

| Code    | Slug                       | Default severity | Owner crate | Summary                                                          |
| ------- | -------------------------- | ---------------- | ----------- | ---------------------------------------------------------------- |
| MOS0010 | set-missing-identifier     | Error            | mos-parse   | syntax: #set not followed by an identifier                       |
| MOS0011 | directive-missing-paren    | Error            | mos-parse   | syntax: directive missing opening parenthesis                    |
| MOS0012 | directive-unterminated     | Error            | mos-parse   | syntax: unterminated directive block                             |
| MOS0013 | directive-trailing-content | Error            | mos-parse   | syntax: unexpected trailing content after directive              |
| MOS0014 | directive-malformed-arg    | Error            | mos-parse   | syntax: malformed directive argument value                       |
| MOS0015 | arglist-shape              | Error            | mos-parse   | syntax: malformed argument list                                  |
| MOS0016 | unterminated-strong        | Warning          | mos-parse   | syntax: unterminated **strong** run; treated as text             |
| MOS0017 | unterminated-emphasis      | Warning          | mos-parse   | syntax: unterminated *emphasis* run; treated as text             |
| MOS0018 | unterminated-code          | Warning          | mos-parse   | syntax: unterminated `code` run; treated as text                 |
| MOS0019 | stray-at-sign              | Warning          | mos-parse   | syntax: stray @ not followed by a label; treated as text         |
| MOS0020 | lone-trailing-backslash    | Warning          | mos-parse   | syntax: lone trailing backslash at end of input; treated as text |

### Semantic

| Code    | Slug                    | Default severity | Owner crate | Summary                                                        |
| ------- | ----------------------- | ---------------- | ----------- | -------------------------------------------------------------- |
| MOS0021 | set-unknown-target      | Error            | mos-eval    | semantic: unknown #set target                                  |
| MOS0022 | unknown-kwarg           | Error            | mos-eval    | semantic: unknown keyword argument                             |
| MOS0023 | arg-type-mismatch       | Error            | mos-eval    | semantic: argument type mismatch or non-positive length        |
| MOS0024 | set-positional-rejected | Error            | mos-eval    | semantic: #set rejects positional argument                     |
| MOS0025 | set-sanity-floor        | Warning          | mos-eval    | semantic: #set value trips a sanity floor; value still applied |
| MOS0026 | label-duplicate         | Error            | mos-eval    | semantic: label declared more than once                        |
| MOS0027 | label-missing           | Error            | mos-eval    | semantic: @reference to a label that does not exist            |
| MOS0028 | image-missing-path      | Error            | mos-eval    | semantic: #image/#figure missing a path argument               |

### Layout

| Code    | Slug                    | Default severity | Owner crate | Summary                                                      |
| ------- | ----------------------- | ---------------- | ----------- | ------------------------------------------------------------ |
| MOS0031 | paper-size-unknown      | Error            | mos-layout  | layout: unknown paper size                                   |
| MOS0032 | geometry-breaks-page    | Error            | mos-layout  | layout: value breaks page geometry; previous value retained  |
| MOS0033 | image-skipped-no-pixels | Warning          | mos-layout  | layout: image reached layout without decoded pixels; skipped |

### Text

| Code    | Slug                    | Default severity | Owner crate | Summary                                                     |
| ------- | ----------------------- | ---------------- | ----------- | ----------------------------------------------------------- |
| MOS0034 | font-family-substituted | Notice           | mos-fonts   | text: substituted bundled Noto Sans for unknown font family |
| MOS0035 | glyph-budget-exhausted  | Warning          | mos-pdf     | text: Base-14 /Differences glyph budget exhausted           |

### Pdf

| Code    | Slug               | Default severity | Owner crate | Summary                                           |
| ------- | ------------------ | ---------------- | ----------- | ------------------------------------------------- |
| MOS0036 | pdf-io-failed      | Error            | mos-pdf     | pdf: backend I/O failure                          |
| MOS0037 | font-subset-failed | Error            | mos-pdf     | pdf: font subsetting failure for an embedded face |

### Io

| Code    | Slug                | Default severity | Owner crate | Summary                                 |
| ------- | ------------------- | ---------------- | ----------- | --------------------------------------- |
| MOS0029 | image-read-failed   | Error            | mos-eval    | io: image file cannot be read from disk |
| MOS0030 | image-decode-failed | Error            | mos-eval    | io: image file cannot be decoded        |

### Internal

| Code    | Slug                       | Default severity | Owner crate | Summary                                               |
| ------- | -------------------------- | ---------------- | ----------- | ----------------------------------------------------- |
| MOS0038 | internal-missing-font-plan | Error            | mos-pdf     | internal: missing embedded font plan for a shaped run |

## CLI rendering

`mos check` and `mos build` render every diagnostic through
`crates/mos/src/main.rs::render_diagnostic`:

```text
error[MOS0026]: label `intro` is declared more than once
  --> main.mos:3:1
   |
 3 | = B <intro>
   | ^^^^^^^^^^^
  note: first declaration of `intro` is here (main.mos:1:1)
notice[MOS0034]: substituted bundled Noto Sans for unknown family `Helvetica`
```

The leading word is the instance severity; the bracketed token is `diagnostic.def().code()`.
Attached `Related` spans render as `note: … (file:line:col)`; `Note` / `Help` / `Hint` annotations
render as `note:` / `help:` / `hint:` rows.
