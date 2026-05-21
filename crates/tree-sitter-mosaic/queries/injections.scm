; Mosaic language injections.
;
; The inline math body uses LaTeX-like syntax (manifest §6), so we inject a
; latex parser when available. The `#offset!` predicate strips the `$`
; delimiters so the embedded parser sees pure math source.

((inline_math) @injection.content
  (#set! injection.language "latex")
  (#offset! @injection.content 0 1 0 -1))

((code_block
   arguments: (argument_list
     (attribute
       key: (identifier) @_k
       value: (string [
         (string_double_content)
         (string_single_content)
       ] @injection.language)))
   body: (raw_body
     (raw_body_content) @injection.content))
  (#eq? @_k "lang")
  (#set! injection.include-children))

; Code spans carry no language tag in the current syntax, so we don't inject
; a default. Editors that want guesswork can add a project-local override.
