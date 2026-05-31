# Mosaic diagnostic code catalog

Authoritative mirror of the diagnostic registry in
[`crates/mos-core/src/codes.rs`](../crates/mos-core/src/codes.rs). That file — not this document —
is the source of truth; a drift test (`crates/mos/tests/catalog.rs`) fails CI if the two disagree.

## The contract

- **Identity is separate from severity.** A diagnostic code is a stable, namespaced, severity-free
  identifier rendered as `MOS0010`. It answers *which rule fired*, never *how hard it fails*.
- **Severity is registry metadata**, carried on the `DiagnosticDef` as `default_severity` and on
  each emitted `Diagnostic` instance (so a future config layer can remap it without touching call
  sites). The CLI renders the instance severity: `error[MOS0140]`, `warning[MOS0020]`,
  `notice[MOS0300]`.
- **Codes are minted in one place.** `DiagnosticCode` and `DiagnosticDef` have private fields and
  crate-private constructors; the `define_codes!` macro is the only mint site, so no crate can forge
  a code or disagree with its registered severity.
- **Numbers are globally unique and organised by category**, never by severity. The same number is
  never reused across severities; the 100-block says *what kind* of diagnostic it is, the
  `default_severity` column says *how it fails*.

## Severities

| Severity  | Meaning                                                             |
| --------- | ------------------------------------------------------------------- |
| `Error`   | Failing. The CLI exits non-zero at the next phase barrier.          |
| `Warning` | Surfaced, but the build continues.                                  |
| `Notice`  | Informational (substitutions, auto-decisions). The build continues. |

`note` / `help` / `hint` are *not* severities — they are `DiagnosticAnnotation` sub-message kinds
attached to a diagnostic, alongside `Related` (a secondary span).

## Category ranges

| Range       | Category               | Owners                                |
| ----------- | ---------------------- | ------------------------------------- |
| `0000–0099` | syntax (parse)         | `mos-parse`                           |
| `0100–0199` | semantic (lower/eval)  | `mos-eval`                            |
| `0200–0299` | layout                 | `mos-layout`                          |
| `0300–0399` | text / fonts / shaping | `mos-fonts`, encoding in `mos-pdf`    |
| `0400–0499` | PDF emission           | `mos-pdf`                             |
| `0500–0599` | project / CLI / IO     | `mos`                                 |
| `0600–9999` | reserved               | HTML/EPUB/LSP backends, plugins, etc. |

## Syntax (`0000–0099`)

| Code    | Slug                       | Default severity | Owner crate | Summary                                                          |
| ------- | -------------------------- | ---------------- | ----------- | ---------------------------------------------------------------- |
| MOS0010 | set-missing-identifier     | Error            | mos-parse   | syntax: #set not followed by an identifier                       |
| MOS0011 | directive-missing-paren    | Error            | mos-parse   | syntax: directive missing opening parenthesis                    |
| MOS0012 | directive-unterminated     | Error            | mos-parse   | syntax: unterminated directive block                             |
| MOS0013 | directive-trailing-content | Error            | mos-parse   | syntax: unexpected trailing content after directive              |
| MOS0014 | directive-malformed-arg    | Error            | mos-parse   | syntax: malformed directive argument value                       |
| MOS0015 | arglist-shape              | Error            | mos-parse   | syntax: malformed argument list                                  |
| MOS0020 | unterminated-strong        | Warning          | mos-parse   | syntax: unterminated **strong** run; treated as text             |
| MOS0021 | unterminated-emphasis      | Warning          | mos-parse   | syntax: unterminated *emphasis* run; treated as text             |
| MOS0022 | unterminated-code          | Warning          | mos-parse   | syntax: unterminated `code` run; treated as text                 |
| MOS0023 | stray-at-sign              | Warning          | mos-parse   | syntax: stray @ not followed by a label; treated as text         |
| MOS0024 | lone-trailing-backslash    | Warning          | mos-parse   | syntax: lone trailing backslash at end of input; treated as text |

## Semantic (`0100–0199`)

