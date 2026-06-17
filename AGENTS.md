# PROJECT KNOWLEDGE BASE

**Generated:** 2026-06-03 **Commit:** b6efdf5 **Branch:** master

## OVERVIEW

Mosaic is a pre-alpha Rust 2024 workspace for a `.mos` typesetting compiler. Current product path is
`mos check` and `mos build` to PDF; `manifest.md` is roadmap/design intent, not shipped truth.

## TRUTH ORDER

1. Current code and tests.
2. README Status section.
3. GitHub Project 5 for active planning state.
4. `manifest-tracker.md` for public roadmap/status.
5. `manifest.md` for product direction only.

If `manifest.md` disagrees with code, trust code. Mention mismatch. Do not silently build future MVP
features because manifest dreams loudly.

## MAINTENANCE

- Update this file when repo structure, commands, shipped scope, or hard-won conventions change.
- Keep GitHub Project 5 as the planning cockpit: milestones are phases, issues are concrete work.
- Update `manifest-tracker.md` when shipped features or public roadmap/status changes.
- Update child `AGENTS.md` files when local crate/example rules change enough to help future agents.
- `CLAUDE.md` is a symlink, edit `AGENTS.md` only.

## CURRENT STATUS

Implemented now:

- `mos check`: parse, lower, resolve, source diagnostics; accepts `.mos` files or project dirs.
- `mos build`: parse, lower, layout, emit PDF under `build/<entry-stem>.pdf` for direct files or
  project-declared `[output].pdf` paths.
- Parser: headings, paragraphs, inline emphasis/strong/code, refs/citations, lists, `#set`, images,
  figures, hard breaks, soft hyphen, NBSP.
- Lowerer/resolver: semantic `Document`, metadata, section numbering, labels/refs, single-key
  citations, bibliography source loading/key checks, images/figures, hard-break semantic nodes.
- Bibliography foundations: `#bibliography("refs.bib")`, minimal BibTeX parsing, citation-key
  resolution diagnostics, and CSL data/style parsing. Resolved `[@key]` markers render numeric
  labels (`[1]`, `[2]`, ... by first-use order); bibliography-list rendering is not shipped yet.
  Dependency tracking: `BibliographyDependency` pairs a `Bibliography` id with a content-hash
  boundary (`mos_bib::bibliography_content_hash`); identity/boundary only, no cache graph yet.
- Layout: greedy text flow, headings, paragraphs, lists, images, simple figures/captions, pages,
  paper/margin/style controls, Unicode/glyph fallback basics.
- PDF: Base-14 metrics, bundled Noto Sans embedding/subsetting, ToUnicode, images, metadata,
  deterministic provenance stamp.
- LSP: `mos-lsp` stdio server publishes current compiler parse/lower/resolve diagnostics for opened
  and changed documents, plus go-to-definition for labels and resolved citations, rename for labels,
  nested heading document symbols, and code actions from compiler suggestions. The `zed-mosaic`
  extension spawns `mos-lsp` (binary discovered via Zed settings `binary.path`, `PATH`, or
  downloaded release asset fallback).

Treat as aspirational/stub unless user asks:

- HTML/EPUB/SVG backends, richer LSP, bibliography rendering, persistent cache, watch mode.
- Package registry/lockfile, formatter, scripting/templates, math/tables/footnotes/index/glossary.
- Float solver, TOC/page refs, pagination fixpoints, Knuth-Plass, automatic hyphenation.
- Reproducible/frozen build semantics and import/conversion tools.

## STRUCTURE

```text
./
├── Cargo.toml          # virtual workspace; no root src/
├── manifest.md         # roadmap/design manifesto, partly ahead/stale
├── manifest-tracker.md # public roadmap/status derived from manifest + current code
├── README.md           # best quick status doc
├── crates/             # 16 workspace crates + excluded Zed extension
├── docs/               # developer docs; diagnostic-codes.md mirrors the registry
├── examples/           # self-contained .mos projects + committed PDF snapshots
├── justfile            # runner-backed fmt/examples/docs recipes
├── package.json        # Bun workspace + local formatter/runner tooling
├── .cargo/config.toml  # cargo aliases: bw/cw/tw/dw/lint/mos
├── .github/workflows/  # CI/docs/release on master and v* tags
└── .opencode/          # Bun/TypeScript OpenCode helper tools
```

## WHERE TO LOOK

