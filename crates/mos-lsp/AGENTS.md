# MOS-LSP KNOWLEDGE BASE

## OVERVIEW

`mos-lsp` is a thin stdio LSP server. It reshapes compiler services into LSP responses; it never
owns parse/lower/resolve policy.

## WHERE TO LOOK

| Task              | Location                 | Notes                                                                   |
| ----------------- | ------------------------ | ----------------------------------------------------------------------- |
| Server loop       | `src/server.rs`          | JSON-RPC framing, state, request dispatch.                              |
| LSP diagnostics   | `src/diagnostics.rs`     | Compiler diagnostic to LSP range conversion.                            |
| Go-to-definition  | `src/definition.rs`      | `@label` reference → declaration span; position↔byte.                   |
| Label rename      | `src/rename.rs`          | Label occurrences (decl token + refs) → WorkspaceEdit.                  |
| Code actions      | `src/code_action.rs`     | Compiler suggestions → LSP quick fixes.                                 |
| Hover             | `src/hover.rs`           | `/** … */` doc comment of the symbol under the cursor.                  |
| Completion        | `src/completion.rs`      | `[@key` prefix → items from the loaded BibTeX records.                  |
| Inlay hints       | `src/inlay_hint.rs`      | Unnamed code blocks → stable display names at their closing delimiters. |
| Document symbols  | `src/document_symbol.rs` | Heading tree → nested LSP symbols.                                      |
| Lowering cache    | `src/cache.rs`           | Per-URI memo of `mos_eval::lower`; invalidated on edit.                 |
| Binary entry      | `src/main.rs`            | Calls `mos_lsp::run()`.                                                 |
| Protocol E2E      | `tests/protocol_e2e.rs`  | Spawns the real binary, framed JSON-RPC over stdio.                     |
| Behavior contract | `README.md`              | Current supported messages and non-goals.                               |

## CURRENT SLICE

- Handles `initialize`, `initialized`, `shutdown`, `exit`.
- Handles full-sync `textDocument/didOpen`, `didChange`, `didClose`.
- Publishes parse/lower/resolve diagnostics after open/change; close clears diagnostics.
- Answers `textDocument/definition`: cursor on `@label` / `@page(label)` → label's first declaration
  span as a `Location`; cursor on a resolved `[@key]` citation → the matching BibTeX key span;
  undeclared label, unresolved citation, or cursor off a reference/citation → `null`.
- Answers `textDocument/documentSymbol`: nested heading outline by Mosaic heading level, with ranges
  extending to the next same-or-higher-level heading.
- Answers `textDocument/rename`: cursor on a label (declaration token or reference) →
  `WorkspaceEdit` rewriting the first declaration token + every reference identifier to `newName`;
  cursor off a label → `null`. Single-document, first-declaration-wins, no new-name validation.
- Answers `textDocument/codeAction`: compiler suggestions become quick fixes where replacement spans
  map into the current document.
- Answers `textDocument/hover`: the `/** … */` doc comment attached to the block under the cursor
  or to the target of the `@label` reference under it; otherwise `null`.
- Answers `textDocument/completion`: cursor inside a parsed `[@key` token on its line → one item
  per record in a complete `LowerResult::bibliography` whose key matches `[A-Za-z0-9_:.-]+` (edit
  replaces the whole key, appends `]` when missing); otherwise `[]`. Never reads `.bib` files itself.
- Unknown requests return JSON-RPC `MethodNotFound`; unknown notifications drop.
- Answers `textDocument/inlayHint` from the cached lowered document: unnamed `#code` blocks get
  a language/body preview and short content hash after the closing delimiter. Identical names
  get document-order suffixes before range filtering. Manual labels suppress hints; generated
  names never become compiler labels or runnable targets.
- Caches each open document's `mos-eval` lowering (`src/cache.rs`), shared by diagnostics and
  `textDocument/definition`: an edit lowers once (publish populates the cache, definition reuses
  it). Invalidated on open/change/close. A `LowerResult` lists the files it read
  (`external_dependencies`: `#image` / `#figure` / `#bibliography`) with content fingerprints;
  `Store::get_if_current` checks them on each hit (`stat`, re-hash only when size/mtime moved) and
  evicts the entry when any changed, appeared, or disappeared, so those docs still reflect the live
  filesystem (#125).
- Advertises UTF-16 position encoding, full text sync, `definitionProvider`,
  `documentSymbolProvider`, `renameProvider`, `codeActionProvider`, `hoverProvider`, and
  `completionProvider` (trigger character `@`), and `inlayHintProvider` (no resolve request).

## BOUNDARY RULES

- Compiler crates own diagnostic codes, messages, spans, and phase behavior.
- This crate only maps diagnostics to LSP wire shape and editor positions.
- Keep `tower-lsp` or heavier protocol framework out unless current slice outgrows direct stdio.
- Unit tests may drive `serve` with in-memory reader/writer; no server process needed. Protocol E2E
  tests (`tests/protocol_e2e.rs`) additionally spawn the real binary via `CARGO_BIN_EXE_mos-lsp`
  with timeout-guarded framed reads.

## ANTI-PATTERNS

- Do not advertise `diagnosticProvider` until pull diagnostics are implemented.
- Go-to-definition and rename for labels stay single-document and walk the lowered `Document`,
  mirror the resolver's first-declaration-wins rule, do not build a workspace index or cross-file
  label map. Citation go-to-definition may jump to the declared BibTeX source already read during
  lowering.
- Do not treat byte offsets as LSP columns; positions are UTF-16.
- Do not make LSP own parse/lower/resolve policy.
