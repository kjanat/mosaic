# zed-mosaic

Zed editor extension for the [Mosaic](https://github.com/kjanat/mosaic) typesetting language
(`.mos`).

Status: **pre-alpha dev-install only.** Grammar URL in `extension.toml` is a local `file://`
placeholder — see "Grammar URL caveat" below.

## What it provides

- Language registration for `.mos` files (scope `source.mosaic`).
- Tree-sitter highlighting via the `tree-sitter-mosaic` grammar (headings, emphasis, raw blocks,
  labels, references, directives, escapes).
- Injections: LaTeX into inline math `$…$`, and the language declared in `#code(lang: "...")[…]` raw
  blocks.

Not yet wired (follow-up): `brackets.scm`, `indents.scm`, `outline.scm`, `textobjects.scm`, language
server.

## Install as a dev extension

1. Open Zed.
2. Command palette → `zed: install dev extension`.
3. Select this directory (`crates/zed-mosaic/`).
4. Open any `examples/*/main.mos` to confirm highlighting and the language picker shows "Mosaic".

## Grammar URL caveat

Zed's `[grammars.<name>]` block clones the configured `repository` and expects `grammar.js` /
`src/parser.c` at the repo root. The Mosaic grammar currently lives in a subdirectory of the
monorepo (`crates/tree-sitter-mosaic`), so the default `file://` URL in `extension.toml` won't fetch
successfully on machines other than the author's.

To make this extension functional locally before `tree-sitter-mosaic` is split into a standalone
repo:

1. Clone or copy `crates/tree-sitter-mosaic/` to a standalone directory:

   ```bash
   git clone https://github.com/kjanat/mosaic /tmp/mosaic-clone
   cp -r /tmp/mosaic-clone/crates/tree-sitter-mosaic ~/src/tree-sitter-mosaic
   (cd ~/src/tree-sitter-mosaic && git init && git add -A && git commit -m "import")
   ```

2. Update `[grammars.mosaic].repository` in `extension.toml` to the standalone path, and `rev` to
   the current `HEAD` SHA.

3. Reinstall the dev extension.

The clean fix is to publish `tree-sitter-mosaic` as `github.com/kjanat/tree-sitter-mosaic` and
reference that URL — tracked as a follow-up.

## Query files

`languages/mosaic/highlights.scm` and `injections.scm` are copies of the canonical files at
`crates/tree-sitter-mosaic/queries/`. Keep them in sync via:

```bash
just sync-zed-queries
```

Zed does not consume `locals.scm` or `tags.scm`, so those query files remain only in the parser
crate.
