; Mosaic document outline.
;
; Headings are the primary navigation structure. Labels are included so
; cross-reference targets can be found from the outline too.

[
  (section1
    (heading
      marker: (heading_marker) @context
      content: (inline_sequence) @name))
  (section2
    (heading
      marker: (heading_marker) @context
      content: (inline_sequence) @name))
  (section3
    (heading
      marker: (heading_marker) @context
      content: (inline_sequence) @name))
  (section4
    (heading
      marker: (heading_marker) @context
      content: (inline_sequence) @name))
  (section5
    (heading
      marker: (heading_marker) @context
      content: (inline_sequence) @name))
  (section6
    (heading
      marker: (heading_marker) @context
      content: (inline_sequence) @name))
] @item

(label
  name: (label_name) @name) @item
