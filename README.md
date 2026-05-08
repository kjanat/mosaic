# Mosaic

A semantic, incremental, constraint-based typesetting compiler — written in
Rust, targeting PDF/HTML/EPUB. Mosaic compiles `.mos` source files into
documents through a dependency graph rather than a linear stream of typesetting
commands, so editing one sentence reflows only the affected pages.

The full design — language, layout algorithm, package model, MVP roadmap — lives
in [`manifest.md`](./manifest.md). This README is just enough to orient you
and to get the workspace building.

## Status

Pre-alpha (`0.0.0`). The 12-crate workspace skeleton is in place. MVP 0 from
`manifest.md` §30 is in progress:

- ✅ parser for headings (`= …`, `== …`, `=== …`), paragraphs, inline
  `*emphasis*` / `**strong**` / `` `code` ``, and `#set name(...)` blocks;
- ✅ lowering to a typed semantic `Document` graph in `mosaic-core`
  (manifest §5, §6 stage 2);
- ✅ `mos check` end-to-end — parse → lower → render diagnostics with
  `file:line:col` and source carets;
- ⏳ basic page layout and the PDF backend (manifest §6 stages 5–9, §21.1).

## Quick start

Requires Rust 1.95+ (edition 2024, resolver 3).

```sh
cargo build --workspace            # or: cargo bw
cargo run -p mosaic-cli -- --help  # or: cargo mos --help
```

The `mos` CLI exposes the manifest §15.1 subcommands. `mos check` is wired
end-to-end; the rest still print a "not yet implemented" placeholder to stderr
and exit non-zero (`ExitCode::FAILURE`) so scripts and CI surface the stub.

```sh
cargo mos check examples/hello/main.mos
# ok: 10 node(s), 0 warning(s)

cargo mos build examples/hello/main.mos
# mos build: parsed and lowered 10 node(s); layout + PDF emission not yet
# implemented (manifest §30 MVP 0 stages 5–9)
```

A second binary, `mos-lsp`, is the language server entry point editors will
spawn on stdio.

## Cargo aliases

Defined in `.cargo/config.toml`:

| alias                           | expansion                                                |
| ------------------------------- | -------------------------------------------------------- |
| `cargo bw` / `cw` / `tw` / `dw` | workspace flavours of `build` / `check` / `test` / `doc` |
| `cargo br` / `rr`               | release `build` / `run`                                  |
| `cargo lint`                    | `clippy --workspace --all-targets -- -D warnings`        |
| `cargo mos …`                   | `run -q -p mosaic-cli -- …`                              |

## Workspace layout

```text
crates/
  mosaic-core       document model, IDs, diagnostics      (manifest §5, §31)
  mosaic-parse      parser for .mos                       (manifest §3, §6)
  mosaic-eval       expression / template evaluator       (manifest §4, §25)
  mosaic-layout     inline + block + page layout          (manifest §6, §22)
  mosaic-pdf        PDF backend                           (manifest §21.1)
  mosaic-html       semantic HTML backend                 (manifest §21.2)
  mosaic-fonts      font discovery, shaping, metrics      (manifest §22.1)
  mosaic-bib        bibliography / citation engine        (manifest §12)
  mosaic-cache      incremental build cache               (manifest §7, §32)
  mosaic-lsp        language server (lib + mos-lsp bin)   (manifest §17)
  mosaic-packages   project / package manifest schema     (manifest §14)
  mosaic-cli        `mos` command-line interface          (manifest §15.1)
examples/hello/     placeholder source + project manifest
```

`mosaic-core` is the leaf-most crate; nothing else depends on `mosaic-cli`.

## License

[MIT](./LICENSE).
