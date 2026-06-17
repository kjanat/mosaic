; Mosaic scope and binding tracking.
;
; Powers rename, "go to definition", and "find references" in editors that
; respect tree-sitter locals queries (nvim-treesitter, Helix).

; --- Scopes -----------------------------------------------------------------

(source_file) @local.scope
(content_body) @local.scope

; --- Definitions ------------------------------------------------------------

; `#import "lib.mos": foo, bar`; each trailing identifier is a binding.
(import_directive
  items: (import_items
    (identifier) @local.definition))

; `<intro:setup>`: label introduction.
(label
  name: (label_name) @local.definition)

; --- References -------------------------------------------------------------

; `@intro:setup`: label use.
(reference
  target: (label_name) @local.reference)

; `@page(intro:setup)`: page-reference label use.
(page_reference
  target: (label_name) @local.reference)

; `#name(...)`: block / inline call site.
(hash_call
  function: (qualified_name) @local.reference)

; `name(...)`: expression-position call (rare in surface syntax).
(call_expr
  function: (qualified_name) @local.reference)
