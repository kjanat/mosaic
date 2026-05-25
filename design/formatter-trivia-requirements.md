# Formatter Trivia Requirements

Design note tracking what the Mosaic compiler parser must preserve before `mos fmt` can be built.
Implementation of `mos fmt` is **out of scope** for this document. The goal is to fix the contract
between the parser and a future formatter so later work has a clear target.

## Scope

This note covers only **currently shipped `.mos` syntax**, as enumerated in `README.md` and
exercised by tests in `crates/mos-parse/src/parser.rs`:

- Headings (`=`, `==`, `===`) with trailing `<label>`
- Paragraphs with inline emphasis `*…*`, strong `**…**`, nested bold-italic `***…***`,
  inline code `` `…` ``
- Leading `<label>` on paragraphs and raw blocks; `@label` cross-references
- Unordered (`- `) and ordered (`N. `) lists with hanging-indent nesting
- `#set name(key: value, …)` directives (string, int, float, length, ident values)
- `#image("path", …)`, `#figure(…)`
- Long-bracket raw blocks: `#pre[[…]]`, `#code[[…]]`
- Hard break `\\`, soft hyphen `\-`, NBSP `U+00A0`, backslash escapes for parser sigils

Anything not in that list — `#import`, `#include`, `#verse`, comments, math, footnotes, tables,
templates, scripting — is **aspirational manifest syntax** and is explicitly excluded from formatter
requirements until it lands in the compiler parser. See *Aspirational syntax* below.

## Status of `mos fmt` today

`mos fmt` is a CLI stub in `crates/mos/src/main.rs:69`/`:106` that returns
`unimplemented_subcommand("fmt")`. The Rust parser in `crates/mos-parse` produces a lossy AST keyed
by `SourceSpan` byte ranges; it does **not** retain a trivia layer. Tree-sitter (`grammar.js`)
already models several trivia tokens (`comment`, `blank_line`) that the Rust parser ignores.

`manifest-tracker.md` already lists the relevant gaps:

- “Preserve comments in the CST if formatter or tooling needs them.” (line 142)
- “Preserve useful formatting trivia for formatter support.” (line 143)
- “Define formatting rules for current syntax.” / “Preserve comments and meaningful trivia.”
  (lines 382, 384)

This note refines those bullets into per-construct requirements.

## Trivia requirements by construct

For each shipped construct, R = required to preserve, N = normalize, ? = open question.

### Document-level

| Trivia                            | Decision | Notes                                                                                                                       |
| --------------------------------- | -------- | --------------------------------------------------------------------------------------------------------------------------- |
| Blank lines between blocks        | R        | Count is significant for authorial intent. Formatter should preserve “0 or ≥1” and collapse runs to a configurable maximum. |
| Trailing newline at EOF           | N        | Always emit single `\n`.                                                                                                    |
| Leading whitespace before a block | N        | Collapse to none unless inside a list item continuation.                                                                    |
| CRLF vs LF line endings           | N        | Parser already normalizes CRLF→LF in inline payloads; formatter emits LF.                                                   |
| Final-line whitespace             | N        | Strip trailing spaces on every emitted line.                                                                                |

### Headings

| Trivia                                  | Decision | Notes                                                                                  |
| --------------------------------------- | -------- | -------------------------------------------------------------------------------------- |
| Level (`=`, `==`, `===`)                | R        | Source level is structural; never auto-promote/demote.                                 |
| Spacing between `=`s and heading text   | N        | Always exactly one space.                                                              |
| Trailing `<label>` presence and name    | R        | Identity-preserving; references depend on it.                                          |
| Spacing before `<label>`                | N        | Always exactly one space before `<`.                                                   |
| Inline content (emphasis, code, refs)   | R        | Formatting follows inline rules below.                                                 |

### Paragraphs and inline content

