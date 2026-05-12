; Mosaic language injections.
;
; The inline math body uses LaTeX-like syntax (manifest §6), so we inject a
; latex parser when available. The `#offset!` predicate strips the `$`
; delimiters so the embedded parser sees pure math source.

((inline_math) @injection.content
  (#set! injection.language "latex")
  (#offset! @injection.content 0 1 0 -1))

; `#code(lang: "rust")[...]` — inject the named language into the raw body.
; The grammar exposes `raw_body` as the body node; `raw_body_content` chunks
; carry the actual text. `string` is a single token covering the quotes, so
; we use `#offset!` to strip the surrounding `"` / `'` characters before the
; capture is consumed as `@injection.language`.
((code_block
   arguments: (argument_list
     (attribute
       key: (identifier) @_k
       value: (string) @injection.language))
   body: (raw_body) @injection.content)
  (#eq? @_k "lang")
  (#offset! @injection.language 0 1 0 -1))

; Code spans carry no language tag in the current syntax, so we don't inject
; a default. Editors that want guesswork can add a project-local override.
