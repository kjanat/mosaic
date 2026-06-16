# MOS-LSP KNOWLEDGE BASE

## OVERVIEW

`mos-lsp` is a thin stdio LSP server: it publishes compiler diagnostics and answers
`textDocument/definition` for `@label` references. Tiny protocol cave around real compiler services

- it re-shapes their output into LSP, never owns parse/lower/resolve policy.

## WHERE TO LOOK

| Task              | Location             | Notes                                                   |
| ----------------- | -------------------- | ------------------------------------------------------- |
| Server loop       | `src/server.rs`      | JSON-RPC framing, state, request dispatch.              |
| LSP diagnostics   | `src/diagnostics.rs` | Compiler diagnostic to LSP range conversion.            |
| Go-to-definition  | `src/definition.rs`  | `@label` reference → declaration span; position↔byte.   |
| Label rename      | `src/rename.rs`      | Label occurrences (decl token + refs) → WorkspaceEdit.  |
| Lowering cache    | `src/cache.rs`       | Per-URI memo of `mos_eval::lower`; invalidated on edit. |
| Binary entry      | `src/main.rs`        | Calls `mos_lsp::run()`.                                 |
| Behavior contract | `README.md`          | Current supported messages and non-goals.               |

## CURRENT SLICE

- Handles `initialize`, `initialized`, `shutdown`, `exit`.
- Handles full-sync `textDocument/didOpen`, `didChange`, `didClose`.
- Publishes parse/lower/resolve diagnostics after open/change; close clears diagnostics.
- Answers `textDocument/definition`: cursor on `@label` / `@page(label)` → label's first declaration
  span as a `Location`; undeclared label or cursor off a reference → `null`.
- Answers `textDocument/rename`: cursor on a label (declaration token or reference) →
  `WorkspaceEdit` rewriting the first declaration token + every reference identifier to `newName`;
  cursor off a label → `null`. Single-document, first-declaration-wins, no new-name validation.
- Unknown requests return JSON-RPC `MethodNotFound`; unknown notifications drop.
- Caches each open document's `mos-eval` lowering (`src/cache.rs`), shared by diagnostics and
  `textDocument/definition`: an edit lowers once (publish populates the cache, definition reuses
  it). Invalidated on open/change/close. Only **pure** lowerings are cached: a `LowerResult` with
  `reads_external_resources` (`#image` / `#figure` / `#bibliography` read files) is never stored, so
  those docs re-lower per request and reflect the live filesystem.
- Advertises UTF-16 position encoding, full text sync, `definitionProvider`, and `renameProvider`.

## BOUNDARY RULES

- Compiler crates own diagnostic codes, messages, spans, and phase behavior.
- This crate only maps diagnostics to LSP wire shape and editor positions.
- Keep `tower-lsp` or heavier protocol framework out unless current slice outgrows direct stdio.
- Tests may drive `serve` with in-memory reader/writer. No server process needed.

## ANTI-PATTERNS

- Do not advertise `diagnosticProvider` until pull diagnostics are implemented.
- Do not add completion, hover, formatting, code actions, workspace index, or preview sync by
  manifesto gravity. (Go-to-definition and label rename for `@label` references are shipped, both
  single-document; cross-file rename, `prepareRename`, and new-name validation are still out.)
- Go-to-definition and rename stay single-document and walk the lowered `Document`, mirror the
  resolver's first-declaration-wins rule, do not build a workspace index or cross-file label map.
- Do not treat byte offsets as LSP columns; positions are UTF-16.
- Do not make LSP own parse/lower/resolve policy.
