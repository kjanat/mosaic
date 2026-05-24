# tree-sitter-mosaic

Tree-sitter grammar for Mosaic `.mos` documents.

This crate is editor-facing syntax infrastructure. It parses source into a concrete syntax tree for
highlighting, navigation, textobjects, injections, and other incremental editor features. It is not
the compiler parser used by `mos check` or `mos build`.

## Supported Syntax

- Line comments `// ...` and block comments `/* ... */`.
- `#set`, `#import`, and `#include` directives.
- Headings with `=` through `======` markers.
- Ordered and unordered lists.
- Paragraphs with soft breaks, `\\` hard breaks, and `\-` soft-hyphen escapes matching the
  compiler's inline parser.
- Inline emphasis `*...*`, strong `**...**`, strong emphasis `***...***`, code spans, inline math,
  labels, references, escapes, and `#linebreak`.
- Block and inline `#name(...)` calls, qualified names such as `#std.image(...)`, optional content
  bodies `[...]`, and trailing block labels `<fig:demo>`.
- Expression values in argument lists: strings, numbers, dimensions, booleans, `null`, arrays,
  objects, call expressions, and qualified names.
- `#verse[...]`, `#pre[...]`, and `#code[...]` blocks. `#pre` and `#code` bodies are raw text.

The grammar follows the repository `EBNF.md` shape, with pragmatic Tree-sitter choices documented in
`grammar.js` where ambiguity would hurt incremental parsing.

## Generated Artifacts

Source of truth:

- `grammar.js` defines the grammar.
- `src/scanner.c` implements external tokens for blank lines and raw block bodies.
- `queries/*.scm` define editor queries for highlights, injections, outline, indents, textobjects,
  runnables, and overrides.

Generated or consumed artifacts:

- `src/parser.c`, `src/grammar.json`, and `src/node-types.json` are produced by Tree-sitter.
- `bindings/rust/lib.rs` exposes `LANGUAGE` and `NODE_TYPES` for Rust users.
- `bindings/node/*`, `binding.gyp`, `CMakeLists.txt`, and `Makefile` support Node/C/native builds.
- `tree-sitter.json` registers the `mosaic` grammar, `source.mosaic` scope, and `.mos` file type.

## Test And Regenerate

Run from `crates/tree-sitter-mosaic/`:

```bash
npm test
npm run generate
npm run parse -- test/inputs/headings__heading_level_1.mos
```

Equivalent direct commands:

```bash
tree-sitter test
tree-sitter generate
tree-sitter parse test/inputs/headings__heading_level_1.mos
```

Rust binding smoke test:

```bash
cargo test -p tree-sitter-mosaic
```

The corpus lives in `test/corpus/*.txt`; sample parse inputs live in `test/inputs/*.mos`; highlight
fixtures live in `test/highlight/*.mos`.

## Rust Example

```rust
let code = "= Hello\n\nA short paragraph.\n";
let mut parser = tree_sitter::Parser::new();
let language = tree_sitter_mosaic::LANGUAGE;
parser.set_language(&language.into())?;
let tree = parser.parse(code, None).ok_or("parse failed")?;
assert!(!tree.root_node().has_error());
# Ok::<(), Box<dyn std::error::Error>>(())
```

## Known Non-goals

- Not semantic lowering, label resolution, layout, PDF emission, package resolution, or execution.
- Not a full CommonMark parser; Mosaic only borrows some lightweight markup shapes.
- Not current shipped compiler truth for every syntax form. Some editor grammar forms, especially
  `#verse`, `#pre`, `#code`, imports, and includes, are ahead of the pre-alpha CLI pipeline.
- Not full language-server behavior. `zed-mosaic` consumes this grammar today; LSP integration
  remains separate.
