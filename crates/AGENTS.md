# CRATES KNOWLEDGE BASE

## OVERVIEW

`crates/` holds 16 Cargo workspace crates plus `zed-mosaic`, an intentionally excluded Zed
extension. Keep dependency direction boring. Boring good. Cycles bad rock.

## CRATE GRAPH

```text
adobe-font-metrics
└── pdf-base14-metrics
    └── mos-fonts
        └── mos-layout
            ├── mos-pdf
            └── mos-html

mos-core
├── mos-parse
│   └── mos-eval
├── mos-cache
├── mos-packages
├── mos-lsp
└── mos-bib
    └── mos-csl

mos = top orchestration only
tree-sitter-mosaic = editor grammar side world
zed-mosaic = excluded Zed extension side world
```

Side/future domains: `mos-cache`, `mos-packages`, `mos-bib`, and `mos-csl` should stay close to
`mos-core` (and `mos-bib`, for `mos-csl`) until real integration requires more. `mos-html` is a stub
backend. `mos-lsp` may consume current compiler phases, but should stay a thin protocol boundary.

## WHERE TO LOOK

| Task                  | Crate                | Notes                                                          |
| --------------------- | -------------------- | -------------------------------------------------------------- |
| Document model/errors | `mos-core`           | Lowest Mosaic layer. No parse/layout/backend deps.             |
| Source syntax         | `mos-parse`          | CST + spans only. No semantic lowering.                        |
| Lower/resolve         | `mos-eval`           | Parse tree to `Document`; refs/images/figures.                 |
| Text/font metrics     | `mos-fonts`          | Base-14 + bundled Noto Sans.                                   |
| Page layout           | `mos-layout`         | Consumes `Document`; emits `PageGraph`.                        |
| PDF output            | `mos-pdf`            | Consumes `PageGraph`; emits files/bytes.                       |
| HTML output           | `mos-html`           | Stub backend; see crate guide before touching.                 |
| CLI                   | `mos`                | Pipeline glue and user diagnostics.                            |
| AFM parser            | `adobe-font-metrics` | Zero-dep parser below metrics crate.                           |
| Core-14 metrics       | `pdf-base14-metrics` | Vendored data + build-generated table.                         |
| Manifest schema       | `mos-packages`       | Parses `mosaic.toml`; no registry yet.                         |
| Tree-sitter grammar   | `tree-sitter-mosaic` | Editor syntax and queries; separate from compiler.             |
| Zed extension         | `zed-mosaic`         | Excluded from workspace; query copies and tasks.               |
| Cache                 | `mos-cache`          | Trait/in-memory stub; no persistence yet; see crate guide.     |
| LSP                   | `mos-lsp`            | Stdio diagnostic publisher; see crate guide for protocol edge. |
| Bibliography          | `mos-bib`            | Minimal BibTeX record parser; no resolution/rendering yet.     |
| CSL styling           | `mos-csl`            | CSL item model + BibTeX mapping + style parser; no processor.  |

## BOUNDARY RULES

- `mos-core`: IDs, nodes, spans, diagnostics. No higher-layer dependencies.
- `mos-parse`: bytes to syntax. Preserve spans. Report recoverable parse errors.
- `mos-eval`: syntax to semantic graph. No layout/page/PDF decisions.
- `mos-layout`: semantic graph to page graph. No source parsing. No file emission.
- `mos-pdf`/`mos-html`: backend sinks. No lowering or layout policy.
- `mos`: orchestrate, print diagnostics, map errors to exit codes. No compiler logic.
- `adobe-font-metrics`/`pdf-base14-metrics`: isolated vendor/metrics layer.
- `tree-sitter-mosaic`: editor CST only. No compiler lowering/layout/backend truth.
- `zed-mosaic`: Zed packaging and copied queries only. Not a workspace crate.

## CONVENTIONS

- Use workspace deps/lints from root `Cargo.toml`.
- Prefer adding tests in the crate that owns behavior.
- Keep future crate dependencies out until source actually needs them.
- Stubs should fail clearly with `CoreError::Unimplemented`, not silent success.
- Public APIs should model current shipped behavior, not the full manifesto.

## ANTI-PATTERNS

- Do not make `mos-core` depend on parser/evaluator/layout/backends.
- Do not route parser behavior by directive name when `DirectiveKind` exists.
- Do not add backend-specific attributes to core unless every backend can tolerate them.
- Do not make CLI read `mosaic.toml` semantics unless explicitly implementing that slice.
- Do not edit generated Tree-sitter parser artifacts by hand.
- Do not update Zed query copies without syncing from canonical Tree-sitter queries.