| Trivia                                                        | Decision | Notes                                                                                                                                                 |
| ------------------------------------------------------------- | -------- | ----------------------------------------------------------------------------------------------------------------------------------------------------- |
| Marker choice for emphasis (`*x*`)                            | R        | Only one shipped marker; preserved trivially.                                                                                                         |
| Strong marker `**x**` and nested `***x***`                     | R        | Number of leading/trailing `*` is meaningful (see `parser.rs` nested tests). Formatter must round-trip the source delimiter count.                    |
| Inline code backticks                                         | R        | Backtick run length must match source; raw text inside is preserved verbatim.                                                                         |
| Backslash escapes (`\*`, `\#`, `\\`, `\-`, …)                  | R        | Escapes encode authorial intent (literal vs sigil). Formatter must round-trip the exact escape, not the decoded glyph.                                |
| Hard break `\\` at end of line                                | R        | Already a semantic node; emit identically.                                                                                                            |
| Soft hyphen `\-`                                              | R        | Semantic node; formatter must not collapse with surrounding text.                                                                                     |
| NBSP `U+00A0` and literal U+00AD                              | R        | Parser preserves byte-identical; formatter must not transcode to ASCII space.                                                                         |
| Soft-wrap line breaks inside a paragraph                      | ?        | Today the parser joins source lines into one inline run. **Gap**: source line breaks are not retained, so a formatter cannot preserve author wrap.    |
| Intra-line consecutive spaces                                 | N        | Collapse to single space. NBSP is exempt (R, above).                                                                                                  |
| Leading inline whitespace inside `*…*` / `` `…` ``            | R        | Whitespace immediately inside delimiters is part of the run today; preserve so `*  x*` round-trips, or normalize after consulting parser behaviour.   |

### Labels and references

| Trivia                                          | Decision | Notes                                                                                                                |
| ----------------------------------------------- | -------- | -------------------------------------------------------------------------------------------------------------------- |
| Leading vs trailing `<label>` position           | R        | Shipped semantics differ between heading (trailing) and paragraph/raw block (leading). Never reposition.             |
| Label name characters                            | R        | Identity-preserving.                                                                                                  |
| `@label` reference text                          | R        | Generic resolver-supplied display text is computed downstream; source token is just `@name` and must round-trip.     |
| Whitespace inside angle brackets                 | N        | Reject/normalize; current parser does not accept whitespace there.                                                   |

### Lists

| Trivia                                            | Decision | Notes                                                                                                                                  |
| ------------------------------------------------- | -------- | -------------------------------------------------------------------------------------------------------------------------------------- |
| Marker kind (`-` vs `N.`)                         | R        | Ordered vs unordered is structural.                                                                                                    |
| Source number digits in ordered lists             | ?        | Parser normalizes numbering and discards source digits. **Gap**: cannot round-trip `1.`/`2.`/`10.` author choices vs forced `1.` style. |
| Indentation depth                                 | R        | Hanging-indent nesting depth determines structure.                                                                                     |
| Indentation width (spaces per level)              | N        | Formatter picks one (recommended: two spaces); parser only cares about strictly-increasing depth.                                      |
| Blank line between items                          | R        | Loose vs tight list is an authorial decision in many Markdown-family formatters; preserve until we decide otherwise.                   |
| Continuation lines under an item                  | R        | Multi-line item bodies exist; preserve their attachment.                                                                               |

### `#set` and other directives

| Trivia                                                  | Decision | Notes                                                                                                                                |
| ------------------------------------------------------- | -------- | ------------------------------------------------------------------------------------------------------------------------------------ |
| Directive name (`#set`, `#image`, `#figure`)            | R        | Identity.                                                                                                                            |
| Whitespace between `#name` and `(`                      | N        | Always no space (matches current shipped grammar).                                                                                   |
| Whitespace inside argument list (`(`, `,`, `:`, `)`)     | N        | Today’s parser skips it (`support.rs`). Formatter chooses a canonical form: `name(key: value, key: value)`.                          |
| Argument order                                          | R        | Positional and keyed arguments must keep source order; no reordering even for keyed arguments.                                       |
| Value literal form (`12pt` vs `12.0pt`, `"a"` vs `'a'`) | R        | Parser preserves the textual form via spans today. Round-trip the literal as written; do not renormalize numeric formatting.         |
| Trailing comma in argument list                         | ?        | Current parser tolerance is not explicitly tested; formatter policy depends on what the parser accepts. **Gap**: needs a fixture.    |
| Newlines inside argument list                           | ?        | `manifest-tracker.md:383` calls out multiline calls. Parser does not preserve them today. **Gap.**                                   |

### `#image` and `#figure`

Same rules as `#set` for arguments. `#figure` body content follows the relevant inline/list rules.

### Long-bracket raw blocks (`#pre[[…]]`, `#code[[…]]`)

| Trivia                                            | Decision | Notes                                                                                                  |
| ------------------------------------------------- | -------- | ------------------------------------------------------------------------------------------------------ |
| Inner text bytes                                  | R        | Verbatim; no whitespace collapse, no escape decoding inside the brackets.                              |
| Leading/trailing newline inside the brackets      | R        | Preserve as-authored.                                                                                  |
| Optional argument list before the body            | R        | Same rules as `#set`.                                                                                  |
| Leading `<label>` attachment                      | R        | Per current parser semantics.                                                                          |

