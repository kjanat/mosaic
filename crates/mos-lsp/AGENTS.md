# MOS-LSP KNOWLEDGE BASE

## OVERVIEW

`mos-lsp` is a thin stdio LSP server for compiler diagnostics only. Tiny protocol cave around real
compiler services.

## WHERE TO LOOK

| Task              | Location             | Notes                                        |
| ----------------- | -------------------- | -------------------------------------------- |
| Server loop       | `src/server.rs`      | JSON-RPC framing, state, request dispatch.   |
| LSP diagnostics   | `src/diagnostics.rs` | Compiler diagnostic to LSP range conversion. |
| Binary entry      | `src/main.rs`        | Calls `mos_lsp::run()`.                      |
| Behavior contract | `README.md`          | Current supported messages and non-goals.    |

## CURRENT SLICE

- Handles `initialize`, `initialized`, `shutdown`, `exit`.
- Handles full-sync `textDocument/didOpen`, `didChange`, `didClose`.
- Publishes parse/lower/resolve diagnostics after open/change; close clears diagnostics.
- Unknown requests return JSON-RPC `MethodNotFound`; unknown notifications drop.
- Advertises UTF-16 position encoding and full text sync only.

## BOUNDARY RULES

- Compiler crates own diagnostic codes, messages, spans, and phase behavior.
- This crate only maps diagnostics to LSP wire shape and editor positions.
- Keep `tower-lsp` or heavier protocol framework out unless current slice outgrows direct stdio.
- Tests may drive `serve` with in-memory reader/writer. No server process needed.

## ANTI-PATTERNS

- Do not advertise `diagnosticProvider` until pull diagnostics are implemented.
- Do not add completion, hover, definition, formatting, code actions, rename, workspace index, or
  preview sync by manifesto gravity.
- Do not treat byte offsets as LSP columns; positions are UTF-16.
- Do not make LSP own parse/lower/resolve policy.
