# mos-lsp

Language-server crate for Mosaic `.mos` files.

This crate is present as the editor/LSP entry point, but current behavior is intentionally a stub.
Do not treat it as a working language server yet.

## Current Behavior

- Library API: `mos_lsp::run() -> mos_core::Result<()>`.
- `run()` always returns `CoreError::Unimplemented("mos-lsp::run")`.
- Binary: `mos-lsp`, defined in `Cargo.toml`, calls `mos_lsp::run()`.
- On error, the binary prints `mos-lsp: {err}` to stderr and exits with failure.

## Boundary

`mos-lsp` should stay a thin language-server boundary around compiler services. It currently depends
only on `mos-core` and `mos-parse`; keep it close to source parsing and diagnostics until real
editor features require more.

Compiler phase ownership stays elsewhere:

- `mos-core`: document IDs, spans, diagnostics, shared errors.
- `mos-parse`: `.mos` source to syntax tree.
- `mos-eval`: syntax to semantic `Document`.
- `mos-layout` / `mos-pdf` / `mos-html`: layout and backend output.
- `mos`: user CLI orchestration.

## Known Non-Goals Today

- No LSP protocol loop.
- No editor diagnostics beyond the unimplemented error.
- No completion, hover, go-to-definition, formatting, code actions, or rename.
- No source-to-PDF sync or live preview.
- No incremental cache or workspace indexing.

The root README and AGENTS files are the source of truth: `mos check` and `mos build` are real; LSP
behavior is not shipped yet. Booga mark cave wall so future hunter no chase ghost mammoth.
