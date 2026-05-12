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

(emphasis) @markup.italic
(strong) @markup.strong
(strong_emphasis) @markup.strong

; --- Code & math ------------------------------------------------------------

(code_span) @markup.raw
(inline_math) @markup.math
(display_math) @markup.math

; --- Labels & references ----------------------------------------------------

(label) @label
(reference) @markup.link.reference
(label_name) @markup.link.label

; --- Escapes ----------------------------------------------------------------

(escape) @string.escape
(linebreak_escape) @string.escape

; --- Calls (override @variable on the function identifier) ------------------

(call
  function: (identifier) @function)

(attribute
  key: (identifier) @variable.parameter)

; --- Punctuation ------------------------------------------------------------

"#" @punctuation.special
"@" @punctuation.special
"$" @punctuation.special
"$$" @punctuation.special

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
] @punctuation.delimiter