### Comments

Comments are **not currently part of the shipped Rust parser**. Tree-sitter defines `//` and
`/* … */` (`grammar.js:95–99`), but the Rust CST has no comment node. For `mos fmt` to be useful,
the compiler parser must learn to attach comments as trivia to adjacent nodes. See *Parser gaps*.

## Aspirational manifest syntax (NOT shipped)

The formatter contract intentionally excludes these because the compiler parser does not produce
them today. Adding any of them is a prerequisite, not a formatter task.

- `#import "path" : names`, `#include "path"` — defined only in `tree-sitter-mosaic/grammar.js`,
  not in `crates/mos-parse`.
- `#verse`, block-form function calls — tree-sitter only.
- Inline and display math — manifest only.
- `//` line comments and `/* … */` block comments — tree-sitter has tokens; compiler parser
  silently fails to recognize them.
- Footnotes, tables, theorems, glossary, bibliography, templates, scripting, package imports —
  manifest only.
- Per-output styling controls beyond `#set` keyed values — manifest only.

If `mos fmt` is built before any of these land, it must reject or pass-through unknown
constructs rather than guess.

## Parser and tooling gaps (follow-up issue candidates)

Each bullet is a concrete follow-up. None of them are resolved by this PR.

1. **No comment syntax in the compiler parser.** Add `//` line comments (and decide on
   `/* … */`) to `crates/mos-parse` with span and attachment rules (leading vs trailing,
   own-line vs same-line). Mirror tree-sitter token shape from `grammar.js:95–99`.
2. **No trivia channel in the AST.** Today every node carries only a `SourceSpan`. A formatter
   needs blank-line counts and comment attachments. Options: (a) sidecar `Vec<Trivia>` keyed by
   span, (b) `Node { leading: Vec<Trivia>, trailing: Vec<Trivia>, … }`, (c) full lossless CST.
   Decision needed before any formatter work.
3. **Source line breaks inside paragraphs are lost.** Parser joins lines into one inline run.
   Formatter cannot preserve author wrap. Decide whether to keep author wrap or canonicalize.
4. **Ordered-list source numbers are discarded.** Parser renormalizes ordered list markers.
   Either preserve source digits as trivia or formally adopt forced `1.` rendering and document
   it.
5. **No fixture for argument-list whitespace, trailing commas, or multiline calls.** Required
   before deciding canonical formatter output for `#set` / `#image` / `#figure`.
6. **No comment fixtures.** Once the parser supports comments, add round-trip tests for the
   formatter contract.
7. **Tree-sitter parity drift.** `grammar.js` defines constructs the compiler parser does not
   (`#import`, `#include`, `#verse`, math, comments). Either narrow the grammar to shipped
   syntax, or document the drift and treat tree-sitter as forward-looking. The Zed extension
   inherits whichever choice is made.

## Tree-sitter / Zed parity implications

- Comments: if the compiler parser learns `//` first, tree-sitter is already ahead and the Zed
  highlight queries will just light up. If it learns `/* */`, ensure `grammar.js:98` regex still
  matches.
- Blank lines: tree-sitter exposes `$.blank_line` as an external token (`grammar.js:37`,
  `:66`). The compiler parser only *skips* blanks today. Whatever trivia model the compiler
  adopts should at minimum count blank lines, so tooling can agree on “paragraph break” vs
  “section break”.
- `#import`/`#include`/`#verse`: keep these as tree-sitter-only and Zed-only until the
  compiler parser supports them, or remove them from the grammar to avoid editor highlighting
  that the compiler will reject. Decision belongs in a follow-up.
- Zed sync (`just sync-zed-queries`) will overwrite copied queries from tree-sitter; any
  formatter-related query additions must land in `crates/tree-sitter-mosaic/queries/` first.

## Out of scope

- Implementing `mos fmt`.
- Choosing a final indentation width, line length, or argument-wrapping policy.
- Adding comment syntax to the compiler parser.
- Changing tree-sitter grammar.
- Updating `manifest-tracker.md` — the relevant gaps are already listed there.

## Acceptance check

- Trivia requirements are listed by construct, restricted to shipped syntax.
- Shipped vs aspirational syntax is separated explicitly.
- Concrete parser/tooling gaps are enumerated for follow-up issues.
- No aspirational construct is presented as shipped.
