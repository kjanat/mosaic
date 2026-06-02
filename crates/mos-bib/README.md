# mos-bib

Bibliography records for Mosaic.

This crate owns the bibliography domain boundary from `manifest.md` §12. It provides a **minimal
BibTeX record parser**: it reads a `.bib` string into typed records that a citation resolver can
build on. The scope is deliberately a small, well-defined BibTeX subset — entry type, citation key,
and string fields — and within that subset the parser is complete and does not panic. It is **not**
a full bibliography engine; styling, resolution, and rendering are separate concerns (see below).

## API

- `parse_bibtex(input: &str) -> Result<Bibliography, BibParseError>` — parse a string.
- `Bibliography { entries: BTreeMap<String, BibEntry> }` — parsed entries keyed by citation key.
- `BibEntry { entry_type: String, key: String, fields: BTreeMap<String, String> }` — one
  `@type{...}` record.
- `BibParseError` / `BibParseErrorKind` — a local, recoverable parse error carrying a byte offset;
  `to_diagnostic` and `From<BibParseError> for CoreError` bridge into `mos-core` diagnostics.
- `Citation { key: String }` — a document-body citation reference.

```rust
use mos_bib::parse_bibtex;

let bib = parse_bibtex("@article{knuth1984, title = {Literate Programming}, year = 1984}")
    .expect("valid BibTeX");
let entry = &bib.entries["knuth1984"];
assert_eq!(entry.entry_type, "article");
assert_eq!(entry.fields["title"], "Literate Programming");
assert_eq!(entry.fields["year"], "1984");
```

## What the parser accepts

- Zero or more `@type{key, field = value, ...}` entries (any entry type, e.g. `@article`), separated
  by whitespace.
- Field values as `{braced}`, `"quoted"`, or bare tokens (e.g. `year = 1984`).
- Comma-separated fields, with an optional trailing comma before the closing `}`.
- **Case handling:** entry types and field names are normalized to lowercase (BibTeX treats them
  case-insensitively); citation keys are preserved verbatim (keys are case-sensitive).
- **Ordering:** entries and fields are stored in `BTreeMap`s, so iteration is deterministic (sorted)
  and stable across runs. Duplicate citation keys are rejected; repeated field names within an entry
  keep the last value.
- Brace values balance nested `{}` by naive counting, so `{The {LaTeX} Companion}` is captured
  whole. Value text is stored **verbatim** (no decoding).
- Panic-free, useful errors for malformed input: a missing `@`, entry type, `{`, citation key, `=`,
  or value; a duplicate citation key; an unterminated brace/quote value; or a missing separator.
  Each `BibParseError` carries the byte offset where it was detected.

## Boundary

- Depends only on `mos-core`.
- Stays close to core model types until real integration needs more.
- Owns bibliography/citation data modeling.
- Does not parse `.mos` syntax, lower documents, lay out pages, or emit backend output.
- Must not depend on `mos-parse` or `mos-eval`.

## Out of scope (separate features)

These are distinct capabilities, not unfinished parts of this parser. They live elsewhere when they
land:

- Full BibTeX, BibLaTeX, or CSL parsing and styling.
- `@string`, `@preamble`, and `@comment` directives, and `#` string concatenation.
- TeX/LaTeX decoding, accent handling, and author-name parsing.
- Reading `.bib` files from disk (this crate parses strings).
- Citation resolution, ordering, clustering, formatting, sorting, and rendering.
- Integration with `mos check`, `mos build`, layout, PDF, HTML, or LSP.

This crate does not claim manifest §12 is complete. Where the manifest and the code disagree, trust
the code.
