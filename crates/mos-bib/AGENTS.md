# MOS BIB KNOWLEDGE BASE

## OVERVIEW

`mos-bib` parses a minimal BibTeX subset into typed records. It is not a bibliography engine: no
styling, sorting, or rendering. Compiler file loading and citation-key resolution live in
`mos-eval`, which consumes this parser.

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
- `bibliography_content_hash(&[u8]) -> ContentHash` (`src/content.rs`): the §4.1 source-hash
  boundary specialized to `.bib` inputs (engine-version + domain-tag + raw bytes, length-framed,
  FNV-1a-128 *interim* hasher). Deterministic, byte-for-byte, no filesystem inputs. Pairs with
  `mos_cache::BibliographyDependency`. See `docs/incremental-dependencies.md` §4.1.

Not implemented:

- `@string` / `@preamble` / `@comment`, `#` concatenation, TeX decoding, name parsing.
- Citation/bibliography rendering, CSL styling, and sorted bibliography output.
- Direct layout, PDF, HTML, or LSP integration. `mos-eval` is the compiler integration point for
  reading `.bib` files and checking citation keys.

## WHERE TO LOOK

| Task         | Location          | Notes                                                    |
| ------------ | ----------------- | -------------------------------------------------------- |
| Public types | `src/record.rs`   | `Bibliography`, `BibEntry`, `Citation`.                  |
| Error type   | `src/error.rs`    | `BibParseError` / `BibParseErrorKind`; offset/line_col.  |
| Parser       | `src/parser.rs`   | Hand-rolled recursive descent; grammar in module docs.   |
| Content hash | `src/content.rs`  | `bibliography_content_hash`; §4.1 boundary, interim FNV. |
| Facade       | `src/lib.rs`      | Module wiring and re-exports only.                       |
| Tests        | `tests/bibtex.rs` | Black-box, against the public API.                       |

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
- Do not move file I/O or citation resolution into this crate; keep those in `mos-eval`.
- Do not claim manifest §12 is complete; trust the code over the manifest.
