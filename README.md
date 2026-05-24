<picture height="64" align="left" alt="Mosaic logo">
  <source media="(prefers-color-scheme: dark)" srcset="design/A4.svg">
  <source media="(prefers-color-scheme: light)" srcset="design/A4.svg">
  <img alt="Mosaic" height="64" align="left" src="design/A4.svg">
</picture>

# Mosaic

[![CI](https://github.com/kjanat/mosaic/actions/workflows/ci.yml/badge.svg)](https://github.com/kjanat/mosaic/actions/workflows/ci.yml)

A semantic, ~~incremental, constraint-based~~ typesetting compiler — written in Rust, targeting PDF
~~/HTML/EPUB~~ .\
Mosaic compiles `.mos` source files into documents ~~through a dependency graph rather than a linear
stream of typesetting commands, so editing one sentence reflows only the affected pages~~.

The full design — language, layout algorithm, package model, MVP roadmap — lives in
[`manifest.md`](./manifest.md). This README is just enough to orient you and to get the workspace
building. The actionable implementation checklist lives in
[`manifest-tracker.md`](./manifest-tracker.md).

## Status

Pre-alpha (`0.0.0`). The 14-crate workspace skeleton is in place. MVP 0 from `manifest.md` §30 is
substantially landed:

- [x] parser for headings (`= …`, `== …`, `=== …`), paragraphs, inline `*emphasis*` / `**strong**` /
      `` `code` ``, `-` / `N.` lists with hanging indents, and `#set name(...)` blocks;
- [x] author-facing line-break controls (issue #26 piece 1/2 + piece 3a): a literal U+00A0 NBSP that
      the greedy breaker never splits, a `\\` hard line break for forced mid-paragraph breaks, and a
      `\-` (or literal U+00AD) soft hyphen that is stripped from rendering today and will become a
      permitted break point when Knuth-Plass lands — see `examples/linebreaks/`;
- [x] lowering to a typed semantic `Document` graph in `mos-core`, with `#image(...)` and
      `#figure(...)` directives evaluated in `mos-eval` (manifest §5, §6 stage 2);
- [x] `mos check` end-to-end — parse → lower → render diagnostics with `file:line:col` and source
      carets;
- [x] `mos build` end-to-end — layout + PDF emission for the Base-14 core fonts and bundled Noto
      Sans, with PNG/JPEG raster images and figure captions (manifest §6 stages 5–9, §21.1);
- [ ] HTML and EPUB backends, incremental cache, LSP, bibliography — see MVP 1–6 in `manifest.md`.

## Quick start

Toolchain pinned via `rust-toolchain.toml` (stable, edition 2024, resolver 3). Rust 1.95+.

```sh
cargo build --workspace            # or: cargo bw
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

A second binary, `mos-lsp`, is the language server entry point editors will spawn on stdio.

## Examples

Each directory under `examples/` is a self-contained Mosaic project (`main.mos` + `mosaic.toml`) and
ships a committed `<name>.pdf` snapshot so GitHub previews render inline:

| project               | exercises                                                       |
| --------------------- | --------------------------------------------------------------- |
| `examples/hello`      | bundled Noto Sans, multilingual coverage, real italic/bold cuts |
| `examples/lists`      | bullet / numbered lists, hanging indent, adaptive gutter        |
| `examples/math`       | Base-14 Helvetica via `/Differences`, math operators            |
| `examples/polish`     | Polish diacritics through Noto Sans                             |
| `examples/linebreaks` | NBSP (U+00A0), hard line break (`\\`), soft hyphen (`\-`)       |

Regenerate every snapshot with `just examples` (rebuilds each project and copies `build/main.pdf`
next to its `main.mos`).

## Cargo aliases

Defined in [`.cargo/config.toml`](./.cargo/config.toml). Cargo's alias schema forbids redefining the
single-letter built-ins (`b` / `c` / `d` / `t` / `r` / `rm`), so the workspace flavours get
two-letter names instead:

| alias        | expansion                                         | purpose                                         |
| ------------ | ------------------------------------------------- | ----------------------------------------------- |
| `cargo bw`   | `build --workspace`                               | build every crate                               |
| `cargo cw`   | `check --workspace --all-targets`                 | type-check including tests / examples / benches |
| `cargo tw`   | `test --workspace`                                | run every crate's test suite                    |
| `cargo dw`   | `doc --workspace --no-deps`                       | rustdoc for our crates only                     |
| `cargo br`   | `build --release`                                 | release build of the current package            |
| `cargo rr`   | `run --release`                                   | release run of the current package              |
| `cargo lint` | `clippy --workspace --all-targets -- -D warnings` | strict clippy; warnings fail the run            |
| `cargo mos`  | `run -q -p mos --`                                | invoke the `mos` CLI                            |

`cargo lint` is **not** aliased as `cargo clippy` because that name is already the clippy
subcommand.

## Workspace layout

```text
crates/
  mos                  command-line interface                 (manifest §15.1)
  mos-core             document model, IDs, diagnostics       (manifest §5, §31)
  mos-parse            parser for .mos                        (manifest §3, §6)
  mos-eval             expression / template evaluator        (manifest §4, §25)
  mos-layout           inline, block, and page layout         (manifest §6, §22)
  mos-pdf              PDF backend                            (manifest §21.1)
  mos-html             semantic HTML backend                  (manifest §21.2)
  mos-fonts            font discovery, shaping, metrics       (manifest §22.1)
  mos-bib              bibliography / citation engine         (manifest §12)
  mos-cache            incremental build cache                (manifest §7, §32)
  mos-lsp              language server (lib + mos-lsp bin)    (manifest §17)
  mos-packages         project / package manifest schema      (manifest §14)
  adobe-font-metrics   zero-dep AFM v4 parser                 (Adobe TN 5004)
  pdf-base14-metrics   baked Core-14 PDF font metrics         (uses adobe-font-metrics)
  tree-sitter-mosaic   Tree-sitter grammar for Mosaic
  zed-mosaic           Zed language extension for Mosaic
examples/
  hello, code, lists, math, polish (committed PDF snapshots)
```

`adobe-font-metrics` is the leaf-most crate (zero deps); nothing else depends on `mos`.

## License

[MIT](./LICENSE) © Kaj Kowalski 2026.
