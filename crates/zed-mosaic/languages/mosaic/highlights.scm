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

"#set" @keyword
"#import" @keyword
"#include" @keyword

"#verse" @keyword
"#pre" @keyword
"#code" @keyword
"#linebreak" @keyword

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

(label) @label
(block_label) @label
(citation) @link_text
(reference) @link_text
(label_name) @link_uri

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
