# mos-lsp

Language-server crate for Mosaic `.mos` files.

The current slice publishes the same parse / lower / resolve diagnostics that `mos check` renders,
but over the Language Server Protocol so editors can show them inline and apply compiler-provided
quick fixes.

> [!WARNING]
> While this crate is in the `0.0.x` line, Mosaic treats it as pre-alpha. Breaking changes are
> acceptable between patch releases. If you depend on this crate, pin an exact version such as
> `=0.0.2`, or accept the risk of API breakage.

## Current Behavior

- Library API: `mos_lsp::run() -> mos_lsp::Result<()>`.
- `run()` drives a stdio LSP server over JSON-RPC 2.0 framed with `Content-Length` headers.
- Implemented requests/notifications: `initialize`, `initialized`, `shutdown`, `exit`,
  `textDocument/didOpen`, `textDocument/didChange` (full sync), `textDocument/didClose`,
  `textDocument/definition`, `textDocument/documentSymbol`, `textDocument/rename`,
  `textDocument/codeAction`, `textDocument/hover`, `textDocument/completion`,
  `textDocument/inlayHint`.
- After every open/change the server sends `textDocument/publishDiagnostics` with the compiler
  diagnostics for that document; close clears them.
- `textDocument/definition` resolves a cursor on an `@label` / `@page(label)` reference to a single
  `Location` covering the label's first declaration, and resolves a cursor on a known `[@key]`
  citation to the key in its declared BibTeX source file. An undeclared label, unresolved citation,
  or cursor off any reference/citation returns `null` (not an error). Label lookups are
  single-document; citation lookups may jump to a `.bib` file already read during lowering.
- `textDocument/rename` rewrites the label under the cursor: its **first** declaration's token and
  every `@label` / `@page(label)` reference: to the request's `newName`, returning a `WorkspaceEdit`
  whose single `changes` entry is keyed by the request URI. Each edit covers only the identifier
  (never the `@` sigil, the `<>` brackets, or the `@page(`…`)` delimiters). A cursor off any label
  returns `null`. Single-document, first-declaration-wins (a duplicate later declaration is left
  untouched); the new name is not validated.
- `textDocument/codeAction` returns `quickfix` actions for compiler diagnostics that carry concrete
  `mos-core::Suggestion` edits. The server projects existing compiler suggestions only; it does not
  synthesize editor-side fixes.
- `textDocument/documentSymbol` returns nested heading symbols from the lowered document. Symbol
  ranges extend until the next same-or-higher-level heading, so editor outlines and breadcrumbs
  follow Mosaic section structure rather than flat syntax nodes.
- `textDocument/completion` offers one item per BibTeX record loaded from the document's declared
  `#bibliography` sources when the cursor sits in a `[@key` token on its line (`@` is the trigger
  character). Keys must match `[A-Za-z0-9_:.-]+`; the editor filters by the typed prefix. An item's
  edit replaces the whole key under the cursor and appends the closing `]` when none follows;
  `detail` is the entry type and `documentation` the entry's `title` field. A cursor off a citation
  (including inside code, raw blocks, directive values, or comments), or a document with any
  missing, unreadable, or malformed bibliography source, gets an empty list; the source diagnostics
  stay as they are.
- Unknown requests get a JSON-RPC `MethodNotFound` (-32601); unknown notifications are dropped.
- `textDocument/inlayHint` shows a generated name after the closing delimiter of each unnamed
  `#code` block in the requested range. It uses the cached lowered document and updates after edits.
  Manually labelled blocks, `#pre`, inline code, comments, and unfinished blocks get no hint. The
  names are display-only: they add no labels, text edits, or runnable commands.
- Advertised capabilities are intentionally narrow: full text sync, UTF-16 position encoding,
  `definitionProvider`, `documentSymbolProvider`, `renameProvider`, `codeActionProvider`,
  `hoverProvider`, `completionProvider` (trigger character `@`), and `inlayHintProvider`
  (`resolveProvider: false`). Pull diagnostics are not advertised.
- Binary: `mos-lsp`, defined in `Cargo.toml`, calls `mos_lsp::run()`.

### Code-block names

Enable inlay hints in your editor to see names beside unlabelled closing delimiters:

```mos
#code(lang: "rust")[[
fn main() {}
]]
```

The name starts with `rust: fn main() {}` and ends with an eight-digit hexadecimal content hash. The
language is capped at 16 characters and the first nonblank body line at 40, with an ellipsis when
truncated. Missing or empty languages use `code`; empty bodies use `empty block`. Control characters
are omitted from previews. String and bare-identifier `lang` arguments are preserved by lowering;
the final `lang` argument wins, and a nontext value has no language metadata.

