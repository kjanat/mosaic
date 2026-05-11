# CRATES KNOWLEDGE BASE

## OVERVIEW

`crates/` is a 14-crate Rust workspace split by compiler phase and backend domain. Keep dependency
direction boring. Boring good. Cycles bad rock.

## CRATE GRAPH

```text
afm
└── pdf-base14-metrics
    └── mosaic-fonts
        └── mosaic-layout
            ├── mosaic-pdf
            └── mosaic-html

mosaic-core
├── mosaic-parse
│   └── mosaic-eval
├── mosaic-cache
├── mosaic-packages
├── mosaic-lsp
└── mosaic-bib

mosaic-cli = top orchestration only
```

Side/future domains: `mosaic-cache`, `mosaic-packages`, `mosaic-bib`, `mosaic-lsp` should stay close
to `mosaic-core` until real integration requires more.

## WHERE TO LOOK

| Task                  | Crate                | Notes                                              |
| --------------------- | -------------------- | -------------------------------------------------- |
| Document model/errors | `mosaic-core`        | Lowest Mosaic layer. No parse/layout/backend deps. |
| Source syntax         | `mosaic-parse`       | CST + spans only. No semantic lowering.            |
| Lower/resolve         | `mosaic-eval`        | Parse tree to `Document`; refs/images/figures.     |
| Text/font metrics     | `mosaic-fonts`       | Base-14 + bundled Noto Sans.                       |
| Page layout           | `mosaic-layout`      | Consumes `Document`; emits `PageGraph`.            |
| PDF output            | `mosaic-pdf`         | Consumes `PageGraph`; emits files/bytes.           |
| CLI                   | `mosaic-cli`         | Pipeline glue and user diagnostics.                |
| AFM parser            | `afm`                | Zero-dep parser below metrics crate.               |
| Core-14 metrics       | `pdf-base14-metrics` | Vendored data + build-generated table.             |
| Manifest schema       | `mosaic-packages`    | Parses `mosaic.toml`; no registry yet.             |
| Cache                 | `mosaic-cache`       | Trait/in-memory stub; no persistence yet.          |
| LSP                   | `mosaic-lsp`         | Binary exists; behavior stub.                      |
| Bibliography          | `mosaic-bib`         | Placeholder only.                                  |

## BOUNDARY RULES

- `mosaic-core`: IDs, nodes, spans, diagnostics. No higher-layer dependencies.
- `mosaic-parse`: bytes to syntax. Preserve spans. Report recoverable parse errors.
- `mosaic-eval`: syntax to semantic graph. No layout/page/PDF decisions.
- `mosaic-layout`: semantic graph to page graph. No source parsing. No file emission.
- `mosaic-pdf`/`mosaic-html`: backend sinks. No lowering or layout policy.
- `mosaic-cli`: orchestrate, print diagnostics, map errors to exit codes. No compiler logic.
- `afm`/`pdf-base14-metrics`: isolated vendor/metrics layer.

## CONVENTIONS

- Use workspace deps/lints from root `Cargo.toml`.
- Prefer adding tests in the crate that owns behavior.
- Keep future crate dependencies out until source actually needs them.
- Stubs should fail clearly with `CoreError::Unimplemented`, not silent success.
- Public APIs should model current shipped behavior, not the full manifesto.

## ANTI-PATTERNS

- Do not make `mosaic-core` depend on parser/evaluator/layout/backends.
- Do not route parser behavior by directive name when `DirectiveKind` exists.
- Do not add backend-specific attributes to core unless every backend can tolerate them.
- Do not make CLI read `mosaic.toml` semantics unless explicitly implementing that slice.
