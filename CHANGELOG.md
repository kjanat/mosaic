# Changelog

All notable changes to this project will be documented here. The format is based on
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/); this project follows
[Semantic Versioning](https://semver.org/spec/v2.0.0.html) but stays at `0.0.0` while pre-alpha.

## [Unreleased]

### Added

- Author-facing inline line-break controls (issue #26):
  - `\\` hard line break (`InlineKind::HardBreak` / `NodeKind::HardBreak`), flushes the current line
    without paragraph spacing; two in a row produce a blank line; collapses silently at paragraph
    start (block-boundary semantics, regardless of whether prior blocks have painted on the page); a
    lone trailing `\` at end of input emits diagnostic `W025`.
  - `\-` soft-hyphen shorthand expanding to U+00AD; SHY codepoints are stripped from the rendered
    text and their byte offsets recorded in `Word.shy_break_offsets`. The greedy line-breaker now
    consumes those offsets: when a word would overflow the line it picks the latest fitting SHY
    position, emits `prefix-` at end of line, and continues with the suffix as the next word. If no
    SHY prefix fits even an empty line, the existing oversized-word cluster fallback still applies.
    Optimal (non-greedy) selection is left for the Knuth-Plass cutover.
  - Non-breaking space U+00A0 preserved as a cohesive unit by the greedy line-breaker.
- New `WordItem` enum (`Word` / `HardBreak`) replacing the bare `Vec<Word>` stream consumed by
  `flow_words`.
- `examples/linebreaks/` project demonstrating all three controls.

### Changed

- Tree-sitter grammar (`crates/tree-sitter-mosaic`) realigned with the compiler's inline parser for
  the author-facing line-break controls (issue #26): `\\` now parses as a dedicated `hard_break`
  node and `\-` as `soft_hyphen_escape`, both highlighted under `@string.escape`. The external
  `linebreak_escape` token is removed; a bare `\` that does not form one of the recognised escape
  tokens (typically a trailing `\` before a newline, which the compiler also warns as `W025`) parses
  as `loose_backslash` rather than a structural ERROR, matching the compiler's "literal text"
  treatment. `escaped_char` continues to cover `\#`, `\*`, `\[`, `\]`, `\<`, etc. Mirrored into
  `crates/zed-mosaic` via `just sync-zed-queries`, and the Zed extension grammar `rev` is bumped to
  pick up the new node types. `mosaic.ebnf` and `EBNF.md` refreshed to match.
