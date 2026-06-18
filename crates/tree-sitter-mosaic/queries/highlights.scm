; Mosaic syntax highlights.
;
; Capture names follow Zed's supported syntax capture set. When a node has
; fallback captures, Zed resolves them right-to-left.

; --- Comments ---------------------------------------------------------------

(comment) @comment
(shebang) @preproc

; --- Literals ---------------------------------------------------------------

(string) @string
(escape_sequence) @string.escape
(number) @number
(dimension) @number
(boolean) @boolean
(null) @constant.builtin

; --- Identifiers (generic fallback; overridden below) -----------------------

(identifier) @variable

; --- Block directives -------------------------------------------------------

; `#set`, `#import`, and `#include` use hidden boundary tokens so `#setpage`
; stays a generic call. Their visible `#` is highlighted by punctuation below.
(verse_block "verse" @keyword)
(pre_block "pre" @keyword)
(code_block "code" @keyword)
(linebreak_call "linebreak" @keyword)

(set_directive
  target: (identifier) @type)

(import_directive
  path: (string) @string @string.special)

(include_directive
  path: (string) @string @string.special)

; --- Headings ---------------------------------------------------------------

(heading_marker) @punctuation.special

(heading
  content: (inline_sequence) @title)

; --- Lists ------------------------------------------------------------------

(unordered_list_marker) @punctuation.list_marker
(ordered_list_marker) @punctuation.list_marker

; --- Inline emphasis --------------------------------------------------------

(emphasis
  (emph_text) @emphasis)

(strong
  (emph_text) @emphasis.strong)

(strong_emphasis
  (emph_text) @emphasis.strong)

; --- Code, math, raw blocks -------------------------------------------------

(code_span
  (code_text) @text.literal)

(code_span
  (code_escape) @text.literal)

(inline_math) @text.literal
(verse_block) @text.literal
(raw_body_open) @punctuation.special
(raw_body_content) @text.literal
(raw_body_close) @punctuation.special

; --- Labels & references ----------------------------------------------------

(block_label) @label
(label
  name: (label_name) @label)

(citation) @link_text
(citation
  target: (label_name) @string.special.symbol)

(reference) @link_text
(reference
  target: (label_name) @string.special.symbol)

(page_reference) @link_text
(page_reference
  target: (label_name) @string.special.symbol)

; --- Escapes ----------------------------------------------------------------

(escaped_char) @string.escape
(hard_break) @string.escape
(soft_hyphen_escape) @string.escape

; --- Calls (override @variable on the function identifier) ------------------

(hash_call
  function: (qualified_name) @function)

(call_expr
  function: (qualified_name) @function)

(linebreak_call) @function

(attribute
  key: (identifier) @property)

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
