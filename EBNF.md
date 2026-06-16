# Mosaic EBNF Reference

## Executive summary

The live [kjanat/mosaic](https://github.com/kjanat/mosaic) repo is already a real pre-alpha
compiler/parser, but its surface syntax is intentionally narrower than the broader language sketched
in the repo’s manifest. Today the parser documents support for headings, paragraphs, inline
emphasis/strong/code, `#set`, `#image`, `#figure`, labels and `@label` references, while the
manifest sketches richer constructs such as `#import`, `#include`, block bodies, broader expression
forms, and a larger long-term language design.

The EBNF below is therefore intentionally reference-first: conservative enough to drive an actual
Tree-sitter grammar, but broad enough to cover the requested editor-facing language. It adopts
CommonMark’s paragraph/blank-line/hard-break model, Typst’s proven `= heading` / `<label>` / `@ref`
/ `#call(...)` surface patterns, AsciiDoc’s distinction between verse and literal blocks, and
Tree-sitter’s documented precedence/conflict/external-scanner constraints.

## Scope and design basis

I grounded this specification in the live repo, the official Tree-sitter documentation and external
scanner guide, the CommonMark spec, the Typst syntax reference, and the Asciidoctor block
documentation plus its more specific verse and literal-block pages. The repo README and examples
make clear that lists are already implemented too, but I have intentionally omitted list productions
because your requested reference scope centers on directives, headings, labels/references,
paragraphs, line breaks, calls, and expressions rather than list layout.

I also intentionally keep inline math opaque and do **not** admit Typst-style content literals as
ordinary expression values in this first EBNF, even though the manifest sketches richer scripting.
That choice keeps `[]` reserved for post-call block bodies and array literals only where syntactic
position makes them obvious, which sharply reduces conflict pressure for an initial Tree-sitter
grammar; the math grammar can be split out later as its own AST-producing layer, consistent with the
manifest’s goal of normalizing math into structured data rather than glyph soup.

## Reference EBNF

Read the grammar below with four contextual rules in mind. First, headings, directives, and block
calls are block forms only at the start of a logical line. Second, comments are lexical trivia, but
line endings are significant and must **not** be placed in Tree-sitter `extras`. Third, a single
newline between nonblank paragraph lines is a soft break, while `\\` inside inline text is a hard
line break (`\-` expands to a U+00AD soft hyphen, a break opportunity consumed by the line-breaker;
a bare trailing `\` is literal, with compiler diagnostic `MOS0038`). Fourth, `#verse[...]` preserves
line endings while `#pre[...]` and `#code[...]` preserve raw text; that mirrors CommonMark’s
paragraph/hard-break model and AsciiDoc’s verse-vs-literal distinction.

```ebnf
(* Mosaic reference grammar, tree-sitter oriented.
   Contextual notes:
   - heading, directives, and block_call are block forms only at the start
     of a logical line (start of file, after a blank line, or after a
     line_end inside a content body).
   - comments are lexical trivia.
   - soft_break is a contextual line_end that does not form a blank_line.
   - #pre[...] and #code[...] bodies are raw; #verse[...] preserves line
     endings but still parses inline markup per line.
*)

source_file         = { blank_line | block } ;

block               = set_directive
                    | import_directive
                    | include_directive
                    | heading
                    | verse_block
                    | pre_block
                    | code_block
                    | block_call
                    | paragraph
                    ;

set_directive       = "#set", hspace1, identifier, opt_hspace, argument_list,
                      [ line_end ] ;

import_directive    = "#import", hspace1, string,
                      [ opt_hspace, ":", opt_hspace, import_items ],
                      [ line_end ] ;

include_directive   = "#include", hspace1, string, [ line_end ] ;

import_items        = identifier, { opt_hspace, ",", opt_hspace, identifier } ;

heading             = heading_marker, hspace1, inline_sequence,
                      [ opt_hspace, block_label ],
                      [ line_end ] ;

heading_marker      = "=", { "=" } ;         (* semantic restriction: 1..6 *)

verse_block         = "#verse",
                      [ opt_hspace, argument_list ],
                      opt_ws, verse_body,
                      [ opt_hspace, block_label ],
                      [ line_end ] ;

pre_block           = "#pre",
                      [ opt_hspace, argument_list ],
                      opt_ws, raw_body,
                      [ opt_hspace, block_label ],
                      [ line_end ] ;

code_block          = "#code",
                      [ opt_hspace, argument_list ],
                      opt_ws, raw_body,
                      [ opt_hspace, block_label ],
                      [ line_end ] ;

block_call          = hash_call,
                      [ opt_hspace, block_label ],
                      [ line_end ] ;

paragraph           = [ leading_label ],
                      paragraph_segment,
                      { paragraph_join, paragraph_segment },
                      [ trailing_label ],
                      [ line_end ] ;

leading_label       = block_label, opt_hspace ;
trailing_label      = opt_hspace, block_label ;
block_label         = label ;

paragraph_segment   = inline_sequence ;
paragraph_join      = soft_break ;
soft_break          = line_end ;             (* contextual: not a blank_line *)

hash_call           = "#", qualified_name,
                      [ opt_hspace, argument_list ],
                      [ opt_ws, content_body ] ;

content_body        = "[", { blank_line | block }, "]" ;

verse_body          = "[",
                      [ verse_line, { line_end, verse_line }, [ line_end ] ],
                      "]" ;

verse_line          = { verse_inline | verse_text } ;

verse_inline        = strong_emphasis
                    | strong
                    | emphasis
                    | code_span
                    | inline_math
                    | reference
                    | label
                    | linebreak_call
                    | inline_call
                    | hard_break
                    | soft_hyphen_escape
                    | escaped_char
                    | loose_backslash
                    ;

verse_text          = verse_char, { verse_char } ;

raw_body            = "[", { raw_escape | raw_char | line_end }, "]" ;
raw_escape          = "\\]" | "\\\\" ;

inline_sequence     = inline_atom, { inline_atom } ;

inline_atom         = strong_emphasis
                    | strong
                    | emphasis
                    | code_span
                    | inline_math
                    | reference
                    | label
                    | linebreak_call
                    | inline_call
                    | hard_break
                    | soft_hyphen_escape
                    | escaped_char
                    | loose_backslash
                    | text
                    ;

inline_call         = hash_call | call_expr ;
linebreak_call      = "#linebreak", [ opt_hspace, argument_list ] ;

emphasis            = "*", emphasis_unit, { emphasis_unit }, "*" ;
strong              = "**", strong_unit, { strong_unit }, "**" ;
strong_emphasis     = "***", strong_emphasis_unit, { strong_emphasis_unit }, "***" ;

emphasis_unit       = strong_emphasis
                    | strong
                    | code_span
                    | inline_math
                    | reference
                    | label
                    | linebreak_call
                    | inline_call
                    | hard_break
                    | soft_hyphen_escape
                    | escaped_char
                    | loose_backslash
                    | emph_text
                    ;

strong_unit         = strong_emphasis
                    | emphasis
                    | code_span
                    | inline_math
                    | reference
                    | label
                    | linebreak_call
                    | inline_call
                    | hard_break
                    | soft_hyphen_escape
                    | escaped_char
                    | loose_backslash
                    | emph_text
                    ;

strong_emphasis_unit = strong
                     | emphasis
                     | code_span
                     | inline_math
                     | reference
                     | label
                     | linebreak_call
                     | inline_call
                     | hard_break
                     | soft_hyphen_escape
                     | escaped_char
                     | loose_backslash
                     | emph_text
                     ;

code_span           = "`", { code_char }, "`" ;
inline_math         = "$", { math_char | math_escape }, "$" ;
math_escape         = "\\", ? any Unicode scalar ? ;

reference           = "@", label_name ;
label               = "<", label_name, ">" ;

(* Inline line-break controls (mirrored from mos-parse inline lowering).
   `\\` is a hard line break (NodeKind::HardBreak). `\-` expands to a
   U+00AD soft hyphen consumed by the greedy line-breaker. `escaped_char`
   covers all other `\X` forms (e.g. `\#`, `\*`, `\[`, `\]`, `\<`); the
   compiler treats the unrecognised forms as literal `X`. A bare `\` that
   does not form one of those tokens is `loose_backslash`; at end of line
   the compiler additionally emits diagnostic `MOS0038`. *)
hard_break          = "\\\\" ;
soft_hyphen_escape  = "\\-" ;
escaped_char        = "\\", ? any Unicode scalar except "\", "-", or line_end ? ;
loose_backslash     = "\\" ;                 (* bare `\`, e.g. before line_end *)

text                = text_char, { text_char } ;

argument_list       = "(",
                      opt_ws,
                      [ argument,
                        { opt_ws, ",", opt_ws, argument },
                        [ opt_ws, "," ] ],
                      opt_ws,
                      ")" ;

argument            = attribute | expression ;
attribute           = identifier, opt_ws, ":", opt_ws, expression ;

expression          = string
                    | dimension
                    | number
                    | boolean
                    | null
                    | array
                    | object
                    | call_expr
                    | qualified_name
                    ;

call_expr           = qualified_name, opt_hspace, argument_list ;

array               = "[",
                      opt_ws,
                      [ expression,
                        { opt_ws, ",", opt_ws, expression },
                        [ opt_ws, "," ] ],
                      opt_ws,
                      "]" ;

object              = "{",
                      opt_ws,
                      [ attribute,
                        { opt_ws, ",", opt_ws, attribute },
                        [ opt_ws, "," ] ],
                      opt_ws,
                      "}" ;

(* ---------- lexical rules ---------- *)

line_end            = "\r\n" | "\n" | "\r" ;

hspace              = " " | "\t" ;
hspace1             = hspace, { hspace } ;

opt_hspace          = { hspace | comment } ;
opt_ws              = { hspace | line_end | comment } ;

comment             = line_comment | block_comment ;
line_comment        = "//", { ? any Unicode scalar except line_end ? } ;
block_comment       = "/*", { ? any Unicode scalar ? }, "*/" ;   (* non-nesting *)

digit               = ? ASCII digit ? ;
hex_digit           = ? ASCII hex digit ? ;

ident_start         = ? ASCII letter or "_" ? ;
ident_continue      = ? ASCII letter or digit or "_" or "-" ? ;

identifier          = ident_start, { ident_continue } ;
qualified_name      = identifier, { ".", identifier } ;

label_segment       = ident_start, { ident_continue } ;
label_name          = label_segment, { ":", label_segment } ;

string              = dq_string | sq_string ;

dq_string           = "\"", { dq_char | escape_seq }, "\"" ;
sq_string           = "'",  { sq_char | escape_seq }, "'" ;

dq_char             = ? any Unicode scalar except double quote, backslash, and line_end ? ;
sq_char             = ? any Unicode scalar except single quote, backslash, and line_end ? ;

escape_seq          = "\\",
                      ( "\"" | "'" | "\\" | "n" | "r" | "t"
                      | "[" | "]" | "<" | ">" | "*" | "`" | "$" | "@"
                      | "#" | "u", "{", hex_digit, { hex_digit }, "}" ) ;

number              = [ "+" | "-" ],
                      ( digit, { digit }, [ ".", { digit } ]
                      | ".", digit, { digit } ) ;

dimension           = number, unit ;
unit                = "pt" | "mm" | "cm" | "in" | "px"
                    | "em" | "rem" | "ch" | "fr" | "%" ;

boolean             = "true" | "false" ;
null                = "null" ;

blank_line          = { hspace }, line_end, { { hspace }, line_end } ;

text_char           = ? any Unicode scalar except line_end and the opener
                        characters "\", "*", "`", "$", "@", "<", "#" ? ;

emph_text           = ? any Unicode scalar except line_end and "\", "*" ? ;
code_char           = ? any Unicode scalar except "`" and line_end ? ;
math_char           = ? any Unicode scalar except "$" and line_end ? ;
raw_char            = ? any Unicode scalar except "]" ? ;
verse_char          = ? any Unicode scalar except line_end and "]" ? ;
```

## Construct examples and tree-sitter mapping

The first group of examples below comes directly from the live repo and manifest surface syntax:
`#set`, headings, `#figure`, labels/references, `#import`, and `#include`. The verse/pre/code rows
are normative examples for the extended reference grammar because those block forms are not yet
visible in the pre-alpha repo but are needed for a complete newline-aware editor grammar.

| EBNF construct                            | Example `.mos` snippet                          | Status   | Suggested CST nodes                                                      |
| ----------------------------------------- | ----------------------------------------------- | -------- | ------------------------------------------------------------------------ |
| `set_directive`                           | `#set page(margin: 24mm)`                       | repo     | `set_directive`, `identifier`, `argument_list`, `attribute`, `dimension` |
| `heading` + `block_label`                 | `= Methods <sec:methods>`                       | manifest | `heading`, `heading_marker`, `text`, `label`                             |
| `paragraph` + `reference` + inline styles | `See @fig:scan with *emphasis* and **strong**.` | manifest | `paragraph`, `reference`, `emphasis`, `strong`                           |
| `block_call`                              | `#figure(caption: "CTPA") <fig:ctpa>`           | repo     | `block_call`, `hash_call`, `argument_list`, `label`                      |
| `import_directive`                        | `#import "@mosaic/templates/article": article`  | manifest | `import_directive`, `string`, `import_items`                             |
| `include_directive`                       | `#include "sections/introduction.mos"`          | manifest | `include_directive`, `string`                                            |
| `verse_block`                             | `#verse[First line`<br>`Second *line*]`         | proposed | `verse_block`, `verse_body`, `emphasis`                                  |
| `pre_block`                               | `#pre[  exact spacing]`                         | proposed | `pre_block`, `raw_body`                                                  |
| `code_block`                              | `#code(lang: "rust")[fn main() {}]`             | proposed | `code_block`, `argument_list`, `raw_body`                                |

For Tree-sitter naming, keep the CST shallow and stable. A first grammar only needs `source_file`,
`heading`, `paragraph`, `set_directive`, `import_directive`, `include_directive`, `block_call`,
`content_body`, `verse_block`, `pre_block`, `code_block`, `argument_list`, `attribute`, `array`,
`object`, `call_expr`, `label`, `reference`, `emphasis`, `strong`, `strong_emphasis`, `code_span`,
`inline_math`, `text`, `string`, `number`, `dimension`, `identifier`, and `comment`. Use
`word: $.identifier`, keep `newline` explicit rather than in `extras`, and use `supertypes` only for
abstract buckets such as `_block`, `_inline`, and `_expression`.

| Tree-sitter concern | Recommendation                                                                        |
| ------------------- | ------------------------------------------------------------------------------------- |
| `extras`            | Horizontal whitespace and comments only; **exclude** newline                          |
| `word`              | `identifier`                                                                          |
| `supertypes`        | `_block`, `_inline`, `_expression`                                                    |
| lexical precedence  | `***` > `**` > `*`; reference/label tokens over generic text                          |
| parse conflicts     | `[strong, strong_emphasis]`, `[emphasis, strong_emphasis]`, `[paragraph, block_call]` |
| likely externals    | `blank_line`, raw `pre/code` body delimiters and content                              |

## Ambiguities and scanner guidance

The important ambiguities here are not theoretical; they are exactly the ones that will hurt a
Tree-sitter grammar if left implicit. Tree-sitter’s own guidance is to solve ordinary token overlap
with lexical precedence and explicit parse conflicts, and to reserve external scanners for tokens
that are impossible or simply too inconvenient to express with regexes alone.

| Ambiguity                                              | Why it happens                                                         | Recommended resolution                                                                                                                                                                                                                                                                                                                           |
| ------------------------------------------------------ | ---------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `***x***` vs nested `**` + `*`                         | Three delimiter lengths compete on the same prefix                     | Give `***` highest lexical precedence, then `**`, then `*`; retain parse conflicts between `strong` and `strong_emphasis` and between `emphasis` and `strong_emphasis`                                                                                                                                                                           |
| soft paragraph break vs blank line                     | Paragraphs are line-oriented; blank lines terminate blocks             | Treat `blank_line` as its own token; parse paragraphs from contiguous nonblank lines only                                                                                                                                                                                                                                                        |
| `\\` / `\-` / `\X` / bare `\`                          | One leading char overlaps four distinct inline atoms                   | Define `hard_break = "\\\\"` and `soft_hyphen_escape = "\\-"` as 2-char tokens that win the lexer's longest-match; `escaped_char` matches `\X` with `\` and `-` excluded from `X`; a bare `\` (typically before `line_end`) falls through to length-1 `loose_backslash` and is literal text (compiler emits `MOS0038` in the trailing-line case) |
| `<label>` vs literal `<` text                          | Labels reuse angle brackets                                            | Recognize label only for `<` + valid `label_name` + `>` with no interior whitespace; otherwise leave `<` to text or require `\<`                                                                                                                                                                                                                 |
| `@label` vs email-like prose                           | `@` is used for references                                             | Recognize references only outside raw/code/math blocks and only when followed by `label_name`; require escaping for literal email-like text until autolink/email syntax exists                                                                                                                                                                   |
| `#foo[...]` vs array literal                           | Both use brackets                                                      | After `#`-prefixed call headers, `[` always opens `content_body`; arrays occur only in expression position, typically inside `(...)`, after `:`, or after `,`                                                                                                                                                                                    |
| raw `#pre[...]` or `#code[...]` content containing `]` | Raw bodies need a terminator                                           | Support `\]` in raw mode and treat the body as scanner-driven raw text, not recursively parsed content                                                                                                                                                                                                                                           |
| multiline comments around blank lines                  | Comments can cross lines while blank lines are structurally meaningful | Keep comments lexical, but let line endings remain visible to the parser; do not let “newline as trivia” erase block boundaries                                                                                                                                                                                                                  |

For `tree-sitter-mosaic`, I would start with an external scanner only for `blank_line` and the raw
`#pre` / `#code` body delimiters and content. Inline line-break controls (`hard_break`,
`soft_hyphen_escape`, `escaped_char`, `loose_backslash`) stay in `grammar.js` as regular tokens;
2-char escapes win the lexer's longest-match, and the bare `\` falls through to the 1-char
`loose_backslash`. I would **not** start with an external scanner for emphasis: a conservative
precedence-based grammar for `***` / `**` / `*` is enough initially, and full CommonMark-grade
delimiter-run semantics can come later if real documents demand them. That matches Tree-sitter’s own
advice about externals, and it keeps the first grammar much easier to reason about.

## Test corpus and parsing flow

This corpus is deliberately small but high-yield: each example isolates one block or inline feature
and keeps the expected CST obvious. It is suitable both as a prose reference set and as
`tree-sitter test` corpus input for the first parser iteration.

| Case                | Snippet                                                                           | Expected parse highlight                                                                                                                                    |
| ------------------- | --------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------- |
| settings            | `#set text(font: "Noto Sans", size: 11pt)`                                        | one `set_directive` with two `attribute` children                                                                                                           |
| heading label       | `= Intro <sec:intro>`                                                             | one `heading`, one trailing `label`                                                                                                                         |
| soft break          | `Line one`<br>`line two`                                                          | one `paragraph`, one `soft_break`, **not** two paragraphs                                                                                                   |
| explicit line break | `Line one \\`<br>`Line two`                                                       | one `paragraph` containing a `hard_break` inline atom between the two text runs (a bare trailing `\` is `loose_backslash` + `soft_break`, not a hard break) |
| refs and styles     | `See @sec:intro with *emph* and **strong** and \`code\`.`                         | one `paragraph` containing `reference`, `emphasis`, `strong`, `code_span`                                                                                   |
| import/include      | `#import "@mosaic/templates/article": article`<br>`#include "sections/intro.mos"` | two top-level directive nodes                                                                                                                               |
| labeled call        | `#figure(image: "demo.png", caption: "Demo") <fig:demo>`                          | one `block_call` with `argument_list` and trailing `label`                                                                                                  |
| verse               | `#verse[First line`<br>`Second *line*]`                                           | one `verse_block`; line boundaries preserved; inline emphasis still parsed                                                                                  |
| code                | `#code(lang: "rust")[fn main() {}`<br>`]`                                         | one `code_block`; body remains raw; no inline parsing inside                                                                                                |
| arrays and objects  | `#set layout(allowed: [top, bottom], opts: {widows: true})`                       | nested `array` and `object` expressions inside one `argument_list`                                                                                          |

The parsing pipeline should mirror the repo’s own architecture: parse source into a concrete syntax
tree, lower to semantic nodes, resolve imports/labels/references/counters, and only then enter
layout. That is exactly how the manifest and README describe the language/compiler split.

```mermaid
flowchart LR
    A[Source .mos file] --> B[Parse to concrete syntax tree]
    B --> C[Lower to semantic document nodes]
    C --> D[Resolve imports labels references counters]
    D --> E[Layout blocks and pages]
```

This is the right first frozen target for `tree-sitter-mosaic`: line-aware without being
Markdown-chaotic, semantic enough for labels and calls, and intentionally conservative about the two
biggest ambiguity source-delimiter runs and bracket bodies.