| Task               | Location                              | Notes                                                       |
| ------------------ | ------------------------------------- | ----------------------------------------------------------- |
| Current status     | `README.md`                           | More accurate than manifest for shipped behavior.           |
| Active planning    | GitHub Project 5                      | Milestones are phases; issues are concrete work records.    |
| Roadmap status     | `manifest-tracker.md`                 | Public roadmap/status; keep aligned with code and README.   |
| Product direction  | `manifest.md`                         | Design intent; many features not built.                     |
| CLI behavior       | `crates/mos/src/main.rs`              | `check` and `build` real; other subcommands fail by design. |
| Syntax             | `crates/mos-parse/src/lib.rs`         | CST, spans, recoverable parse diagnostics.                  |
| Semantic lowering  | `crates/mos-eval/src/lib.rs`          | Directives, images, figures, metadata.                      |
| References         | `crates/mos-eval/src/resolve.rs`      | Labels/refs, section/figure numbering.                      |
| Bibliography eval  | `crates/mos-eval/src/bibliography.rs` | `#bibliography`, `.bib` loading, citation-key checks.       |
| BibTeX parser      | `crates/mos-bib/src/`                 | Minimal records; no rendering/styling.                      |
| CSL foundations    | `crates/mos-csl/src/`                 | CSL item model, BibTeX map, style parser; no processor.     |
| Document model     | `crates/mos-core/src/lib.rs`          | Bottom-layer IDs, nodes, diagnostics.                       |
| Diagnostic codes   | `crates/mos-core/src/codes.rs`        | `MOS####` registry (truth source); `define_codes!` macro.   |
| Diagnostic catalog | `docs/diagnostic-codes.md`            | Human mirror of the registry; drift-tested in CI.           |
| Layout             | `crates/mos-layout/src/lib.rs`        | Biggest hotspot, stateful page/text flow.                   |
| Font rules         | `crates/mos-fonts/src/lib.rs`         | Base-14 + embedded Noto Sans.                               |
| PDF output         | `crates/mos-pdf/src/lib.rs`           | Deterministic object/font/image emission.                   |
| Metrics data       | `crates/pdf-base14-metrics/`          | Vendored AFM/AGL, build-generated Rust.                     |
| Editor grammar     | `crates/tree-sitter-mosaic/`          | Tree-sitter grammar/queries; not compiler truth.            |
| Zed extension      | `crates/zed-mosaic/`                  | Excluded from workspace; copied query bundle.               |
| Examples           | `examples/`                           | Snapshot PDFs regenerated by `just examples`.               |
| Developer docs     | `docs/`                               | Design notes; do not overclaim shipped behavior.            |
| CI/release         | `.github/workflows/`                  | CI path ignores, docs deploy, crates.io publish.            |
| Agent tooling      | `.opencode/tools/`                    | GitHub Project 5 and PR helper tools.                       |

## CRATE FLOW

- Core path: `mos-core -> mos-parse -> mos-eval -> mos-layout -> mos-pdf`; `mos` orchestrates.
- Font path: `adobe-font-metrics -> pdf-base14-metrics -> mos-fonts -> mos-layout`.
- Partial/integration-pending: `mos-html`, `mos-cache`, parts of `mos-packages`, most LSP features,
  bibliography rendering, and the CSL processor. `mos-bib` parsing and `mos-csl` data/style parsing
  are real shipped foundations.
- Editor side worlds: `tree-sitter-mosaic` syntax infrastructure; `zed-mosaic` excluded extension.

## CONVENTIONS

- Rust stable, edition 2024, workspace/product MSRV 1.96, workspace resolver 3.
- Workspace lints are strict. `unsafe_code = "forbid"`; CI uses `-D warnings -D clippy::all`.
- Clippy set is curated. Do not enable whole pedantic/nursery/restriction groups.
- Formatting runs through local `dprint`; TOML via `tombi`; Rust via `rustfmt`; `justfile` via
  `just --dump`.
- Tests must stay clippy-clean. Many tests avoid `unwrap`, `expect`, and raw `panic`.
- Keep domain direction one-way. CLI glues; parse does not lower; layout does not emit PDF.
- Use existing `CoreError`/`Diagnostic` paths for user errors. No panics for bad documents.
- Diagnostics: `MOS####` codes are opaque/stable and minted only in `mos-core::codes`. Add a code by
  editing `codes.rs` + `docs/diagnostic-codes.md` together (drift-tested). CLI phase barriers run a
  phase to completion, then exit if any error was collected.

## ANTI-PATTERNS

- Do not treat `manifest.md` examples as implemented language features.
- Do not add broad MVP systems unless user asked for that slice.
- Do not put compiler/domain logic in `mos`.
- Do not edit generated `$OUT_DIR/baked.rs`; source is `pdf-base14-metrics/data` + `build.rs`.
- Do not assume `mosaic.toml` output controls CLI yet; current `mos build` reads entry source.
- Do not commit `examples/*/build/main.pdf`; commit `examples/*/<name>.pdf` snapshots only.
- Do not invent font shaping. Manifest says HarfBuzz/equivalent for a reason.
- Do not treat Tree-sitter grammar as the compiler parser or shipped language truth.
- Do not assume `crates/zed-mosaic` participates in workspace Cargo commands.
- Do not claim *bibliography-list* rendering is shipped: resolved `[@key]` markers now render
  numeric labels (`[1]`, ...), but the sorted bibliography entry list is not rendered yet.

## COMMANDS

```bash
cargo bw
cargo cw
cargo tw
cargo lint
cargo mos check examples/hello/main.mos
cargo mos build examples/hello/main.mos
cargo mosls
just setup
just fmt
just examples
just doc-nightly
```

## GOTCHAS

- CI branch is `master`, not `main`.
- `RUSTFLAGS=-D warnings` and `RUSTDOCFLAGS=-D warnings` make warnings fatal in CI.
- `dprint` extends remote config; first run may need network/cache.
- `just fmt`, `just examples`, and `just doc-nightly` run through `runner-run`; `just setup`
  bootstraps it.
- `just examples` mutates committed snapshot PDFs via `runner mos build examples/*`.
- `just sync-zed-queries` overwrites copied Zed query files from Tree-sitter query sources.
- Base-14 unsupported codepoints can become `?`; embedded Noto path covers more Latin/Unicode.
- Release hygiene: cut `CHANGELOG.md` into a version section before tagging, then create the signed
  `v*` tag on that release commit. The crates.io workflow publishes the whole workspace in
  dependency order; GitHub release notes should preserve the generated `What's Changed` section.