Names survive moving a block, changing unrelated text, relocating the document, and switching
between LF and CRLF. Editing the body or language changes the name. Identical displayed names get
`#2`, `#3`, and so on in document order, computed before viewport filtering. Their suffixes can
change when an identical block is inserted or removed. Hashes are engine-version stamped, so these
names are not persistent identifiers across compiler versions.

Hints appear immediately after the actual closing delimiter, including long forms such as `]==]`. A
viewport containing just that position receives the hint even when the opening delimiter is
off-screen. Positions use UTF-16 and include the requested range's boundary points. Adding a manual
`<ex:main>` after the delimiter suppresses the generated hint and gives the block its ordinary
referenceable label. Code-block extraction and execution remain future CLI work.

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
`server::tests::initialize_did_open_publishes_diagnostics_and_exits`; run it with `cargo test -p
mos-lsp initialize_did_open_publishes_diagnostics_and_exits`.

### Protocol E2E tests

`tests/protocol_e2e.rs` spawns the real `mos-lsp` binary and drives it over stdio with
`Content-Length`-framed JSON-RPC, like an editor would: initialize/initialized handshake (capability
assertions under a Zed-like client profile), the didOpen/didChange/didClose diagnostics lifecycle,
go-to-definition for `@label` references (including UTF-16 column handling for non-BMP text) and for
`[@key]` citations against a real on-disk `.bib` fixture, citation-key completion against the same
kind of fixture, rename, quickfix code actions, document symbols, code-block hints through edits,
labelling, close/reopen, and clean shutdown/exit. All reads are framed and timeout-guarded, so a
hung server fails the suite instead of hanging CI. The harness runs as part of `cargo test -p
mos-lsp`.

## Boundary

`mos-lsp` is the thin protocol boundary around compiler services. Diagnostic messages, codes, and
spans, and suggestions come from `mos-core` / `mos-parse` / `mos-eval`; this crate only re-shapes
them into LSP positions/edits and dispatches JSON-RPC. Go-to-definition follows the same rule: it
walks the lowered `mos-eval` `Document` (label declarations, reference spans, and resolved citation
target spans) and translates spans to LSP ranges, mirroring resolver/bibliography state rather than
reimplementing policy. Citation completion reads the `LowerResult`'s `bibliography`, the records
`mos-eval` already loaded to resolve citations, so the server never opens a `.bib` file itself.
Code-block hints read the lowered raw node's language, text, span, and authored content hash. The
LSP derives display names without reparsing source or changing compiler labels.

To avoid re-lowering the same source repeatedly, the server keeps an in-memory per-document cache of
`mos_eval::lower` output (`src/cache.rs`). Both paths share it: publishing diagnostics on `didOpen`
/ `didChange` lowers the document once into the cache, and a later `textDocument/definition` request
on the unchanged source reuses that same lowering, so an edit lowers its source only once for both
diagnostics and go-to-definition. The cache only memoises the existing lowering: it owns no
parse/lower policy, and an entry is dropped whenever the document's source changes (`didOpen`
re-open / `didChange`) or the document closes, so a cached lowering is always derived from the
current source.

`mos_eval::lower` is not a pure function of the source; `#image` / `#figure` and `#bibliography`
read external files, so its `LowerResult` lists them in `external_dependencies` with the content
fingerprint each had at lowering time (`None` for a file that could not be read). On every hit the
cache `stat`s those files, re-hashes one only when its size, modification time, or inode identity
(device, inode, `ctime` on Unix) moved or when it was written within two seconds of being
fingerprinted, and drops the entry when any of them changed, appeared, or disappeared, so a document
always reflects the current filesystem (a referenced image appearing after open is seen by the next
request, and so is a same-size rewrite that restores the old mtime) while unchanged files cost one
`stat` instead of a full re-lower. Pure lowerings have no dependencies and are reused without
touching the filesystem.

Compiler phase ownership stays elsewhere:

- `mos-core`: document IDs, spans, diagnostics, shared errors.
- `mos-parse`: `.mos` source to syntax tree.
- `mos-eval`: syntax to semantic `Document`, including resolver diagnostics.
- `mos-layout` / `mos-pdf` / `mos-html`: layout and backend output.
- `mos`: user CLI orchestration.

## Known Non-Goals Today

- No formatting, and no completion beyond citation keys (no label, directive, or argument
  completion). Navigation/editing is limited to go-to-definition, label rename, and
  compiler-suggestion code actions. Rename does no cross-file work, no `prepareRename` validation,
  and no new-name checking.
- No incremental document sync: `didChange` replaces the buffer wholesale.
- No source-to-PDF sync or live preview.
- No persistent or cross-session compilation cache, and no workspace indexing. The only caching is
  an in-memory per-document lowering memo shared by diagnostics and go-to-definition (see Boundary),
  rebuilt each edit.
- No multi-file projects: diagnostics are produced from the opened document in isolation.

The root README and AGENTS files remain the source of truth for what is and isn't shipped overall.
