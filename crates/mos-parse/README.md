# mos-parse

Syntax parser for Mosaic `.mos` source files.

`mos-parse` turns one UTF-8 source string into a concrete syntax tree plus recoverable diagnostics.
It does not lower to the semantic document model, resolve references, load files, inspect images, or
make layout/backend decisions. Those belong to later crates, mainly `mos-eval`, `mos-layout`, and
`mos-pdf`.

## Purpose

- Preserve source spans as byte offsets into the original input.
- Return typed syntax nodes for the language subset currently shipped.
- Keep parsing recoverable: malformed user input produces diagnostics, not panics.
- Keep syntax separate from semantics. Directive names and argument values are parsed here;
  validation and meaning happen later.

## Supported Syntax

Top-level items:

- Headings: `= Heading`, `== Heading`, `=== Heading`, and deeper `=` runs accepted by parser when
  followed by whitespace.
- Paragraphs: consecutive non-blank, non-block-start lines.
- Lists: bullet `- item` and ordered `1. item`; nesting by leading spaces; ordered lists renumber
  later from 1.
- Directives: `#set name(...)`, `#image(...)`, `#figure(...)`.
- Raw blocks: `#pre[[...]]` and `#code[[...]]`, including `[=[...]=]`-style long brackets for bodies
  containing `]]`.

Inline runs:

- Plain text.
- `*emphasis*`.
- `**strong**`.
- `` `code` ``.
- `@label` references.

Labels:

- Trailing heading labels: `= Title <sec:intro>`.
- Leading paragraph/raw-block labels: `<p:intro> Text...`.
- Label characters: ASCII letters, digits, `_`, `-`, `:`, `.`.

Directive arguments:

- Named args: `key: value`.
- Leading positional string for standalone calls: `#image("path.png")`.
- Values: strings, integers, floats, lengths with `mm`, `pt`, or `em`, and identifiers.
- String escapes: `\\`, `\"`, `\n`, `\t`, `\r`.

## Module Layout

- `lib.rs`: public exports and `parse(src, file)` entry point.
- `syntax.rs`: public CST types: `SyntaxTree`, `Item`, `Inline`, `SetArg`, `SetValue`,
  `ParseResult`.
- `parser.rs`: parser state, top-level dispatch, span and diagnostic helpers, tests.
- `block.rs`: headings and paragraphs.
- `list.rs`: list collection and nesting.
- `inline.rs`: non-nesting inline tokenizer.
- `directive.rs`: `#set`, `#image`, `#figure`, `#pre`, `#code`, directive values, balanced
  delimiters.
- `support.rs`: internal scanning, label stripping, raw text normalization, list marker helpers.

## Spans And Diagnostics

Every syntax node that represents source carries a `mos_core::SourceSpan`. Spans index the raw input
bytes and preserve the caller-provided file path.

`parse` always returns `ParseResult`. Check `ParseResult::has_errors()` before lowering. Warnings
are used for recoverable inline issues such as unterminated emphasis or stray `@`; errors are used
for malformed directives or raw blocks.

CRLF source spans still point into the original source. Some payload text normalizes line endings,
for example paragraph inline text and raw block bodies.

## Example

```rust
use std::path::Path;

use mos_parse::{InlineKind, Item, parse};

let result = parse("= Intro <sec:intro>\nSee @sec:intro.\n", Path::new("main.mos"));

assert!(!result.has_errors());
assert!(matches!(result.tree.items[0], Item::Heading { .. }));

let Item::Paragraph { inlines, .. } = &result.tree.items[1] else {
    unreachable!();
};
assert!(inlines.iter().any(|inline| inline.kind == InlineKind::Reference));
```

## Known Non-Goals

Not implemented here:

- Semantic lowering, section numbering, duplicate/unknown-label checks, image validation, or figure
  behavior.
- General function calls, `#let`, `if`, custom scripting, templates, imports, or includes.
- Math, equations, tables, citations, footnotes, indexes, glossaries, or bibliography syntax.
- Comments-preserving formatter CST.
- Nested inline markup. Current inline parsing treats emphasis/strong/code contents as plain text.
- File IO. The parser only receives source text and a path for spans.
