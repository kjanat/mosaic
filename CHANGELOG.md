# Changelog

All notable changes to this project will be documented here. The format is based on
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/); this project follows
[Semantic Versioning](https://semver.org/spec/v2.0.0.html) but stays at `0.0.0` while pre-alpha.

## [Unreleased]

### Added

- Author-facing inline line-break controls (issue #26):
  - `\\` hard line break (`InlineKind::HardBreak` / `NodeKind::HardBreak`), flushes the current line
    without paragraph spacing; two in a row produce a blank line; collapses silently at page start;
    lone trailing `\` emits diagnostic `W025`.
  - `\-` soft-hyphen shorthand expanding to U+00AD; SHY codepoints are stripped from the rendered
    text and their byte offsets recorded in `Word.shy_break_offsets` for the future Knuth-Plass
    hyphenation pass.
  - Non-breaking space U+00A0 preserved as a cohesive unit by the greedy line-breaker.
- New `WordItem` enum (`Word` / `HardBreak`) replacing the bare `Vec<Word>` stream consumed by
  `flow_words`.
- `examples/linebreaks/` project demonstrating all three controls.
