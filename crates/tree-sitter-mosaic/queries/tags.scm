; Mosaic code-navigation tags.
;
; Drives ctags-like symbol indexers. Each capture must come in pairs:
; a `@definition.*` or `@reference.*` on the outer node, plus a `@name`
; on the identifying sub-node.

; --- Definitions ------------------------------------------------------------

; Headings act as section definitions; the rendered content is the name.
(heading
  content: (inline_sequence) @name) @definition.section

; `<intro:setup>` declares a target label.
(label
  name: (label_name) @name) @definition.label

; --- References -------------------------------------------------------------

; `#name(...)` — template/function invocation.
(call
  function: (identifier) @name) @reference.call

; `@intro:setup` — label use.
(reference
  target: (label_name) @name) @reference.label

; `#import "lib.mos"` / `#include "..."` — module references by path.
(import_directive
  path: (string) @name) @reference.import

(include_directive
  path: (string) @name) @reference.import
