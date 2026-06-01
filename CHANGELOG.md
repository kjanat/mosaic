# Changelog

All notable changes to this project will be documented here. The format is based on
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/); this project follows
[Semantic Versioning](https://semver.org/spec/v2.0.0.html) but stays at `0.0.0` while pre-alpha.

## [Unreleased]

### Added

- Structured diagnostic suggestions (https://github.com/kjanat/mosaic/issues/63): `mos-core` gains a
  backend-neutral `Suggestion` payload — a `SourceSpan` plus replacement text — that diagnostics can
  carry alongside prose `DiagnosticAnnotation::Help` annotations. `Diagnostic` stores them in a
  private `suggestions` vec exposed through a `with_suggestion` builder and a `suggestions()`
  accessor; an empty replacement encodes deletion and a zero-length span encodes insertion. Emitting
  suggestions from resolver diagnostics and rendering them in the CLI/LSP are out of this slice.
- Nearest-label suggestions for unknown references (https://github.com/kjanat/mosaic/issues/51): the
  resolver now attaches a structured `Suggestion` to `MOS0033` ("unknown label") that rewrites the
  whole `@label` token to the closest existing label (`@intrdo` → `@intro`). Candidates are drawn
  only from the resolver label index and filtered to names spellable as `@` references, so an
  applied fix always parses and resolves. Matching is a local byte Levenshtein with a conservative
  threshold (edit distance within `len / 3`, and nothing suggested below three bytes); ties break
  deterministically on `(distance, label)` and at most one candidate is offered. Rendering
  suggestions in the CLI/LSP stays out of this slice.
- Minimal single-key `[@key]` citation syntax (https://github.com/kjanat/mosaic/issues/47):
  citations parse and lower into semantic placeholder nodes rendered as `[?key?]` until bibliography
  resolution lands. Malformed citation groups recover as literal text with diagnostic `MOS0039`.
- User-facing label/reference documentation (https://github.com/kjanat/mosaic/issues/53):
  [`docs/labels-and-references.md`][docs:labels-and-references] records the shipped declaration
  forms, reference rendering, diagnostics, greedy reference parsing, and page-reference boundary.
- Minimal stdio LSP server (https://github.com/kjanat/mosaic/issues/49): [`mos-lsp`][mos-lsp]
  replaces its stub with a server that publishes compiler parse/lower/resolve diagnostics via
  `textDocument/publishDiagnostics` on open and full change. Source `SourceSpan`s are projected to
  UTF-16 LSP ranges and stable `MOS####` codes are preserved. Pull diagnostics, completion, hover,
  formatting, and workspace indexing are explicitly out of this slice.
- Author-facing inline line-break controls (https://github.com/kjanat/mosaic/issues/26):
  - `\\` hard line break (`InlineKind::HardBreak` / `NodeKind::HardBreak`), flushes the current line
    without paragraph spacing; two in a row produce a blank line; collapses silently at paragraph
    start (block-boundary semantics, regardless of whether prior blocks have painted on the page); a
    lone trailing `\` at end of input emits diagnostic `MOS0038`.
  - `\-` soft-hyphen shorthand expanding to `U+00AD`; SHY codepoints are stripped from the rendered
    text and their byte offsets recorded in `Word.shy_break_offsets`. The greedy line-breaker now
    consumes those offsets: when a word would overflow the line it picks the latest fitting SHY
    position, emits `prefix-` at end of line, and continues with the suffix as the next word. If no
    SHY prefix fits even an empty line, the existing oversized-word cluster fallback still applies.
    Optimal (non-greedy) selection is left for the Knuth-Plass cutover.
  - Non-breaking space `U+00A0` preserved as a cohesive unit by the greedy line-breaker.
- New `WordItem` enum (`Word` / `HardBreak`) replacing the bare `Vec<Word>` stream consumed by
  `flow_words`.
- [`examples/linebreaks/`][ex:linebreaks] project demonstrating all three controls.

### Changed

- Diagnostics gained a registry-backed `MOS####` code system
  (https://github.com/kjanat/mosaic/issues/57): diagnostic identity is split from severity, codes
  are minted only in `mos-core::codes`, and a human-readable catalog
  ([`docs/diagnostic-codes.md`][docs:diagnostic-codes]) is drift-tested against the registry.
  Reporting moves to a sink-based model and parser/eval diagnostic coverage is tightened.
- Resolver ([`crates/mos-eval/src/resolve.rs`][mos-eval:resolve.rs]) now models label targets
  explicitly (https://github.com/kjanat/mosaic/issues/44): the label index is a typed
  `label → LabelTarget` map distinguishing `Section { number }`, `Figure`, and `Generic` targets
  instead of an untyped `label → NodeId` lookup. Section references still render from the captured
  hierarchical counter; figure labels are recognised as a distinct kind (figure numbering and
  kind-aware reference text land in https://github.com/kjanat/mosaic/issues/46, below).
  Duplicate-label (`MOS0030`) and unknown-reference (`MOS0033`) diagnostics are unchanged.
- Resolver ([`crates/mos-eval/src/resolve.rs`][mos-eval:resolve.rs]) now numbers figures and
  resolves figure references (https://github.com/kjanat/mosaic/issues/46): every `NodeKind::Figure`
  receives a deterministic document-order `number` (`1`, `2`, `3`, …, counted independently of
  section numbering); captioned figures get a visible `Figure N: …` label stamped onto the caption;
  and an `@label` reference to a figure renders kind-aware as `Figure N` instead of the bare label.
  The supplement word (`Figure`) comes from a single localization seam and is joined to the number
  with a non-breaking space so the label never wraps off its number. Section references,
  generic-label fallback, and duplicate/unknown diagnostics are unchanged.
- Resolver ([`crates/mos-eval/src/resolve.rs`][mos-eval:resolve.rs]) now attaches a structured
  rename suggestion to duplicate-label diagnostics (https://github.com/kjanat/mosaic/issues/52):
  every `MOS0030` carries a machine-readable `Suggestion` — a deterministic, collision-aware rename
  to the next free `{label}-N` (smallest `N >= 2` not already declared or suggested) over the
  duplicate label token span — building on the `mos-core` `Suggestion` payload above. The existing
  `MOS0030` message, the related first-declaration note, and first-declaration-wins resolution are
  unchanged; emitting the suggestion does not mutate the document, so the resolver stays idempotent.
  Rendering suggestions in the CLI/LSP remains out of this slice.
- Tree-sitter grammar ([`crates/tree-sitter-mosaic`][tree-sitter-mosaic]) realigned with the
  compiler's inline parser for the author-facing line-break controls
  (https://github.com/kjanat/mosaic/issues/26): `\\` now parses as a dedicated `hard_break` node and
  `\-` as `soft_hyphen_escape`, both highlighted under `@string.escape`. The external
  `linebreak_escape` token is removed; a bare `\` that does not form one of the recognised escape
  tokens (typically a trailing `\` before a newline, which the compiler also warns as `MOS0038`)
  parses as `loose_backslash` rather than a structural ERROR, matching the compiler's "literal text"
  treatment. `escaped_char` continues to cover `\#`, `\*`, `\[`, `\]`, `\<`, etc. Mirrored into
  [`crates/zed-mosaic`][zed-mosaic] via `just sync-zed-queries`, and the Zed extension grammar `rev`
  is bumped to pick up the new node types. [`mosaic.ebnf`][mosaic.ebnf] and [`EBNF.md`][EBNF]
  refreshed to match.

## [0.0.0] - 2026-05-22

First tagged pre-alpha. The full crate stack under [`crates/`][crates] (everything except the
[`zed-mosaic`][zed-mosaic] editor extension) is published to crates.io via a resumable release
workflow.

### Added

- `mos check`: parse, lower, and resolve a `.mos` file or project directory, emitting
  source-anchored diagnostics with stable `MOS####` codes, carets, UTF-8-accurate columns, and
  CRLF-aware spans. The CLI applies phase-barrier fail-fast (each phase runs to completion, then
  exits if any error was collected) and gates PDF emission on diagnostic severity.
- `mos build`: render a document to PDF under `build/<entry-stem>.pdf`, or to a project-declared
  `[output].pdf` path. Built PDFs open automatically after a successful build.
- CLI accepts both single `.mos` files and project directories.
- Markup parser: headings, paragraphs, inline emphasis / strong / nested bold-italic, inline and
  multiline code spans, raw code blocks (including long-bracket form with literal bodies and
  preserved tabs and escapes), unordered (`-`) and ordered (`N.`) lists with hanging indent, `#set`,
  `#image`, `#figure`, and cross-reference labels / references.
- Semantic lowering and resolution: a `Document` model with metadata, automatic hierarchical section
  numbering, a cross-reference resolver, and duplicate / unknown-label diagnostics.
- Layout engine: greedy text flow; headings, paragraphs, and lists (with adaptive gutters and
  per-marker shaping for ordered lists); raster images and simple figures with captions; pages,
  paper sizes, and margins; `#set` page / text / document properties wired into layout and PDF
  metadata; NFC text normalization; and oversized-word breaking on shaped glyph clusters.
- Fonts: a zero-dependency Adobe Font Metrics (AFM v4.x) parser; Base-14 metrics; per-glyph font
  fallback; and bundled, subsetted Noto Sans, Noto Sans Mono, and Noto Sans Math for broad Unicode
  coverage beyond the Base-14 cliff.
- PDF backend: WinAnsi plus Latin-Extended text via per-document `/Differences` and `/ToUnicode`,
  Type 0 CID emission for embedded fonts, GPOS-positioned glyph output, PNG and JPEG image XObjects,
  title / author Info metadata, and deterministic object / font / image emission order.
- Editor tooling: a Tree-sitter grammar and corpus ([`tree-sitter-mosaic`][tree-sitter-mosaic]) and
  a Zed extension ([`zed-mosaic`][zed-mosaic]) providing highlighting, outline, document runnables,
  and semantic-token defaults.
- An in-memory cache foundation ([`mos-cache`][mos-cache]) backed by a `HashMap`.

[Unreleased]: https://github.com/kjanat/mosaic/compare/v0.0.0...HEAD
[0.0.0]: https://github.com/kjanat/mosaic/releases/tag/v0.0.0

<!-- other-link-definitions -->

[EBNF]: EBNF.md
[crates]: crates/
[docs:diagnostic-codes]: docs/diagnostic-codes.md
[docs:labels-and-references]: docs/labels-and-references.md
[ex:linebreaks]: examples/linebreaks/
[mos-cache]: crates/mos-cache/
[mos-eval:resolve.rs]: crates/mos-eval/src/resolve.rs
[mos-lsp]: crates/mos-lsp/
[mosaic.ebnf]: mosaic.ebnf
[tree-sitter-mosaic]: crates/tree-sitter-mosaic/
[zed-mosaic]: crates/zed-mosaic/
