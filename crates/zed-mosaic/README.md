# zed-mosaic

Zed editor extension for the [Mosaic] typesetting language (`.mos`).

## What it provides

- Language registration for `.mos` files (scope `source.mosaic`).
- Tree-sitter highlighting via the `tree-sitter-mosaic` grammar (headings, emphasis, raw blocks,
  labels, references, directives, escapes).
- Injections: LaTeX into inline math `$…$`, and the language declared in `#code(lang: "...")[…]` raw
  blocks.
- Outline entries for headings and label targets.
- Bracket matching, indentation, and Vim textobjects from Tree-sitter queries.
- Default semantic token rules for the future Mosaic language server.

Not yet wired (follow-up): language server.

## Install as a dev extension

1. Open Zed.
2. Command palette → `zed: install dev extension`.
3. Select this directory ([`crates/zed-mosaic/`]).
4. Open any `examples/*/main.mos` to confirm highlighting and the language picker shows "Mosaic".

The grammar is loaded from [`crates/tree-sitter-mosaic`] (sibling crate in this repo) via
`[grammars.mosaic] path = …` in [`extension.toml`].

## Query files

Zed query files in [`languages/mosaic/`] are copies of the canonical files at
[`crates/tree-sitter-mosaic/queries/`]. Keep them in sync via:

```bash
just sync-zed-queries
```

Zed's extension query loader does not consume Tree-sitter `locals.scm` or `tags.scm` under those
filenames. Navigation and editing features use Zed query files such as [`outline.scm`],
[`brackets.scm`], [`indents.scm`], and [`textobjects.scm`].

## Semantic tokens

[`languages/mosaic/semantic_token_rules.json`] reserves the `mosaic*` custom token namespace for the
future LSP and maps those semantic tokens to Zed theme styles. It is inactive until the extension
registers `mosaic-lsp` and Zed has semantic tokens enabled (`combined` or `full`)[^semantic-tokens].

<!-- sorted case-sensitive -->

[^semantic-tokens]: https://zed.dev/docs/extensions/languages#syntax-highlighting-with-semantic-tokens

[Mosaic]: https://github.com/kjanat/mosaic "Typesetting language for technical documents"
[`brackets.scm`]: languages/mosaic/brackets.scm
[`crates/tree-sitter-mosaic/queries/`]: ../tree-sitter-mosaic/queries/
[`crates/tree-sitter-mosaic`]: ../tree-sitter-mosaic/
[`crates/zed-mosaic/`]: ../zed-mosaic/
[`extension.toml`]: extension.toml
[`indents.scm`]: languages/mosaic/indents.scm
[`languages/mosaic/`]: languages/mosaic/
[`languages/mosaic/semantic_token_rules.json`]: languages/mosaic/semantic_token_rules.json
[`outline.scm`]: languages/mosaic/outline.scm
[`textobjects.scm`]: languages/mosaic/textobjects.scm
