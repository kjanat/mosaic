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
- Language server features through [`mos-lsp`]: diagnostics, go-to-definition, and label rename.
- Reserved semantic token style rules for a future Mosaic language server.

The Rust/WASM entrypoint in [`src/lib.rs`] registers the extension and spawns [`mos-lsp`] as the
Mosaic language server (see [Language server](#language-server)). No compiler logic lives here.

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

## Language server

Opening a `.mos` file starts [`mos-lsp`] for the `Mosaic` language. [`extension.toml`] declares the
`mos-lsp` language server and [`src/lib.rs`] resolves the binary. Current features: compiler
diagnostics on open/change, go-to-definition for `@label` / `@page(label)` references, and label
rename via `textDocument/rename`.

### Binary discovery

The extension locates `mos-lsp` in this order:

1. `lsp."mos-lsp".binary.path` in your Zed settings (explicit override).
2. `mos-lsp` on `PATH`.
3. A release binary already downloaded by a previous run (cached on disk).
4. The matching asset from the latest [`kjanat/mosaic`] GitHub release, downloaded automatically.

Steps 3–4 let installed-extension users (no local checkout) get the server without building it: the
extension picks the `mos-lsp-<target-triple>.{tar.gz,zip}` asset for `current_platform()`, extracts
it, and caches the path. The `download_file` capability in [`extension.toml`] grants that download.
If a release asset for the platform is missing, Zed surfaces an error. For local development,
install the server from the workspace so it lands on `PATH` (step 2, ahead of any download):

```bash
cargo mosils   # cargo install --path=crates/mos-lsp --bin=mos-lsp --force
```

The release assets are produced by [`.github/workflows/release.yml`], which calls the reusable
[`release-binaries.yml`] pipeline to cross-compile `mos-lsp` for every Zed-supported target on each
`v*` tag.

To pin a specific binary instead, add to your Zed settings:

```json
{
	"lsp": {
		"mos-lsp": {
			"binary": { "path": "/absolute/path/to/mos-lsp" }
		}
	}
}
```

### Verify the language server

After installing the dev extension with `mos-lsp` available:

1. Open a `.mos` file with an undeclared `[@key]` citation or a broken `@ref` → diagnostics appear.
2. Place the cursor on a `@label` / `@page(label)` reference and go-to-definition → jumps to the
   label declaration.
3. Rename a label → the declaration and all `@label` / `@page(label)` references update together.

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

- Build/check the extension crate from this directory with `cargo check`, or from the repo root with
  `cargo check --manifest-path crates/zed-mosaic/Cargo.toml`. The root Cargo workspace excludes this
  crate.
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
future LSP and maps those semantic tokens to Zed theme styles. It is inactive because [`mos-lsp`]
does not yet advertise a semantic tokens provider (it ships diagnostics, definition, and rename);
enabling Zed semantic tokens (`combined` or `full`) has no effect until the server emits
them[^semantic-tokens].

## Known non-goals

- No formatter, code actions, completion, package resolution, preview pane, or watch mode. LSP
  features are limited to what [`mos-lsp`] advertises today (diagnostics, definition, rename).
- No compiler behavior lives here. `mos check`/`mos build` and [`mos-lsp`] remain owned by the main
  Mosaic crates.

<!-- sorted case-sensitive -->

[^semantic-tokens]: https://zed.dev/docs/extensions/languages#syntax-highlighting-with-semantic-tokens

[Mosaic]: https://github.com/kjanat/mosaic "Typesetting language for technical documents"
[`brackets.scm`]: languages/mosaic/brackets.scm
[`crates/tree-sitter-mosaic/queries/`]: ../tree-sitter-mosaic/queries/
[`crates/tree-sitter-mosaic`]: ../tree-sitter-mosaic/
[`.github/workflows/release.yml`]: ../../.github/workflows/release.yml
[`crates/zed-mosaic/`]: ../zed-mosaic/
[`extension.toml`]: extension.toml
[`kjanat/mosaic`]: https://github.com/kjanat/mosaic
[`release-binaries.yml`]: ../../.github/workflows/release-binaries.yml
[`indents.scm`]: languages/mosaic/indents.scm
[`languages/mosaic/config.toml`]: languages/mosaic/config.toml
[`languages/mosaic/`]: languages/mosaic/
[`languages/mosaic/semantic_token_rules.json`]: languages/mosaic/semantic_token_rules.json
[`languages/mosaic/tasks.json`]: languages/mosaic/tasks.json
[`mos-lsp`]: ../mos-lsp/
[`outline.scm`]: languages/mosaic/outline.scm
[`overrides.scm`]: languages/mosaic/overrides.scm
[`runnables.scm`]: languages/mosaic/runnables.scm
[`src/lib.rs`]: src/lib.rs
[`textobjects.scm`]: languages/mosaic/textobjects.scm
