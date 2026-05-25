# PROJECT KNOWLEDGE BASE

**Generated:** 2026-05-25 **Commit:** 2dc6844 **Branch:** master

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
- If `CLAUDE.md` exists as a symlink to this file, edit `AGENTS.md`. Do not replace or separately
  maintain `CLAUDE.md`.

## CURRENT STATUS

Implemented now:

- `mos check`: parse, lower, resolve, source diagnostics; accepts `.mos` files or project dirs.
- `mos build`: parse, lower, layout, emit PDF under `build/<entry-stem>.pdf` for direct files or
  project-declared `[output].pdf` paths.
- Parser: headings, paragraphs, inline emphasis/strong/nested bold-italic/code, labels/references,
  lists, `#set`, `#image`, `#figure`, `\\` hard break, `\-` soft hyphen, U+00A0 NBSP.
- Lowerer/resolver: semantic `Document`, metadata, section numbering, duplicate/unknown label
  diagnostics, generic reference text, hard-break semantic nodes.
- Layout: greedy text flow, headings, paragraphs, lists, images, simple figures/captions, pages,
  paper sizes, margins, text styles, NBSP/hard-break/greedy soft-hyphen controls, NFC-normalized
  text, embedded glyph fallback sub-runs.
- PDF: Base-14 metrics, `/Differences`, bundled Noto Sans embedding/subsetting, `/ToUnicode`,
  GPOS-positioned embedded glyph output, PNG/JPEG image XObjects, title/author Info metadata.

Treat as aspirational/stub unless user asks:

- HTML/EPUB/SVG backends, LSP behavior, bibliography, persistent cache, watch mode.
- Package registry/lockfile resolution, formatter, scripting/functions/templates.
- Math/equations/tables/theorems/footnotes/index/glossary.
- Float solver, TOC/page refs, layout fixpoint over pagination, Knuth-Plass, automatic hyphenation,
  Unicode line breaking.
- Reproducible/frozen build semantics, import/conversion tools.

## STRUCTURE

```text
./
├── Cargo.toml          # virtual workspace; no root src/
├── manifest.md         # roadmap/design manifesto, partly ahead/stale
├── manifest-tracker.md # public roadmap/status derived from manifest + current code
├── README.md           # best quick status doc
├── crates/             # 15 workspace crates + excluded Zed extension
├── examples/           # self-contained .mos projects + committed PDF snapshots
├── justfile            # fmt + example snapshot regeneration
├── .cargo/config.toml  # cargo aliases: bw/cw/tw/dw/lint/mos
└── .github/workflows/  # CI/docs on master
```

## WHERE TO LOOK

| Task              | Location                         | Notes                                                       |
| ----------------- | -------------------------------- | ----------------------------------------------------------- |
| Current status    | `README.md`                      | More accurate than manifest for shipped behavior.           |
| Active planning   | GitHub Project 5                 | Milestones are phases; issues are concrete work records.    |
| Roadmap status    | `manifest-tracker.md`            | Public roadmap/status; keep aligned with code and README.   |
| Product direction | `manifest.md`                    | Design intent; many features not built.                     |
| CLI behavior      | `crates/mos/src/main.rs`         | `check` and `build` real; other subcommands fail by design. |
| Syntax            | `crates/mos-parse/src/lib.rs`    | CST, spans, recoverable parse diagnostics.                  |
| Semantic lowering | `crates/mos-eval/src/lib.rs`     | Directives, images, figures, metadata.                      |
| References        | `crates/mos-eval/src/resolve.rs` | Generic labels/refs, partial MVP 1.                         |
| Document model    | `crates/mos-core/src/lib.rs`     | Bottom-layer IDs, nodes, diagnostics.                       |
| Layout            | `crates/mos-layout/src/lib.rs`   | Biggest hotspot, stateful page/text flow.                   |
| Font rules        | `crates/mos-fonts/src/lib.rs`    | Base-14 + embedded Noto Sans.                               |
| PDF output        | `crates/mos-pdf/src/lib.rs`      | Deterministic object/font/image emission.                   |
| Metrics data      | `crates/pdf-base14-metrics/`     | Vendored AFM/AGL, build-generated Rust.                     |
| Editor grammar    | `crates/tree-sitter-mosaic/`     | Tree-sitter grammar/queries; not compiler truth.            |
| Zed extension     | `crates/zed-mosaic/`             | Excluded from workspace; copied query bundle.               |
| Examples          | `examples/`                      | Snapshot PDFs regenerated by `just examples`.               |

## CRATE FLOW

```text
adobe-font-metrics -> pdf-base14-metrics -> mos-fonts -> mos-layout -> mos-pdf
mos-core -> mos-parse -> mos-eval -> mos-layout
mos orchestrates core/eval/layout/pdf
```

`mos-html`, `mos-bib`, `mos-cache`, `mos-lsp`, and parts of `mos-packages` are mostly skeletons or
partial foundations.

`crates/tree-sitter-mosaic` is editor syntax infrastructure. `crates/zed-mosaic` is a Zed extension
under `crates/` but excluded from the Cargo workspace.

## CONVENTIONS

- Rust stable, edition 2024, MSRV 1.95, workspace resolver 3.
- Workspace lints are strict. `unsafe_code = "forbid"`; CI uses `-D warnings -D clippy::all`.
- Clippy set is curated. Do not enable whole pedantic/nursery/restriction groups.
- Formatting runs through `dprint`; TOML via `tombi`; Rust via `rustfmt`.
- Tests must stay clippy-clean. Many tests avoid `unwrap`, `expect`, and raw `panic`.
- Keep domain direction one-way. CLI glues; parse does not lower; layout does not emit PDF.
- Use existing `CoreError`/`Diagnostic` paths for user errors. No panics for bad documents.

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

## COMMANDS

Use `run ...` when the user writes it; otherwise use the project commands directly.

```bash
cargo bw
cargo cw
cargo tw
cargo lint
cargo mos check examples/hello/main.mos
cargo mos build examples/hello/main.mos
just fmt
just examples
```

## GOTCHAS

- CI branch is `master`, not `main`.
- `RUSTFLAGS=-D warnings` and `RUSTDOCFLAGS=-D warnings` make warnings fatal in CI.
- `dprint` extends remote config; first run may need network/cache.
- `just examples` mutates committed snapshot PDFs.
- `just sync-zed-queries` overwrites copied Zed query files from Tree-sitter query sources.
- Base-14 unsupported codepoints can become `?`; embedded Noto path covers more Latin/Unicode.
