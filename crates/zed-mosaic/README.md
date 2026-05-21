# zed-mosaic

Zed editor extension for the [Mosaic](https://github.com/kjanat/mosaic) typesetting language
(`.mos`).

## What it provides

- Language registration for `.mos` files (scope `source.mosaic`).
- Tree-sitter highlighting via the `tree-sitter-mosaic` grammar (headings, emphasis, raw blocks,
  labels, references, directives, escapes).
- Injections: LaTeX into inline math `$…$`, and the language declared in `#code(lang: "...")[…]` raw
  blocks.
- Outline entries for headings and label targets.
- Bracket matching, indentation, and Vim textobjects from Tree-sitter queries.

Not yet wired (follow-up): language server.

## Install as a dev extension

1. Open Zed.
2. Command palette → `zed: install dev extension`.
3. Select this directory (`crates/zed-mosaic/`).
4. Open any `examples/*/main.mos` to confirm highlighting and the language picker shows "Mosaic".

The grammar is loaded from `crates/tree-sitter-mosaic` (sibling crate in this repo) via
`[grammars.mosaic] path = …` in `extension.toml`.

## Query files

Zed query files in `languages/mosaic/` are copies of the canonical files at
`crates/tree-sitter-mosaic/queries/`. Keep them in sync via:

```bash
just sync-zed-queries
```

Zed's extension query loader does not consume Tree-sitter `locals.scm` or `tags.scm` under those
filenames. Navigation and editing features use Zed query files such as `outline.scm`,
`brackets.scm`, `indents.scm`, and `textobjects.scm`.
