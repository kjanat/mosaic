; Mosaic language injections.
;
; The math node bodies use LaTeX-like syntax (manifest §6), so we inject a
; latex parser when available. The `#offset!` predicate strips the `$`/`$$`
; delimiters so the embedded parser sees pure math source.

((inline_math) @injection.content
  (#set! injection.language "latex")
  (#offset! @injection.content 0 1 0 -1))

((display_math) @injection.content
  (#set! injection.language "latex")
  (#offset! @injection.content 0 2 0 -2))

; Code spans carry no language tag in the current syntax, so we don't inject
; a default. Editors that want guesswork can add a project-local override.