| Code    | Slug                    | Default severity | Owner crate | Summary                                                        |
| ------- | ----------------------- | ---------------- | ----------- | -------------------------------------------------------------- |
| MOS0100 | set-unknown-target      | Error            | mos-eval    | semantic: unknown #set target                                  |
| MOS0101 | unknown-kwarg           | Error            | mos-eval    | semantic: unknown keyword argument                             |
| MOS0102 | arg-type-mismatch       | Error            | mos-eval    | semantic: argument type mismatch or non-positive length        |
| MOS0103 | set-positional-rejected | Error            | mos-eval    | semantic: #set rejects positional argument                     |
| MOS0120 | set-sanity-floor        | Warning          | mos-eval    | semantic: #set value trips a sanity floor; value still applied |
| MOS0140 | label-duplicate         | Error            | mos-eval    | semantic: label declared more than once                        |
| MOS0141 | label-missing           | Error            | mos-eval    | semantic: @reference to a label that does not exist            |
| MOS0160 | image-missing-path      | Error            | mos-eval    | semantic: #image/#figure missing a path argument               |
| MOS0161 | image-read-failed       | Error            | mos-eval    | semantic: image file cannot be read from disk                  |
| MOS0162 | image-decode-failed     | Error            | mos-eval    | semantic: image file cannot be decoded                         |

## Layout (`0200–0299`)

| Code    | Slug                    | Default severity | Owner crate | Summary                                                      |
| ------- | ----------------------- | ---------------- | ----------- | ------------------------------------------------------------ |
| MOS0200 | paper-size-unknown      | Error            | mos-layout  | layout: unknown paper size                                   |
| MOS0201 | geometry-breaks-page    | Error            | mos-layout  | layout: value breaks page geometry; previous value retained  |
| MOS0220 | image-skipped-no-pixels | Warning          | mos-layout  | layout: image reached layout without decoded pixels; skipped |

## Text / fonts / shaping (`0300–0399`)

| Code    | Slug                    | Default severity | Owner crate | Summary                                                |
| ------- | ----------------------- | ---------------- | ----------- | ------------------------------------------------------ |
| MOS0300 | font-family-substituted | Notice           | mos-fonts   | font: substituted bundled Noto Sans for unknown family |
| MOS0310 | glyph-budget-exhausted  | Warning          | mos-pdf     | font: Base-14 /Differences glyph budget exhausted      |

## PDF emission (`0400–0499`)

| Code    | Slug                       | Default severity | Owner crate | Summary                                                    |
| ------- | -------------------------- | ---------------- | ----------- | ---------------------------------------------------------- |
| MOS0400 | pdf-io-failed              | Error            | mos-pdf     | pdf: backend I/O failure                                   |
| MOS0401 | font-subset-failed         | Error            | mos-pdf     | pdf: font subsetting failure for an embedded face          |
| MOS0402 | internal-missing-font-plan | Error            | mos-pdf     | pdf: internal: missing embedded font plan for a shaped run |

## Reserved / retired

| Code    | Slug    | Default severity | Owner crate | Summary                                       |
| ------- | ------- | ---------------- | ----------- | --------------------------------------------- |
| MOS0000 | example | Error            | mos-core    | reserved documentation example; never emitted |

- `MOS0000` is referenced only by doctests where a concrete code is needed; no compiler stage emits
  it.
- The pre-`MOS` scheme (`E0xx` / `W0xx`) was retired wholesale when identity was split from
  severity. The old `W040` substitution warning has no successor — `mos-layout` keeps a regression
  test asserting uncovered glyphs flow through without a diagnostic.

## CLI rendering

`mos check` and `mos build` render every diagnostic through
`crates/mos/src/main.rs::render_diagnostic`:

```text
error[MOS0140]: label `intro` is declared more than once
  --> main.mos:3:1
   |
 3 | = B <intro>
   | ^^^^^^^^^^^
  note: first declaration of `intro` is here (main.mos:1:1)
notice[MOS0300]: substituted bundled Noto Sans for unknown family `Helvetica`
```

The leading word is the instance severity; the bracketed token is `diagnostic.def().code()`.
Attached `Related` spans render as `note: … (file:line:col)`; `Note` / `Help` / `Hint` annotations
render as `note:` / `help:` / `hint:` rows.
