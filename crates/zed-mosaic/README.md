# zed-mosaic

Zed editor extension for the [Mosaic](https://github.com/kjanat/mosaic) typesetting language
(`.mos`).

Status: **pre-alpha dev-install only.** Grammar is fetched from a `git subtree split` branch
(`tree-sitter-mosaic-root`) via a `file://` URL — see "Grammar URL" below.

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

## Grammar URL

Zed's `[grammars.<name>]` block clones the configured `repository` and expects `grammar.js` /
`src/parser.c` at the repo root. The grammar lives in `crates/tree-sitter-mosaic/` — a subdirectory.
To bridge this without a separate published repo, we maintain a `tree-sitter-mosaic-root` branch
produced by `git subtree split`: its root tree IS the contents of `crates/tree-sitter-mosaic/`, so
Zed sees `grammar.js` where it expects it.

`extension.toml` currently points at a local `file://` clone of the monorepo. To set it up on a
fresh machine:

1. Clone the monorepo somewhere stable (e.g. `~/projects/mosaic`).
2. From inside that clone, create the split branch:

   ```bash
   just refresh-zed-grammar
   ```

3. Edit `extension.toml`'s `[grammars.mosaic].repository` to point at your local clone path.
4. Install the dev extension (see above).

When grammar code changes, re-run `just refresh-zed-grammar` and reinstall the extension (or run
`zed: rebuild grammar` from the palette).

The long-term fix is to publish `tree-sitter-mosaic` as `github.com/kjanat/tree-sitter-mosaic` and
switch `repository` to that `https://…` URL. Tracked as a follow-up.

## Query files

`languages/mosaic/highlights.scm` and `injections.scm` are copies of the canonical files at
`crates/tree-sitter-mosaic/queries/`. Keep them in sync via:

```bash
just sync-zed-queries
```

Zed does not consume `locals.scm` or `tags.scm`, so those query files remain only in the parser
crate.
