# MOS CSL KNOWLEDGE BASE

## OVERVIEW

`mos-csl` holds the CSL (Citation Style Language) data foundations: a typed item data model, a
BibTeX → CSL mapping, and a CSL 1.0.2 XML style parser. It is **not** a CSL processor — there is no
evaluation of a style against data, and no wiring into the compiler pipeline.

## CURRENT SCOPE

Implemented:

- Item data model (`item.rs`): `Item`, `ItemType`, and the standard/number/date/name variable
  vocabularies (spec Appendices III–IV, including deprecated standard variable `event`), plus `Name`
  and `Date`/`DateParts`. A `csl_vocab!` macro generates each vocabulary's
  `as_str`/`from_csl`/`Display`. Ordered enums + `BTreeMap`s for deterministic iteration.
- BibTeX mapping (`from_bibtex.rs`): infallible `item_from_bib_entry` / `library_from_bibliography`.
- Style parser (`parser.rs` + `style.rs`): `parse_style(&str) -> Result<Style, CslParseError>`, a
  read-only `roxmltree` DOM walk into the `Style` AST; local error bridges to `MOS0044`. It retains
  selected rendering options, dependent-style info links, name-part formatting, and raw in-style
  locale blocks for future processing, but does not evaluate them.

Not implemented:

- The CSL processor: style evaluation, formatting, sorting, disambiguation, name ordering, terms,
  ordinals, date rendering.
- Locale files / locale fallback; retained in-style `<locale>` blocks are not interpreted.
- Full BibTeX name parsing (protected institutional names, particles/suffixes); `month` mapping.
- Disk I/O and any `mos-eval`/layout/PDF/LSP integration.

## WHERE TO LOOK

| Task         | Location             | Notes                                                         |
| ------------ | -------------------- | ------------------------------------------------------------- |
| Item model   | `src/item.rs`        | `Item`, `ItemType`, variable enums, `Name`, `Date`.           |
| BibTeX map   | `src/from_bibtex.rs` | `BibEntry`/`Bibliography` → `Item`; tables + name/year rules. |
| Style AST    | `src/style.rs`       | `Style`, `Element`, `Common`, and element structs.            |
| Style parser | `src/parser.rs`      | `roxmltree` walk; dispatch on local element names.            |
| Error type   | `src/error.rs`       | `CslParseError`; offset/`line_col`; `MOS0044` bridge.         |
| Facade       | `src/lib.rs`         | Module wiring and re-exports only.                            |
| Tests        | `tests/style.rs`     | Black-box style-parser tests; mapping tests are unit tests.   |

## CONVENTIONS

- Keep the public API the three pieces: item model, BibTeX mapping, `parse_style` + the `Style` AST.
- `parse_style` returns the local `CslParseError`, but bridge it to a `mos-core` `Diagnostic`
  (`MOS0044`) via `to_diagnostic` / `From`. Do not change the signature to return `CoreError`.
- Use `BTreeMap` and ordered enums so output and tests stay deterministic.
- Dispatch the parser on element *local* names, but require the `<style>` root to be in the CSL
  namespace or none (reject a foreign namespace). Retain modelled rendering options as raw strings;
  retain in-style `<locale>` as raw XML; ignore other unmodelled attributes; error on unknown
  rendering elements. Reject unsupported style versions, `<text>` elements with multiple source
  selectors, and invalid `<choose>` branch order.
- Tests return `()` and use `expect`/`expect_err` — the workspace enables
  `clippy::panic_in_result_fn`, so `Result`-returning tests with `assert!` are clippy errors.

## ANTI-PATTERNS

- Do not depend on `mos-parse` or `mos-eval`.
- Do not grow a CSL processor/renderer in this crate's parser slice.
- Do not parse locale files or implement locale fallback here yet.
- Do not claim manifest §12 is complete; trust the code over the manifest.
