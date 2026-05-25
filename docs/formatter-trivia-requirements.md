# Formatter Trivia Requirements

Design note tracking what the Mosaic compiler parser must preserve before `mos fmt` can be built.
Implementation of `mos fmt` is **out of scope** for this document. The goal is to fix the contract
between the parser and a future formatter so later work has a clear target.

## Scope

This note covers only **currently shipped `.mos` syntax**, as enumerated in `README.md` and
exercised by tests in `crates/mos-parse/src/parser.rs`:

- Headings (`=`, `==`, `===`) with trailing `<label>`
- Paragraphs with inline emphasis `*…*`, strong `**…**`, nested bold-italic `***…***`, inline code
  `` `…` `` (single-backtick delimiters only)
- Leading `<label>` on paragraphs and raw blocks; `@label` cross-references
- Unordered (`-`) and ordered (`N.`) lists with hanging-indent nesting of further marker lines
- `#set name(key: value, …)` directives with double-quoted strings, int, float, length, ident
  values; string escapes `\\`, `\"`, `\n`, `\t`, `\r` inside `"…"`
- `#image("path", …)`, `#figure(…)`
- Long-bracket raw blocks: `#pre[[…]]`, `#code[[…]]` (long-bracket `=` runs supported)
- Only two inline escapes: `\\` → hard break, `\-` → soft hyphen (U+00AD). NBSP `U+00A0` is
  preserved as a literal codepoint, not via an escape.

Anything not in that list — `#import`, `#include`, `#verse`, comments, math, footnotes, tables,
templates, scripting, generic backslash escapes like `\*` / `\#`, single-quoted string literals — is
**aspirational manifest syntax** and is explicitly excluded from formatter requirements until it
lands in the compiler parser. See *Aspirational syntax* below.

## Status of `mos fmt` today

`mos fmt` is a CLI stub in `crates/mos/src/main.rs:69`/`:106` that returns
`unimplemented_subcommand("fmt")`. The Rust parser in `crates/mos-parse` produces a lossy AST keyed
by `SourceSpan` byte ranges; it does **not** retain a trivia layer. Tree-sitter (`grammar.js`)
already models several trivia tokens (`comment`, `blank_line`) that the Rust parser ignores.

`manifest-tracker.md` already lists the relevant gaps:

- “Preserve comments in the CST if formatter or tooling needs them.” (line 142)
- “Preserve useful formatting trivia for formatter support.” (line 143)
- “Define formatting rules for current syntax.” / “Preserve comments and meaningful trivia.” (lines
  382, 384)

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

| Trivia                                | Decision | Notes                                                  |
| ------------------------------------- | -------- | ------------------------------------------------------ |
| Level (`=`, `==`, `===`)              | R        | Source level is structural; never auto-promote/demote. |
| Spacing between `=`s and heading text | N        | Always exactly one space.                              |
| Trailing `<label>` presence and name  | R        | Identity-preserving; references depend on it.          |
| Spacing before `<label>`              | N        | Always exactly one space before `<`.                   |
| Inline content (emphasis, code, refs) | R        | Formatting follows inline rules below.                 |

### Paragraphs and inline content

