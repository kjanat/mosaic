# zed-mosaic

Zed editor extension for the [Mosaic] typesetting language (`.mos`). It is a language-support
extension only: register Mosaic files, load the Tree-sitter grammar, and provide editor
queries/tasks.

## What it provides

- `.mos` language registration as `Mosaic`, grammar `mosaic`, Tree-sitter scope `source.mosaic`.
- Highlighting through the sibling [`tree-sitter-mosaic`] grammar.
- Zed queries for highlights, injections, outline, brackets, indents, textobjects, overrides, and
  runnables.
- Editor config for two-space indentation, soft wrap, comments, bracket pairs, word characters,
  auto-closing/surround pairs, and list continuation.
- Runnables/tasks for `mos build` and `mos build --open` on the current file.
- Reserved semantic token style rules for a future Mosaic language server.

The Rust/WASM entrypoint in [`src/lib.rs`] only calls `register_extension!`; no LSP, commands, or
runtime hooks are currently implemented.

## Grammar registration

[`extension.toml`] declares extension id `mosaic`, language `Mosaic`, and grammar `mosaic`. The
grammar is loaded from this repository at [`crates/tree-sitter-mosaic`]. Keep the pinned grammar
`rev` in [`extension.toml`] aligned with the commit containing the generated Tree-sitter artifacts.

During local grammar development, the commented `file:///home/kjanat/projects/mosaic` stanza in
[`extension.toml`] shows the intended shape for pointing Zed at a local checkout/branch.

## Install as a dev extension

1. Open Zed.
2. Command palette → `zed: install dev extension`.
3. Select this directory ([`crates/zed-mosaic/`]).
4. Open any `examples/*/main.mos` to confirm highlighting and the language picker shows "Mosaic".

## Queries and config

[`languages/mosaic/config.toml`] maps `.mos` files and `mos` code fences/modelines to Mosaic. Query
files in [`languages/mosaic/`] are Zed-facing copies/configuration. The canonical Tree-sitter query
sources live at [`crates/tree-sitter-mosaic/queries/`]. Keep shared query files in sync via:

```bash
just sync-zed-queries
```

Zed's extension query loader does not consume Tree-sitter `locals.scm` or `tags.scm` under those
filenames. Navigation and editing features use Zed query files such as [`outline.scm`],
[`brackets.scm`], [`indents.scm`], [`textobjects.scm`], [`runnables.scm`], and [`overrides.scm`].

## Development and regeneration

- Build/check the extension crate with normal workspace Rust commands, for example
  `cargo check -p zed-mosaic`.
- Regenerate the Tree-sitter parser from [`crates/tree-sitter-mosaic`] with `npm run generate` when
  `grammar.js` changes.
- Run `just sync-zed-queries` after changing canonical query files in
  [`crates/tree-sitter-mosaic/queries/`].
- Reinstall/reload the dev extension in Zed after changing `extension.toml`, language config,
  queries, or the WASM entrypoint.

## Tasks

[`languages/mosaic/tasks.json`] provides document-level runnables for:

- `Mosaic: Build PDF`
- `Mosaic: Build and Open PDF`

The default build task expects `mos` on `PATH` and runs from the current file's directory so
`build/<entry-stem>.pdf` lands next to the document source. Build-and-open uses `mos build --open`,
which selects the platform opener by default. Users can override these tasks in project or global
Zed `tasks.json` files by binding their own task to the same runnable tags.

## Semantic tokens

[`languages/mosaic/semantic_token_rules.json`] reserves the `mosaic*` custom token namespace for the
future LSP and maps those semantic tokens to Zed theme styles. It is inactive until the extension
registers `mos-lsp` and Zed has semantic tokens enabled (`combined` or `full`)[^semantic-tokens].

## Known non-goals

- No language server integration yet. The workspace has a `mos-lsp` binary, but this extension does
  not spawn or configure it.
- No formatter, code actions, completion, diagnostics, package resolution, preview pane, or watch
  mode.
- No compiler behavior lives here. `mos check`/`mos build` remain owned by the main Mosaic crates
  and CLI.

<!-- sorted case-sensitive -->

[^semantic-tokens]: https://zed.dev/docs/extensions/languages#syntax-highlighting-with-semantic-tokens

[Mosaic]: https://github.com/kjanat/mosaic "Typesetting language for technical documents"
[`brackets.scm`]: languages/mosaic/brackets.scm
[`crates/tree-sitter-mosaic/queries/`]: ../tree-sitter-mosaic/queries/
[`crates/tree-sitter-mosaic`]: ../tree-sitter-mosaic/
[`crates/zed-mosaic/`]: ../zed-mosaic/
[`extension.toml`]: extension.toml
[`indents.scm`]: languages/mosaic/indents.scm
[`languages/mosaic/config.toml`]: languages/mosaic/config.toml
[`languages/mosaic/`]: languages/mosaic/
[`languages/mosaic/semantic_token_rules.json`]: languages/mosaic/semantic_token_rules.json
[`languages/mosaic/tasks.json`]: languages/mosaic/tasks.json
[`outline.scm`]: languages/mosaic/outline.scm
[`overrides.scm`]: languages/mosaic/overrides.scm
[`runnables.scm`]: languages/mosaic/runnables.scm
[`src/lib.rs`]: src/lib.rs
[`textobjects.scm`]: languages/mosaic/textobjects.scm
