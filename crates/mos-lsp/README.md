# mos-lsp

Language-server crate for Mosaic `.mos` files.

The current slice publishes the same parse / lower / resolve diagnostics that `mos check` renders,
but over the Language Server Protocol so editors can show them inline. Everything else listed under
non-goals stays unbuilt.

## Current Behavior

- Library API: `mos_lsp::run() -> mos_lsp::Result<()>`.
- `run()` drives a stdio LSP server over JSON-RPC 2.0 framed with `Content-Length` headers.
- Implemented requests/notifications: `initialize`, `initialized`, `shutdown`, `exit`,
  `textDocument/didOpen`, `textDocument/didChange` (full sync), `textDocument/didClose`,
  `textDocument/definition`.
- After every open/change the server sends `textDocument/publishDiagnostics` with the compiler
  diagnostics for that document; close clears them.
- `textDocument/definition` resolves a cursor on an `@label` / `@page(label)` reference to a single
  `Location` covering the label's first declaration; an undeclared label or a cursor off any
  reference returns `null` (not an error). Lookups are single-document, so the result reuses the
  request URI.
- Unknown requests get a JSON-RPC `MethodNotFound` (-32601); unknown notifications are dropped.
- Advertised capabilities are intentionally narrow: full text sync, UTF-16 position encoding, and
  `definitionProvider`. Pull diagnostics are not advertised.
- Binary: `mos-lsp`, defined in `Cargo.toml`, calls `mos_lsp::run()`.

### Manual smoke test

```sh
python3 - <<'PY' | cargo run -p mos-lsp
import json
import sys

messages = [
    {"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {}},
    {
        "jsonrpc": "2.0",
        "method": "textDocument/didOpen",
        "params": {
            "textDocument": {
                "uri": "file:///tmp/main.mos",
                "languageId": "mosaic",
                "version": 1,
                "text": "see @no:such\n",
            }
        },
    },
    {"jsonrpc": "2.0", "method": "exit"},
]

for message in messages:
    body = json.dumps(message, separators=(",", ":")).encode()
    sys.stdout.buffer.write(f"Content-Length: {len(body)}\r\n\r\n".encode())
    sys.stdout.buffer.write(body)
PY
```

The server replies with an `initialize` response and one `publishDiagnostics` notification carrying
a `MOS0033` diagnostic for the unknown `@no:such` reference.

Automated coverage for the same path lives in
`server::tests::initialize_did_open_publishes_diagnostics_and_exits`; run it with
`cargo test -p mos-lsp initialize_did_open_publishes_diagnostics_and_exits`.

## Boundary

`mos-lsp` is the thin protocol boundary around compiler services. Diagnostic messages, codes, and
spans come from `mos-core` / `mos-parse` / `mos-eval`; this crate only re-shapes them into LSP
positions and dispatches JSON-RPC. Go-to-definition follows the same rule: it walks the lowered
`mos-eval` `Document` (label declarations and reference spans) and translates spans to LSP ranges,
mirroring the resolver's first-declaration-wins index rather than reimplementing label policy.

To avoid re-lowering the same source repeatedly, the server keeps an in-memory per-document cache of
`mos_eval::lower` output (`src/cache.rs`). Both paths share it: publishing diagnostics on `didOpen`
/ `didChange` lowers the document once into the cache, and a later `textDocument/definition` request
on the unchanged source reuses that same lowering — so an edit lowers its source only once for both
diagnostics and go-to-definition. The cache only memoises the existing lowering — it owns no
parse/lower policy — and an entry is dropped whenever the document's source changes (`didOpen`
re-open / `didChange`) or the document closes, so a cached lowering is always derived from the
current source.

Compiler phase ownership stays elsewhere:

- `mos-core`: document IDs, spans, diagnostics, shared errors.
- `mos-parse`: `.mos` source to syntax tree.
- `mos-eval`: syntax to semantic `Document`, including resolver diagnostics.
- `mos-layout` / `mos-pdf` / `mos-html`: layout and backend output.
- `mos`: user CLI orchestration.

## Known Non-Goals Today

- No completion, hover, formatting, code actions, or rename. Go-to-definition for `@label`
  references is the one navigation request implemented.
- No incremental document sync — `didChange` replaces the buffer wholesale.
- No source-to-PDF sync or live preview.
- No persistent or cross-session compilation cache, and no workspace indexing. The only caching is
  an in-memory per-document lowering memo shared by diagnostics and go-to-definition (see Boundary),
  rebuilt each edit.
- No multi-file projects: diagnostics are produced from the opened document in isolation.

The root README and AGENTS files remain the source of truth for what is and isn't shipped overall.
