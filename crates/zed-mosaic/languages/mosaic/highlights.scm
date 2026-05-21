; Mosaic syntax highlights.
;
; Capture names follow the broad tree-sitter convention shared by
; nvim-treesitter, Helix, and Zed. Later matches override earlier ones
; on the same node, so specific captures come after generic fallbacks.

; --- Comments ---------------------------------------------------------------

(comment) @comment

; --- Literals ---------------------------------------------------------------

(string) @string
(number) @number
(dimension) @number
(boolean) @boolean
(null) @constant.builtin

; --- Identifiers (generic fallback; overridden below) -----------------------

(identifier) @variable

; --- Block directives -------------------------------------------------------

"#set" @keyword
"#import" @keyword.import
"#include" @keyword.import

"#verse" @keyword
"#pre" @keyword
"#code" @keyword
"#linebreak" @keyword

(set_directive
  target: (identifier) @type)

(import_directive
  path: (string) @string.special.path)

(include_directive
  path: (string) @string.special.path)

; --- Headings ---------------------------------------------------------------

(heading_marker) @markup.heading.marker
(heading) @markup.heading

; --- Inline emphasis --------------------------------------------------------

(emphasis) @emphasis
(strong) @emphasis.strong
(strong_emphasis) @emphasis.strong

; --- Code, math, raw blocks -------------------------------------------------

(code_span) @text.literal
(inline_math) @markup.math
(verse_block) @markup.quote
(pre_block) @markup.raw.block
(code_block) @markup.raw.block
(raw_body_content) @text.literal

; --- Labels & references ----------------------------------------------------

(label) @label
(block_label) @label
(reference) @markup.link.reference
(label_name) @markup.link.label

; --- Escapes ----------------------------------------------------------------

(escaped_char) @string.escape
(linebreak_escape) @string.escape

; --- Calls (override @variable on the function identifier) ------------------

(hash_call
  function: (qualified_name) @function)

(call_expr
  function: (qualified_name) @function)

(linebreak_call) @function.builtin

(attribute
  key: (identifier) @variable.parameter)

; --- Punctuation ------------------------------------------------------------

"#" @punctuation.special
"@" @punctuation.special
"$" @punctuation.special

[
  "*"
  "**"
  "***"
] @punctuation.special

[
  "<"
  ">"
] @punctuation.bracket

[
  "("
  ")"
  "["
  "]"
  "{"
  "}"
] @punctuation.bracket

[
  ","
  ":"
  "."
] @punctuation.delimiter