| Trivia                                             | Decision | Notes                                                                                                                                                                                                                                                                  |
| -------------------------------------------------- | -------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Marker choice for emphasis (`*x*`)                 | R        | Only one shipped marker; preserved trivially.                                                                                                                                                                                                                          |
| Strong marker `**x**` and nested `***x***`         | R        | Number of leading/trailing `*` is meaningful (see `parser.rs` nested tests). Formatter must round-trip the source delimiter count.                                                                                                                                     |
| Inline code backticks                              | R        | Only a single-byte `` ` `` delimiter is shipped (`inline.rs:226`); body bytes between the pair are stored verbatim. There is no multi-backtick run-length syntax to preserve.                                                                                          |
| Hard break `\\` at end of line                     | R        | Already a semantic node; emit identically.                                                                                                                                                                                                                             |
| Soft hyphen `\-` source spelling                   | ?        | The parser immediately rewrites `\-` to a literal U+00AD inside the inline text (`inline.rs:127`); source spelling is recoverable only by inspecting the span. **Gap**: to round-trip `\-` vs an authored U+00AD, the AST must record which expansions came from `\-`. |
| NBSP `U+00A0` and literal U+00AD                   | R        | Parser preserves byte-identical; formatter must not transcode to ASCII space.                                                                                                                                                                                          |
| Lone `\` followed by non-escape byte               | R        | Today the `\` is kept as a literal byte and the next character is parsed normally (`inline.rs:143`, test `backslash_before_non_escape_byte_is_silent_literal`). Formatter must emit the `\` verbatim so `C:\Temp` and `\*foo*` survive.                                |
| Soft-wrap line breaks inside a paragraph           | ?        | Today the parser joins source lines into one inline run. **Gap**: source line breaks are not retained, so a formatter cannot preserve author wrap.                                                                                                                     |
| Intra-line consecutive spaces                      | N        | Collapse to single space. NBSP is exempt (R, above).                                                                                                                                                                                                                   |
| Leading inline whitespace inside `*…*` / `` `…` `` | R        | Whitespace immediately inside delimiters is part of the run today; preserve so `*  x*` round-trips, or normalize after consulting parser behaviour.                                                                                                                    |

### Labels and references

| Trivia                                 | Decision | Notes                                                                                                            |
| -------------------------------------- | -------- | ---------------------------------------------------------------------------------------------------------------- |
| Leading vs trailing `<label>` position | R        | Shipped semantics differ between heading (trailing) and paragraph/raw block (leading). Never reposition.         |
| Label name characters                  | R        | Identity-preserving.                                                                                             |
| `@label` reference text                | R        | Generic resolver-supplied display text is computed downstream; source token is just `@name` and must round-trip. |
| Whitespace inside angle brackets       | N        | Reject/normalize; current parser does not accept whitespace there.                                               |

### Lists

| Trivia                                | Decision | Notes                                                                                                                                                                                                                                                            |
| ------------------------------------- | -------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Marker kind (`-` vs `N.`)             | R        | Ordered vs unordered is structural.                                                                                                                                                                                                                              |
| Source number digits in ordered lists | ?        | Parser normalizes numbering and discards source digits. **Gap**: cannot round-trip `1.`/`2.`/`10.` author choices vs forced `1.` style.                                                                                                                          |
| Indentation depth                     | R        | Hanging-indent nesting depth determines structure.                                                                                                                                                                                                               |
| Indentation width (spaces per level)  | N        | Formatter picks one (recommended: two spaces); parser only cares about strictly-increasing depth.                                                                                                                                                                |
| Blank line between items              | R        | The parser breaks list collection on any blank line (`list.rs:26`), but blank-line-separated runs of marker lines still belong to the same logical list visually; preserve until we decide otherwise.                                                            |
| Continuation lines under an item      | —        | **Not shipped.** `collect_list_lines` (`list.rs:22`) stops at the first non-marker line; an indented line without a marker breaks the list rather than continuing the previous item. Formatter contract for continuation is deferred until parser support lands. |

### `#set` and other directives

