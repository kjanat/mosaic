# Changelog

All notable changes to this project will be documented here. The format is based on
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/); this project follows
[Semantic Versioning](https://semver.org/spec/v2.0.0.html) with `0.0.x` pre-alpha releases.

## [Unreleased]

### Added

- Sharper `@`-reference diagnostics: an `@key` reference that matches no label but exactly matches a
  bibliography key now reports that the author likely meant a citation and offers `[@key]` as a
  machine-applicable fix ([`mos-eval`][mos-eval] adds a `MOS0033` hint + suggestion). A heading
  `<label>` that is not the last element on its line now raises the new `MOS0048` warning with a
  reorder fix from [`mos-parse`][mos-parse], instead of silently dropping the label declaration and
  failing only downstream at every `@ref` to it. Both fixes are emitted as structured
  [`mos-core`][mos-core] `Suggestion`s, ready for LSP code actions.

- Zed language-server wiring (https://github.com/kjanat/mosaic/issues/118): the
  [`zed-mosaic`][zed-mosaic] extension now declares and spawns [`mos-lsp`][mos-lsp] as the `Mosaic`
  language server, so opening a `.mos` file in Zed gets compiler diagnostics, go-to-definition, and
  label rename. Binary discovery order: the Zed setting `lsp."mos-lsp".binary.path` (with optional
  `binary.arguments`), then `mos-lsp` on `PATH` (install via `cargo mosils`); when neither resolves
  the extension surfaces a clear error pointing at both paths. `extension.toml` gains a
  `[language_servers.mos-lsp]` declaration and `src/lib.rs` implements `language_server_command`
  plus `initialization_options` / `workspace_configuration` passthrough from Zed settings. A new
  `examples/lsp/` project demonstrates heading labels, `@label` / `@page(label)` references, and
  `[@key]` citations for exercising the server. CI now type-checks the workspace-excluded extension
  crate against `wasm32-wasip2` so the wiring cannot silently regress.

- CLI structured suggestion rendering (https://github.com/kjanat/mosaic/issues/109): `mos check` and
  `mos build` now print existing machine-actionable [`mos-core`][mos-core] `Suggestion` payloads as
  `help:` fix-it lines after diagnostics. Unknown-label near misses show a concrete replacement like
  `@intrdo` -> `@intro`, duplicate-label diagnostics show the deterministic rename like `dup` ->
  `dup-2`, and diagnostics without structured suggestions keep their existing output shape.

- LSP label rename: [`mos-lsp`][mos-lsp] now advertises `renameProvider` and answers
  `textDocument/rename`. A cursor on a label: either a declaration's `<label>` token or an `@label`
  / `@page(label)` reference renames it across the document; the response is a `WorkspaceEdit`
  rewriting the **first** declaration's token and every reference to the request's `newName`. Each
  edit covers only the identifier, never the `@` sigil, the `<>` brackets, or the `@page(`…`)`
  delimiters. First-declaration-wins (a duplicate later declaration is left untouched, matching the
  resolver); a cursor off any label returns `null`. Single-document; no workspace/cross-file rename,
  no `prepareRename`, and the new name is not validated.

- Figure numbering controls (https://github.com/kjanat/mosaic/issues/76): `#figure` gains two
  per-figure arguments interpreted by [`mos-eval`][mos-eval]. `numbered: false` opts a figure out of
  the auto `Figure N` counter: it carries no number, its caption keeps no `Figure N:` prefix, and
  (the documented counter rule) it does **not** advance the counter, so surrounding numbered figures
  stay contiguous; a reference to a skipped figure renders its bare label. `supplement: "Plate"`
  swaps the `Figure` supplement word in both the caption (`Plate 1: …`) and references (`Plate 1`)
  while still numbering; `supplement: ""` (or `supplement: none`) drops the word entirely, rendering
  the number alone (`1: …`, `1`) for a "no visible prefix" caption. Numbering stays deterministic
  from document order: `numbered:` is a boolean, not an explicit count. Default `#figure` behavior
  is unchanged, and `#image` remains the way to place an inherently unnumbered graphic. See
  [`docs/labels-and-references.md`](docs/labels-and-references.md).

- LSP go-to-definition (https://github.com/kjanat/mosaic/issues/71): [`mos-lsp`][mos-lsp] now
  advertises `definitionProvider` and answers `textDocument/definition`. A cursor on an `@label` or
  `@page(label)` reference resolves to the source range of the label's declaration; an undeclared
  label or a cursor off any reference returns `null` (no error). When a label is declared more than
  once, the definition points at the **first** declaration; the same one the resolver keeps and
  reports its "first declaration is here" note against. Single-document only: no workspace index, no
  rename, no source/PDF sync.

- Page references (https://github.com/kjanat/mosaic/issues/72): `@page(label)` renders the **printed
  page number** of a labelled target. It parses to a distinct [`mos-parse`][mos-parse]
  `InlineKind::PageReference`, lowers to a [`mos-core`][mos-core] `NodeKind::PageReference`, and is
  resolved by `mos build` through a **bounded resolve↔layout fixpoint**:
  [`mos-layout`][mos-layout]'s `LayoutResult` exposes a `label_pages` map (label → 1-based start
  page), and [`mos-eval`][mos-eval]'s `resolve_page_reference_fixpoint` lays out, rewrites each page
  reference from that map, and re-lays-out until the page numbers stabilize. Non-convergence stops
  at an iteration cap and emits `MOS0047` (warning), keeping the last computed numbers. An
  undeclared `@page` label is `MOS0033` at check time, like a bad `@ref`. A bare `@page` stays an
  ordinary reference; only a well-formed `@page(label)` is a page reference (a pre-alpha change:
  `@page(x)` previously parsed as a `page` reference plus literal `(x)`).

- Citation-key resolution (https://github.com/kjanat/mosaic/issues/65): declared
  `#bibliography("refs.bib")` sources are read during lowering, parsed with [`mos-bib`][mos-bib],
  and checked against `[@key]` citations. Known citations are marked `resolved = true` for a later
  rendering slice while keeping the visible `[?key?]` placeholder. Unknown keys now emit `MOS0045`,
  duplicate keys across bibliography sources emit `MOS0046`, and missing/unreadable/malformed
  bibliography sources suppress false missing-key diagnostics until the record set is complete.
- Numeric citation rendering (https://github.com/kjanat/mosaic/issues/67): resolved `[@key]` markers
  now render a bracketed number (`[1]`, `[2]`, ...) assigned by first-use order over known records.
  Repeated citations to one key reuse its number, and unresolved keys keep the `[?key?]`
  placeholder. Sorted bibliography-list rendering and CSL styles remain out of scope.
- Typed dependency identities (https://github.com/kjanat/mosaic/issues/64): [`mos-cache`][mos-cache]
  now exports `DependencyId`, `DependencyKind`, `ProjectPath`, and `ProjectPathError`: deterministic
  identities for the build inputs that have a real identity today (source/asset/bibliography files
  and labels). `ProjectPath` canonicalizes project-relative file paths (fold `\`→`/`, drop `.`/empty
  segments, resolve `..`, NFC-normalize) so equal logical inputs share one id and rejects empty, raw
  absolute, drive-prefixed, or project-escaping identities. Identities only: no content hashing,
  dependency graph, or `CacheKey` wiring yet, and layout-input/node/style kinds wait for real
  identity schemes.
- Bibliography file dependencies (https://github.com/kjanat/mosaic/issues/69): the first content
  boundary built on those identities. [`mos-bib`][mos-bib] now exports `bibliography_content_hash`,
  the source-hash boundary specialized to `.bib` (engine version + domain tag + raw bytes,
  byte-for-byte, length-framed; interim FNV-1a-128, documented swappable to BLAKE3 without an API
  change), and [`mos-cache`][mos-cache] adds `BibliographyDependency`, pairing a `Bibliography` id
  with that content hash so a future incremental build can tell when citation data changed.
  Construction guarantees the bibliography variant (so `path()`/`kind()` are infallible), the hash
  is caller-supplied (no new crate dependency edge), and it stays identity/boundary only: no
  dependency graph, `CacheKey` wiring, or persistent cache yet.
- Page boundary signatures (https://github.com/kjanat/mosaic/issues/70): [`mos-layout`][mos-layout]
  now exposes `PageBoundarySignature` (per page) and `PageGraphSignature` (the ordered per-page
  list, via `PageGraphSignature::of_graph` / `LayoutResult::page_boundary_signatures`); the §4.5
  `PageOutputHash` reduced to today's layout primitives. Each page folds its number, the quantized
  page box, and ordered runs (quantized position/size, a backend-neutral font identity, text) and
  image placements (intrinsic pixel dimensions + quantized rectangle); shaped glyphs, decoded
  pixels, absolute paths, PDF resource names, and encounter-order image ids are excluded for
  determinism and locality. `first_divergence` reports the first page index where two graphs differ,
  i.e. where pagination changed. Comparison only; no reflow loop or cache wiring yet.
- Shared content hasher: [`mos-core`][mos-core] now exports `ContentHasher`, the engine-stamped,
  length-framed FNV-1a-128 boundary hasher (interim, swappable per the incremental-dependencies §9.4
  slice). [`mos-bib`][mos-bib]'s `bibliography_content_hash` and the new page signatures both build
  on it instead of re-deriving FNV; bibliography hash values are unchanged.

- Literal angle brackets via `\<`: [`mos-parse`][mos-parse] now treats `\<` as an escaped literal
  `<`, the third inline escape alongside `\\` (hard break) and `\-` (soft hyphen). Prose and
  headings can finally contain literal angle brackets -- e.g. a section titled
  `The \<head>
  element` -- which was previously impossible because a trailing `<name>` was
  unconditionally eaten as a label. The label scanners honor the escape too: an escaped `\<name>` at
  a heading's trailing position or a paragraph's leading position is literal text, not a label
  declaration, so writing about markup no longer mints stray labels or trips `MOS0048`. The
  misplaced-heading-label warning gains a hint pointing at the `\<` escape, and the
  [`tree-sitter-mosaic`][tree-sitter-mosaic] grammar already tokenizes `\<` as `escaped_char`
  (locked with a corpus test).

### Changed

- Reference nodes now carry a stamped label-identifier span
  (https://github.com/kjanat/mosaic/issues/116): the [`mos-parse`][mos-parse] `Inline` records a
  `label_span` for `@label` / `@page(label)` (the bare identifier, excluding the `@` sigil and the
  `@page(`…`)` wrapper), and [`mos-eval`][mos-eval] stamps it onto lowered `Reference` /
  `PageReference` nodes as the same `label_span.start` / `label_span.end` attributes declarations
  carry. [`mos-lsp`][mos-lsp] label rename now reads that span directly instead of re-deriving the
  editable range from reference-node span geometry, removing a latent coupling to parser span
  conventions. No intended behavior change for ordinary references; this additionally fixes latent
  range drift for **styled** references (e.g. `*@intro*`), whose node span the parser widens to the
  emphasis delimiters; the old geometry then produced an off-by-the-delimiters rename range, while
  the stamped identifier span is exact.

- LSP diagnostics and go-to-definition now share one lowering per edit
  (https://github.com/kjanat/mosaic/issues/106): publishing diagnostics on `didOpen` / `didChange`
  lowers a document once into the same per-URI cache introduced in #102, and a later
  `textDocument/definition` request on the unchanged source reuses that cached
  [`mos-eval`][mos-eval] lowering instead of lowering the text a second time. Same invalidation
  invariant (source mutation drops the entry) and identical observable behavior: same diagnostics,
  same definition `Location`/`null`: only the duplicate per-edit lowering is gone. Only **pure**
  lowerings are cached: [`mos-eval`][mos-eval]'s `LowerResult` now reports
  `reads_external_resources` (set when `#image` / `#figure` / `#bibliography` read files), and the
  language server never caches such a lowering; those documents are re-lowered per request so they
  always reflect the current filesystem rather than a stale snapshot.

- LSP go-to-definition no longer re-lowers on every request
  (https://github.com/kjanat/mosaic/issues/102): [`mos-lsp`][mos-lsp] now memoises each open
  document's [`mos-eval`][mos-eval] lowering in an in-memory per-URI cache and reuses it across
  `textDocument/definition` requests. The cache is invalidated whenever the document's source
  changes (`didOpen` re-open / `didChange`) or the document closes, so a cached lowering is always
  derived from the current source: behavior is unchanged, only the repeated parse + lower per
  request is gone. The boundary stays thin: the cache only memoises `mos_eval::lower`, it owns no
  parse/lower policy, and it is ready to back future requests (hover, rename) once those land.
- Crate packages now use explicit include allowlists: ordinary crates inherit workspace defaults for
  root licenses, README, examples, source, and tests, while data/build-script crates keep precise
  root-anchored package lists. This keeps agent/project files out of future crates.io tarballs.

### Fixed

- `resolve_relative` ([`mos-core`][mos-core]) no longer silently swallows excess `..`. It looped
  `PathBuf::pop` over each `..`, so once the base was exhausted the extra `..` simply vanished:
  `proj/sub` + `../../../x` collapsed to a bare `x` *inside* the base that the author never wrote.
  Normalization is now correctly lexical -- a `..` that escapes a **relative** base is preserved as
  a leading `..` (`-> ../x`), while a `..` at an **absolute** root is clamped (`/a` +
  `../../x ->
  /x`), matching the OS. Still filesystem-free and symlink-agnostic; identity-grade
  canonicalization stays in `mos_cache::ProjectPath`.

## [0.0.1] - 2026-06-03

### Added

- Citation Style Language (CSL) data foundations (manifest §12): a new [`mos-csl`][mos-csl] crate
  adds (1) a typed CSL **item data model**: `Item`, `ItemType`, and the standard/number/date/name
  variable vocabularies from CSL 1.0.2 Appendices III–IV, plus `Name` and `Date`/`DateParts`; (2) an
  infallible **BibTeX → CSL mapping** (`item_from_bib_entry`, `library_from_bibliography`) from
  `mos-bib` records (entry types map to the closest CSL type, recognised fields to variables,
  unknown fields dropped, authors split on `and`, `Last, First` / `First Last` names become CSL
  personal names, numeric `year` → `issued`, report `number` → CSL `number`, conference `address` →
  `event-place`); and (3) a **CSL XML style parser** (`parse_style` → a typed `Style` AST covering
  `<style>`, `<info>`, `<citation>`, `<bibliography>`, `<macro>`, and the rendering elements) built
  on `roxmltree`, retaining selected style/citation/bibliography/name/sort/date/label rendering
  options, dependent-style info links, `<name-part>` formatting, and raw in-style `<locale>` blocks
  for a future processor. The data model includes the deprecated CSL standard variable `event` for
  spec coverage, and the parser accepts `1.0` and `1.0.x` versions (rejecting other style versions),
  `<text>` elements with multiple source selectors, and invalid `<choose>` branch order. It
  dispatches on element local names but requires the `<style>` root to be in the CSL namespace or
  none, rejecting a foreign namespace. Malformed styles return a recoverable `CslParseError`
  carrying a byte offset and bridging to a `mos-core` `Diagnostic` via the new `MOS0044` code. This
  is the data/parser foundation only: no CSL processor (no style evaluation, formatting, sorting,
  disambiguation, or locales), no locale-file parsing/fallback, and no `mos-eval` / layout / PDF
  wiring.
- Minimal BibTeX record parser (https://github.com/kjanat/mosaic/issues/66): [`mos-bib`][mos-bib]
  gains `parse_bibtex(input: &str) -> Result<Bibliography, BibParseError>`, reading a BibTeX string
  into typed records: `Bibliography { entries }` keyed by citation key and
  `BibEntry { entry_type, key, fields }`, both ordered by `BTreeMap` for deterministic iteration. It
  accepts zero or more `@type{key, field = value, ...}` entries with braced (`{...}`), quoted
  (`"..."`), or bare (`year = 1984`) values, comma-separated fields with an optional trailing comma,
  and naive nested-brace balancing. Entry types and field names are lowercased (BibTeX treats them
  case-insensitively); citation keys are preserved verbatim and duplicate keys are rejected.
  Malformed input returns a recoverable `BibParseError` carrying a byte offset, with `to_diagnostic`
  / `From<BibParseError> for CoreError` bridging into `mos-core` diagnostics, and never panics. This
  is the parser slice only: reading `.bib` files from disk, `@string` / `@preamble` / `@comment` and
  `#` concatenation, TeX decoding, name parsing, citation-key resolution, and any
  citation/bibliography rendering are out of scope, and there is no `mos-eval` / layout / PDF
  integration yet.
- Bibliography source directive boundary (https://github.com/kjanat/mosaic/issues/68): a
  `#bibliography("refs.bib")` directive ([`mos-parse`][mos-parse] adds a
  `DirectiveKind::Bibliography` call-block shape that accepts a positional path or the named
  `path:`/`src:` forms) lowers in [`mos-eval`][mos-eval] to a `NodeKind::Bibliography` node carrying
  the literal `src` plus the `resolved_path` resolved against the source file's directory, so a
  later BibTeX-reading slice can open the database without re-deriving the location. A missing or
  empty path is a hard error (`MOS0040`); a declared-but-absent file is a non-fatal warning
  (`MOS0041`) that still emits the node. Parsing `.bib` contents, resolving citation keys, and
  rendering citations or the bibliography are explicitly out of this slice.
- Mosaic provenance stamp in PDF metadata (https://github.com/kjanat/mosaic/issues/79):
  [`mos-pdf`][mos-pdf] now writes deterministic `/Producer` and `/Creator` Info-dictionary entries
  set to `Mosaic <version>` (sourced from `CARGO_PKG_VERSION` at compile time via the `PRODUCER`
  constant), so a generated PDF traces back to the compiler that bred it. Existing `/Title` and
  `/Author` are preserved and output stays byte-for-byte deterministic; no `/CreationDate`,
  `/ModDate`, XMP, git SHA, hostname, username, path, wall-clock, or build-timestamp data. Both
  fields carry the same constant string and PDF string escaping is handled by the existing `TextStr`
  writer. Deterministic dates via `SOURCE_DATE_EPOCH` and an XMP metadata packet are deferred
  follow-ups.
- Structured diagnostic suggestions (https://github.com/kjanat/mosaic/issues/63): `mos-core` gains a
  backend-neutral `Suggestion` payload: a `SourceSpan` plus replacement text that diagnostics can
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
  every `MOS0030` carries a machine-readable `Suggestion`: a deterministic, collision-aware rename
  to the next free `{label}-N` (smallest `N >= 2` not already declared or suggested) over the
  duplicate label token span: building on the `mos-core` `Suggestion` payload above. The existing
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

[Unreleased]: https://github.com/kjanat/mosaic/compare/v0.0.1...HEAD
[0.0.1]: https://github.com/kjanat/mosaic/compare/v0.0.0...v0.0.1
[0.0.0]: https://github.com/kjanat/mosaic/releases/tag/v0.0.0

<!-- other-link-definitions -->

[EBNF]: EBNF.md
[crates]: crates/
[docs:diagnostic-codes]: docs/diagnostic-codes.md
[docs:labels-and-references]: docs/labels-and-references.md
[ex:linebreaks]: examples/linebreaks/
[mos-bib]: crates/mos-bib/
[mos-cache]: crates/mos-cache/
[mos-core]: crates/mos-core/
[mos-csl]: crates/mos-csl/
[mos-eval]: crates/mos-eval/
[mos-eval:resolve.rs]: crates/mos-eval/src/resolve.rs
[mos-layout]: crates/mos-layout/
[mos-lsp]: crates/mos-lsp/
[mos-parse]: crates/mos-parse/
[mos-pdf]: crates/mos-pdf/
[mosaic.ebnf]: mosaic.ebnf
[tree-sitter-mosaic]: crates/tree-sitter-mosaic/
[zed-mosaic]: crates/zed-mosaic/
