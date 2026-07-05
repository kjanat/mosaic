# MOSAIC-PARSE KNOWLEDGE BASE

## OVERVIEW

`mos-parse` turns `.mos` bytes into syntax data plus recoverable diagnostics. It is syntax only;
semantic meaning belongs in `mos-eval`.

## CURRENT GRAMMAR

Implemented:

- Headings: `=`, `==`, `===`.
- Paragraphs.
- Inline: `*emphasis*`, `**strong**`, backtick code, `@label` and `@page(label)` references,
  `[@key]` citations.
- Inline line-break controls: `\\` hard break (emits `InlineKind::HardBreak`), `\-` soft-hyphen
  shorthand (expands to literal U+00AD in the text payload), literal U+00A0 NBSP (preserved verbatim
  through to layout).
- Labels on headings and paragraph starts.
- Lists: `-` and `N.`, nesting by spaces, explicit continuation lines indented to the item text
  column, and parent continuation after nested child lists.
- Directives: `#set name(...)`, `#image(...)`, `#figure(...)`, `#bibliography(...)`.
- Raw blocks: `#pre[[...]]` and `#code[[...]]`, with `[=[...]=]` delimiters for nested `]]`.
- Values: string, int, float, length `mm`/`pt`/`em`, ident.

Not implemented despite manifest examples:

- General function calls, `#let`, `if`, custom scripting, math `$...$`, equations, tables, citation
  clusters, semantic citation resolution, includes, comments-preserving formatter CST.

## WHERE TO LOOK

| Task             | Location                       | Notes                                                |
| ---------------- | ------------------------------ | ---------------------------------------------------- |
| Public CST       | `SyntaxTree`, `Item`, `Inline` | Consumers should use typed variants.                 |
| Directive kind   | `DirectiveKind`                | Load-bearing; do not infer from name strings.        |
| List item blocks | `ListItemBlock`                | Ordered item paragraphs and nested lists.            |
| Parser driver    | `Parser::run`                  | Top-level dispatch.                                  |
| Directives       | directive parser section       | Balanced parens, recovery, `#set` vs `#image`.       |
| Set values       | `parse_set_value`              | UTF-8 and length unit care.                          |
| Paragraphs       | `parse_paragraph`              | Raw spans, normalized payload text.                  |
| Lists            | list parser section            | Marker, text-column continuation, and nesting rules. |
| Inline runs      | inline parser section          | Non-nesting by design today.                         |

## CONVENTIONS

- Preserve source spans over raw input. Byte offsets must stay valid UTF-8 boundaries.
- Prefer recoverable diagnostics over hard failure.
- CRLF input spans index raw source; text payload may normalize line endings.
- Spaces count for list indentation; tabs are tolerated only as post-marker whitespace and never
  make an indented continuation.
- List continuation is explicit only: blank lines terminate, under-indented lines are not lazy
  continuation, and marker lines start sibling/child lists before continuation is considered.
- Parser should not load files, decode images, resolve refs, or assign document semantics.
- Inline escape expansion (`\-` → U+00AD) buffers into a `pending` string and is flushed at
  structural boundaries (delimiter close, code span, label reference, EOI). Adding new escapes must
  route through the same buffer so spans and styling stay consistent.

## ANTI-PATTERNS

- Do not parse future manifest syntax unless current task explicitly asks for it.
- Do not lower `#figure` or `#image` here. Return syntax; evaluator owns semantics.
- Do not use `panic` for malformed user input.