| Trivia                                               | Decision | Notes                                                                                                                                                                                                                 |
| ---------------------------------------------------- | -------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Directive name (`#set`, `#image`, `#figure`)         | R        | Identity.                                                                                                                                                                                                             |
| Whitespace between `#name` and `(`                   | N        | Spaces and tabs between `#name` (and `#set name`) and `(` are accepted today (`directive.rs:25`, `:122`, `:141`, `:163`); none of that spacing is retained in the AST. Formatter chooses a canonical form (no space). |
| Whitespace inside argument list (`(`, `,`, `:`, `)`) | N        | Today’s parser skips it (`skip_set_ws`). Formatter chooses a canonical form: `name(key: value, key: value)`.                                                                                                          |
| Argument order                                       | R        | Positional and keyed arguments must keep source order; no reordering even for keyed arguments.                                                                                                                        |
| String literal form (`"a"` only)                     | R        | Only `"…"` strings are shipped (`directive.rs:414`); `'…'` is not parsed. Formatter must emit `"…"` and round-trip the supported in-string escapes (`\\`, `\"`, `\n`, `\t`, `\r`).                                    |
| Numeric literal form (`12pt` vs `12.0pt`)            | R        | Parser stores the decoded value but the source spans cover the original digits; formatter should round-trip the literal as written rather than renormalize.                                                           |
| Trailing comma in argument list                      | R        | Accepted today (`set_value_trailing_comma_ok`). Formatter policy: preserve when present, do not insert one.                                                                                                           |
| Newlines inside argument list                        | ?        | Accepted today (`set_block_multiline`), but all interior whitespace is dropped by `skip_set_ws`. **Gap**: AST cannot tell a multiline call from a single-line one, so a formatter cannot preserve author wrap.        |

### `#image` and `#figure`

Same rules as `#set` for arguments. `#figure` body content follows the relevant inline/list rules.

### Long-bracket raw blocks (`#pre[[…]]`, `#code[[…]]`)

| Trivia                                      | Decision | Notes                                                                                                                                                                                                                                                         |
| ------------------------------------------- | -------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Inner text bytes                            | R/N      | Most bytes are preserved verbatim with no escape decoding, but `normalize_raw_text` (`directive.rs:72`) trims a single leading newline immediately after `[[` and rewrites `\r\n` → `\n`. The on-the-wire AST text therefore is not byte-identical to source. |
| Leading newline inside the brackets         | —        | **Not preservable today.** The parser always strips exactly one leading `\n` after the opening long bracket. Formatter contract: emit the canonical form `#kw[[\n…\n]]`.                                                                                      |
| `\r\n` vs `\n` line endings inside the body | N        | Normalized to `\n` by the parser; formatter emits `\n`.                                                                                                                                                                                                       |
| Long-bracket `=` run length                 | R        | `[==[ … ]==]` etc. is shipped (`scan_long_raw_open`). Formatter must round-trip the exact `=` count chosen by the author.                                                                                                                                     |
| Optional argument list before the body      | R        | Same rules as `#set`.                                                                                                                                                                                                                                         |
| Leading `<label>` attachment                | R        | Per current parser semantics.                                                                                                                                                                                                                                 |

### Comments

Comments are **not currently part of the shipped Rust parser**. Tree-sitter defines `//` and
`/* … */` (`grammar.js:95–99`), but the Rust CST has no comment node. For `mos fmt` to be useful,
the compiler parser must learn to attach comments as trivia to adjacent nodes. See *Parser gaps*.

## Aspirational manifest syntax (NOT shipped)

The formatter contract intentionally excludes these because the compiler parser does not produce
them today. Adding any of them is a prerequisite, not a formatter task.

- `#import "path" : names`, `#include "path"` — defined only in `tree-sitter-mosaic/grammar.js`, not
  in `crates/mos-parse`.
- `#verse`, block-form function calls — tree-sitter only.
- Inline and display math — manifest only.
- `//` line comments and `/* … */` block comments — tree-sitter has tokens; compiler parser silently
  fails to recognize them.
- Generic backslash escapes (`\*`, `\#`, `\[`, `\]`, `\<`, …): `tree-sitter-mosaic` has an
  `escaped_char` rule, but the compiler parser only recognizes `\\` and `\-` and treats any other
  `\X` as a literal `\` byte followed by `X` (`inline.rs:143`, test
  `backslash_before_non_escape_byte_is_silent_literal`).
- Multi-backtick code spans (`…`, `` ```…``` ``): only single-backtick `` `…` `` is shipped.
- Single-quoted string literals (`'…'`) inside directive arguments: only `"…"` is shipped
  (`directive.rs:414`).
- Lazy paragraph continuation lines inside list items: `list.rs:22` breaks list collection on any
  non-marker line.
- Footnotes, tables, theorems, glossary, bibliography, templates, scripting, package imports —
  manifest only.
- Per-output styling controls beyond `#set` keyed values — manifest only.

