# MOS BIB KNOWLEDGE BASE

## OVERVIEW

`mos-bib` parses a minimal BibTeX subset into typed records. It is not a bibliography engine: no
citation resolution, styling, sorting, or rendering, and no wiring into the compiler pipeline.

## CURRENT SCOPE

Implemented:

- `parse_bibtex(&str) -> Result<Bibliography, BibParseError>`: zero or more `@type{...}` entries.
- Values as `{braced}` (naive nested-brace balancing), `"quoted"`, or bare tokens (`year = 1984`),
  stored verbatim.
- Lowercased entry types and field names; verbatim, case-sensitive citation keys.
- Deterministic ordering via `BTreeMap` for both entries and fields; duplicate keys error, repeated
  fields keep the last value.
- Panic-free recovery: `BibParseError` carries a byte offset and bridges via `to_diagnostic` /
  `From<BibParseError> for CoreError`.

Not implemented:

- `@string` / `@preamble` / `@comment`, `#` concatenation, TeX decoding, name parsing.
- Reading `.bib` files from disk, citation-key resolution, and any citation/bibliography rendering.
- Integration with `mos-eval`, layout, PDF, HTML, or LSP.

## WHERE TO LOOK

| Task         | Location          | Notes                                                   |
| ------------ | ----------------- | ------------------------------------------------------- |
| Public types | `src/record.rs`   | `Bibliography`, `BibEntry`, `Citation`.                 |
| Error type   | `src/error.rs`    | `BibParseError` / `BibParseErrorKind`; offset/line_col. |
| Parser       | `src/parser.rs`   | Hand-rolled recursive descent; grammar in module docs.  |
| Facade       | `src/lib.rs`      | Module wiring and re-exports only.                      |
| Tests        | `tests/bibtex.rs` | Black-box, against the public API.                      |

## CONVENTIONS

- Keep the public API small: `parse_bibtex` plus the record/error types.
- `parse_bibtex` returns the local `BibParseError` (issue #66), but it bridges into the standard
  diagnostics surface via `BibParseError::to_diagnostic` and `From<BibParseError> for CoreError`
  (code `MOS0043`). Keep the local type as the parser entry point; don't change `parse_bibtex` to
  return `CoreError` directly.
- Use `BTreeMap` (not `HashMap`) so output and tests stay deterministic.
- No panics on malformed input; return `BibParseError` instead.
- Document the parser's deliberate limits (verbatim values, naive brace counting) honestly.

## ANTI-PATTERNS

- Do not depend on `mos-parse` or `mos-eval`; this crate stays close to `mos-core`.
- Do not add dependencies for this minimal subset (a hand-rolled parser is enough).
- Do not grow toward full BibTeX/BibLaTeX/CSL, macros, or rendering in this crate's parser slice.
- Do not claim manifest §12 is complete; trust the code over the manifest.
