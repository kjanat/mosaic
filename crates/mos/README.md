# mos CLI

`mos` is the command-line entry point for Mosaic. It glues the compiler crates together, reads entry
files, renders diagnostics, maps failures to process exit codes, and writes build output. Compiler
logic belongs in the phase crates, not here.

Current shipped path:

```text
.mos source -> mos-eval lower/resolve -> mos-layout page graph -> mos-pdf PDF file
```

## Commands

Implemented now:

- `mos check [entry]`: read a `.mos` file, lower/resolve it, print diagnostics, and exit non-zero on
  errors. Default entry: `main.mos`.
- `mos build [entry]`: read, lower/resolve, lay out, and emit `build/<entry-stem>.pdf`. Default
  entry: `main.mos`.

`mos build` also accepts:

- `--open`: open the generated PDF with the platform default viewer.
- `--open=PROGRAM`: open the generated PDF with a specific viewer.

Parsed but not wired to behavior yet:

- `--frozen`
- `--reproducible`

Stubbed subcommands fail clearly with a non-zero exit code:

- `init`
- `watch`
- `fmt`
- `test`
- `profile`
- `clean`
- `package`

## Examples

From the workspace root:

```sh
cargo mos check examples/hello/main.mos
cargo mos build examples/hello/main.mos
```

Equivalent direct Cargo invocation:

```sh
cargo run -q -p mos -- check examples/hello/main.mos
cargo run -q -p mos -- build examples/hello/main.mos
```

Successful `check` prints an `ok:` summary. Successful `build` creates `build/main.pdf` relative to
the current working directory.

## Crate Boundaries

`mos` should stay boring glue:

- Use `mos-eval` for lowering, metadata, labels, references, images, and figures.
- Use `mos-layout` for page/text layout.
- Use `mos-pdf` for PDF emission.
- Use `mos-core` diagnostics and errors for user-facing failures.
- Do not put parser, resolver, layout, PDF, package-registry, cache, or LSP behavior in this crate.

## Known Non-Goals

Do not claim these are implemented in `mos` today:

- HTML/EPUB/SVG output.
- Watch mode.
- Formatting.
- Persistent cache or package registry resolution.
- Reproducible/frozen build semantics.
- Bibliography, math, tables, footnotes, indexes, or glossary support.
- LSP behavior; `mos-lsp` is a separate entry point.