If `mos fmt` is built before any of these land, it must reject or pass-through unknown constructs
rather than guess.

## Parser and tooling gaps (follow-up issue candidates)

Each bullet is a concrete follow-up. None of them are resolved by this PR.

1. **No comment syntax in the compiler parser.** Add `//` line comments (and decide on `/* … */`) to
   `crates/mos-parse` with span and attachment rules (leading vs trailing, own-line vs same-line).
   Mirror tree-sitter token shape from `grammar.js:95–99`.
2. **No trivia channel in the AST.** Today every node carries only a `SourceSpan`. A formatter needs
   blank-line counts and comment attachments. Options: (a) sidecar `Vec<Trivia>` keyed by span, (b)
   `Node { leading: Vec<Trivia>, trailing: Vec<Trivia>, … }`, (c) full lossless CST. Decision needed
   before any formatter work.
3. **Source line breaks inside paragraphs are lost.** Parser joins lines into one inline run.
   Formatter cannot preserve author wrap. Decide whether to keep author wrap or canonicalize.
4. **Ordered-list source numbers are discarded.** Parser renormalizes ordered list markers. Either
   preserve source digits as trivia or formally adopt forced `1.` rendering and document it.
5. **Multiline directive calls are not preserved.** `set_block_multiline` confirms newlines are
   accepted inside `(…)`, but `skip_set_ws` drops them. To round-trip multiline `#set` / `#image` /
   `#figure` calls, the AST must record where the author placed line breaks (e.g., per-argument
   leading/trailing trivia, or an "is multiline" flag with normalized formatting rules).
6. **Soft-hyphen source spelling is collapsed.** `\-` becomes a literal U+00AD in the inline text
   immediately (`inline.rs:127`). To distinguish authored `\-` from a literal U+00AD glyph, the AST
   needs a per-character trivia marker or a dedicated `SoftHyphen` inline node.
7. **Raw-block leading newline is trimmed.** `normalize_raw_text` always strips one `\n` after the
   opening long bracket. If a formatter must round-trip a raw block whose body really starts with
   multiple blank lines, that information is gone. Decide whether to keep current trimming and
   document it as canonical, or extend the AST to remember the original leading newline count.
8. **No list continuation support.** Adding lazy / indented continuation lines to list items is
   itself a parser feature, not just a formatter follow-up. Track separately.
9. **No comment fixtures.** Once the parser supports comments, add round-trip tests for the
   formatter contract.
10. **Tree-sitter parity drift.** `grammar.js` defines constructs the compiler parser does not
    (`#import`, `#include`, `#verse`, math, comments, generic `escaped_char`). Either narrow the
    grammar to shipped syntax, or document the drift and treat tree-sitter as forward-looking. The
    Zed extension inherits whichever choice is made.

## Tree-sitter / Zed parity implications

- Comments: if the compiler parser learns `//` first, tree-sitter is already ahead and the Zed
  highlight queries will just light up. If it learns `/* */`, ensure `grammar.js:98` regex still
  matches.
- Blank lines: tree-sitter exposes `$.blank_line` as an external token (`grammar.js:37`, `:66`). The
  compiler parser only *skips* blanks today. Whatever trivia model the compiler adopts should at
  minimum count blank lines, so tooling can agree on “paragraph break” vs “section break”.
- `#import`/`#include`/`#verse`: keep these as tree-sitter-only and Zed-only until the compiler
  parser supports them, or remove them from the grammar to avoid editor highlighting that the
  compiler will reject. Decision belongs in a follow-up.
- `escaped_char`: the tree-sitter grammar highlights `\#`, `\*`, `\[`, `\]`, `\<` etc. as escapes.
  The compiler parser does not honour them. The formatter must follow the compiler: any `\X` other
  than `\\` or `\-` round-trips as a literal `\` followed by `X`. The grammar rule is fine as a
  highlight hint, but the formatter must not "decode" the sequence.
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
