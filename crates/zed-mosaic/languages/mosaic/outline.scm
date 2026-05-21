; Mosaic document outline.
;
; Headings are the primary navigation structure. Labels are included so
; cross-reference targets can be found from the outline too.

(heading
  content: (inline_sequence) @name) @item

(label
  name: (label_name) @name) @item
