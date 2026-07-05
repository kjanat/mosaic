<picture height="64" align="left" alt="Mosaic logo">
  <source media="(prefers-color-scheme: dark)" srcset="design/A4.svg">
  <source media="(prefers-color-scheme: light)" srcset="design/A4.svg">
  <img alt="Mosaic" height="64" align="left" src="design/A4.svg">
</picture>

# Mosaic

[![CI](https://github.com/kjanat/mosaic/actions/workflows/ci.yml/badge.svg)](https://github.com/kjanat/mosaic/actions/workflows/ci.yml)

A semantic, ~~incremental, constraint-based~~ typesetting compiler: written in Rust, targeting PDF
~~/HTML/EPUB~~ .\
Mosaic compiles `.mos` source files into documents ~~through a dependency graph rather than a linear
stream of typesetting commands, so editing one sentence reflows only the affected pages~~.

The full design: language, layout algorithm, package model, MVP roadmap: lives in
[`manifest.md`](./manifest.md). This README is just enough to orient you and to get the workspace
building. The actionable implementation checklist lives in
[`manifest-tracker.md`](./manifest-tracker.md).

## Status

Pre-alpha (`0.0.0`). The 16-crate workspace skeleton is in place, plus an excluded Zed extension
under `crates/zed-mosaic`. MVP 0 from `manifest.md` §30 is substantially landed:

- [x] parser for headings (`= …`, `== …`, `=== …`), paragraphs, inline `*emphasis*` / `**strong**` /
      `` `code` `` / `[@key]` citations, `-` / `N.` lists with hanging indents and indented
      continuation lines, and `#set name(...)` blocks;
- [x] author-facing line-break controls (issue #26 pieces 1/2/3): a literal U+00A0 NBSP that the
      greedy breaker never splits, a `\\` hard line break for forced mid-paragraph breaks, and a
      `\-` (or literal U+00AD) soft hyphen that stays invisible when the word fits and otherwise
      breaks the line at the latest fitting marker with a visible hyphen (optimal non-greedy
      selection still belongs to the eventual Knuth-Plass pass); see `examples/linebreaks/`;
- [x] lowering to a typed semantic `Document` graph in `mos-core`, with `#image(...)`,
      `#figure(...)`, and `#bibliography(...)` source declarations evaluated in `mos-eval` (manifest
      §5, §6 stage 2);
- [x] `mos check` end-to-end: parse → lower → render diagnostics with `file:line:col` and source
      carets;
- [x] `mos build` end-to-end: layout + PDF emission for the Base-14 core fonts and bundled Noto
      Sans, with PNG/JPEG raster images and figure captions (manifest §6 stages 5–9, §21.1);
- [x] `mos-lsp` publishes current compiler diagnostics over stdio LSP on open/change, answers
      `textDocument/definition` for `@label` references, renames labels, and exposes compiler
      suggestions as code actions;
- [ ] HTML and EPUB backends, persistent incremental cache, full bibliography rendering (sorted
      entry lists, CSL styles) and compiler integration, and richer LSP features; see MVP 1–6 in
      `manifest.md`. Citation keys resolve and resolved `[@key]` markers render numeric labels
      (`[1]`, ...), but BibTeX/CSL foundations are not yet a shipped end-to-end bibliography
      pipeline.

Label and reference behavior is documented in
[`docs/labels-and-references.md`](./docs/labels-and-references.md).

### Rust API Stability

Mosaic crates are published as pre-alpha `0.0.x` packages. The CLI and `.mos` language are the main
product surface today; public Rust APIs may still break in patch releases when the internal model
needs cleanup. If you depend on a Mosaic crate directly, pin an exact patch version such as
`mos-parse = "=0.0.1"` until a stronger stability policy lands.

## Quick start

Toolchain pinned via `rust-toolchain.toml` (stable, edition 2024, resolver 3). Rust 1.96+.

```sh
cargo build --workspace     # or: cargo bw
cargo run -p mos -- --help  # or: cargo mos --help
```

The `mos` CLI exposes the manifest §15.1 subcommands. `check` and `build` are wired end-to-end; the
rest (`init`, `watch`, `fmt`, `test`, `profile`, `clean`, `package`) print a "not yet implemented"
placeholder to stderr and exit non-zero (`ExitCode::FAILURE`) so scripts and CI surface the stub.

```sh
cargo mos check examples/hello/main.mos
# ok: 52 node(s), 0 warning(s)

cargo mos build examples/hello/main.mos
# wrote build/main.pdf in 583 ms
```

Executable `.mos` files may start with a byte-zero shebang for shell use:

```mos
#!/usr/bin/env -S mos build --open
= Title
```

`mos check` and `mos build` ignore that first line as script metadata. Later `#!` text remains
ordinary document content; the CLI still requires an explicit subcommand and does not treat bare
`mos main.mos` as build/open shorthand.

A second binary, `mos-lsp`, is the language server entry point editors will spawn on stdio.

## Examples

Each directory under `examples/` is a self-contained Mosaic project (`main.mos` + `mosaic.toml`) and
ships a committed `<name>.pdf` snapshot so GitHub previews render inline:

| project               | exercises                                                       |
| --------------------- | --------------------------------------------------------------- |
| `examples/code`       | inline code, `#pre`, `#code`, long-bracket raw blocks           |
| `examples/hello`      | bundled Noto Sans, multilingual coverage, real italic/bold cuts |
| `examples/lists`      | bullet / numbered lists, hanging indent, continuation, gutter   |
| `examples/math`       | Base-14 Helvetica via `/Differences`, math operators            |
| `examples/polish`     | Polish diacritics through Noto Sans                             |
| `examples/linebreaks` | NBSP (U+00A0), hard line break (`\\`), soft hyphen (`\-`)       |
| `examples/lsp`        | `@label` refs, `<label>` declarations, citations, figure labels |

Regenerate every snapshot with `just examples`. The recipe bootstraps `runner-run` if needed, then
runs `runner mos build examples/*`; each example manifest writes its committed `<name>.pdf` output.

## Tooling Recipes

Root JavaScript tooling is managed by Bun. `package.json` provides local `dprint`, `tombi`, and
`runner-run`; the `justfile` uses `runner-run` for project recipes.

```sh
just setup       # install runner commands
just fmt         # runner fmt -> dprint fmt
just examples    # runner mos build examples/*
just doc-nightly # rustup run nightly -- runner dwn
```

`dprint` also formats Rust through `rustfmt`, TOML through `tombi`, and the `justfile` through
`just --dump --justfile`.

## Cargo aliases

Defined in [`.cargo/config.toml`](./.cargo/config.toml). Cargo's alias schema forbids redefining the
single-letter built-ins (`b` / `c` / `d` / `t` / `r` / `rm`), so the workspace flavours get
two-letter names instead:

| alias          | expansion                                                                       | purpose                                         |
| -------------- | ------------------------------------------------------------------------------- | ----------------------------------------------- |
| `cargo bw`     | `build --workspace`                                                             | build every crate                               |
| `cargo bwa`    | `build --workspace --all-targets`                                               | build all targets                               |
| `cargo cw`     | `check --workspace --all-targets`                                               | type-check including tests / examples / benches |
| `cargo tw`     | `test --workspace`                                                              | run every crate's test suite                    |
| `cargo dw`     | `doc --workspace --no-deps`                                                     | rustdoc for our crates only                     |
| `cargo dwn`    | `--config .cargo/nightly.toml doc`                                              | nightly rustdoc checks                          |
| `cargo br`     | `build --release`                                                               | release build of the current package            |
| `cargo rr`     | `run --release`                                                                 | release run of the current package              |
| `cargo cov`    | `llvm-cov --summary-only --no-clean`                                            | coverage summary                                |
| `cargo lint`   | `clippy --workspace --all-targets --all-features -- -D warnings -D clippy::all` | strict clippy; warnings fail the run            |
| `cargo mos`    | `run --release -q -p mos --`                                                    | invoke the `mos` CLI                            |
| `cargo mosls`  | `run --release -q -p mos-lsp --`                                                | invoke the `mos-lsp` server                     |
| `cargo mosi`   | `install --path=crates/mos --bin=mos --force`                                   | install the `mos` CLI locally                   |
| `cargo mosils` | `install --path=crates/mos-lsp --bin=mos-lsp --force`                           | install `mos-lsp` locally                       |

`cargo lint` is **not** aliased as `cargo clippy` because that name is already the clippy
subcommand.

## Workspace layout

```text
crates/
  mos                  command-line interface                 (manifest §15.1)
  mos-core             document model, IDs, diagnostics       (manifest §5, §31)
  mos-parse            parser for .mos                        (manifest §3, §6)
  mos-eval             lowering + reference/figure resolver   (manifest §4, §25)
  mos-layout           inline, block, and page layout         (manifest §6, §22)
  mos-pdf              PDF backend                            (manifest §21.1)
  mos-html             stub semantic HTML backend boundary    (manifest §21.2)
  mos-fonts            font identity, shaping, metrics        (manifest §22.1)
  mos-bib              minimal BibTeX parser                  (manifest §12)
  mos-csl              CSL item/style parser foundations      (manifest §12)
  mos-cache            cache trait + in-memory implementation (manifest §7, §32)
  mos-lsp              language server (lib + mos-lsp bin)    (manifest §17)
  mos-packages         project / package manifest schema      (manifest §14)
  adobe-font-metrics   zero-dep AFM v4 parser                 (Adobe TN 5004)
  pdf-base14-metrics   baked Core-14 PDF font metrics         (uses adobe-font-metrics)
  tree-sitter-mosaic   Tree-sitter grammar for Mosaic
  zed-mosaic           Zed language extension for Mosaic
examples/
  hello, code, linebreaks, lists, math, polish (committed PDF snapshots)
```

`adobe-font-metrics` is the leaf-most crate (zero deps); nothing else depends on `mos`. `zed-mosaic`
lives under `crates/` but is excluded from the Cargo workspace.

## License

[MIT](./LICENSE) © Kaj Kowalski 2026.

<!-- rumdl-disable-file MD033 MD041 -->
