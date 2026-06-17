; Mosaic text objects.

(heading
  content: (inline_sequence) @class.inside) @class.around

(list) @class.around

(list_item
  content: (inline_sequence) @function.inside) @function.around

(verse_block
  body: (verse_body) @class.inside) @class.around

(pre_block
  body: (raw_body
    (raw_body_content) @class.inside)) @class.around

(code_block
  body: (raw_body
    (raw_body_content) @class.inside)) @class.around

(block_call
  (hash_call
    body: (content_body) @class.inside)) @class.around

(block_call) @function.around

(comment)+ @comment.around
