# mos-lsp

Language-server crate for Mosaic `.mos` files.

The current slice publishes the same parse / lower / resolve diagnostics that `mos check` renders,
but over the Language Server Protocol so editors can show them inline. Everything else listed under
non-goals stays unbuilt.

## Current Behavior

- Library API: `mos_lsp::run() -> mos_lsp::Result<()>`.
- `run()` drives a stdio LSP server over JSON-RPC 2.0 framed with `Content-Length` headers.
- Implemented requests/notifications: `initialize`, `initialized`, `shutdown`, `exit`,
  `textDocument/didOpen`, `textDocument/didChange` (full sync), `textDocument/didClose`.
- After every open/change the server sends `textDocument/publishDiagnostics` with the compiler
  diagnostics for that document; close clears them.
- Unknown requests get a JSON-RPC `MethodNotFound` (-32601); unknown notifications are dropped.
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

## Boundary

`mos-lsp` is the thin protocol boundary around compiler services. Diagnostic messages, codes, and
spans come from `mos-core` / `mos-parse` / `mos-eval`; this crate only re-shapes them into LSP
positions and dispatches JSON-RPC.

Compiler phase ownership stays elsewhere:

- `mos-core`: document IDs, spans, diagnostics, shared errors.
- `mos-parse`: `.mos` source to syntax tree.
- `mos-eval`: syntax to semantic `Document`, including resolver diagnostics.
- `mos-layout` / `mos-pdf` / `mos-html`: layout and backend output.
- `mos`: user CLI orchestration.

## Known Non-Goals Today

- No completion, hover, go-to-definition, formatting, code actions, or rename.
- No incremental document sync — `didChange` replaces the buffer wholesale.
- No source-to-PDF sync or live preview.
- No incremental cache or workspace indexing.
- No multi-file projects: diagnostics are produced from the opened document in isolation.

The root README and AGENTS files remain the source of truth for what is and isn't shipped overall.
