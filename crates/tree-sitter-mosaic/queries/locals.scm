; Mosaic scope and binding tracking.
;
; Powers rename, "go to definition", and "find references" in editors that
; respect tree-sitter locals queries (nvim-treesitter, Helix).

; --- Scopes -----------------------------------------------------------------

(source_file) @local.scope
(content_block) @local.scope

; --- Definitions ------------------------------------------------------------

; `#import "lib.mos": foo, bar` — each trailing identifier is a binding.
(import_directive
  (identifier) @local.definition.import)

; `<intro:setup>` — label introduction.
(label
  name: (label_name) @local.definition.label)

; --- References -------------------------------------------------------------

; `@intro:setup` — label use.
(reference
  target: (label_name) @local.reference)

; `#name(...)` — function/template call site.
(call
  function: (identifier) @local.reference)
