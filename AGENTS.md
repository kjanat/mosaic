# PROJECT KNOWLEDGE BASE

**Generated:** 2026-06-19 **Commit:** d491cc6 **Branch:** feat/hanging-list-syntax

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

## CURRENT STATUS

Shipped slice:

- `mos check`: parse, lower, resolve, source diagnostics for files/projects.
- `mos build`: parse, lower, layout, PDF output under direct-file or `[output].pdf` paths.
- Parser/eval: headings, paragraphs, lists, inline styling/code/refs/page-refs/citations, raw
  `#pre`/`#code` blocks, `#set`, images, figures, hard breaks, soft hyphen, NBSP, `#bibliography`,
  `//` line + `/* */` block + `/** */` doc comments (recognized and dropped; verbatim in code/raw;
  unterminated `/*` is `MOS0050`).
- Bibliography: minimal BibTeX + CSL data/style parsing, citation-key checks, numeric `[@key]`
  labels by first use, and a rendered cited-entry list (plain text, first-use order) at the
  `#bibliography` site. CSL-styled rendering is not shipped.
- Layout/PDF: greedy flow, pages, figures/images, Base-14 + Noto embedding/subsetting, ToUnicode,
  heading bookmarks (`/Outlines`), deterministic provenance.
- LSP: diagnostics, definition for labels/citations, label rename, document symbols, code actions,
  hover (a symbol's `/** … */` doc comment).

Aspirational/stub unless user asks: HTML/EPUB/SVG, persistent cache, watch/formatter/package
systems, math/tables/footnotes/index/glossary, float solver, TOC generation, broad pagination
fixpoints, Knuth-Plass, automatic hyphenation, reproducible/frozen builds, import/conversion tools.

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
├── package.json        # Bun workspace, patched deps, formatter/runner tooling
├── .cargo/             # cargo aliases + nightly doc config
├── .github/            # CI/docs/release workflows + Pages action
└── .opencode/          # Bun/TypeScript OpenCode helper tools
```

## WHERE TO LOOK

| Task               | Location                              | Notes                                                       |
| ------------------ | ------------------------------------- | ----------------------------------------------------------- |
| CLI behavior       | `crates/mos/src/main.rs`              | `check` and `build` real; other subcommands fail by design. |
| Syntax             | `crates/mos-parse/src/lib.rs`         | CST, spans, recoverable parse diagnostics.                  |
| Semantic lowering  | `crates/mos-eval/src/lib.rs`          | Directives, images, figures, metadata, citations.           |
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
| Cargo aliases      | `.cargo/config.toml`                  | `bw`/`bwa`/`cw`/`tw`/`lint`/`mos`/`mosls`.                  |
| CI/release         | `.github/`                            | CI path ignores, Pages action, crates.io, binaries.         |
| Agent tooling      | `.opencode/tools/`                    | GitHub Project 5 and PR helper tools.                       |

## CRATE FLOW

- Core path: `mos-core -> mos-parse -> mos-eval -> mos-layout -> mos-pdf`; `mos` orchestrates.
- Font path: `adobe-font-metrics -> pdf-base14-metrics -> mos-fonts -> mos-layout`.
- Partial: `mos-html`, persistent cache wiring, `mos-packages`, CSL processor. Numeric citation
  labels and the cited-entry bibliography list are real.
- Editor side worlds: `tree-sitter-mosaic` syntax infrastructure; `zed-mosaic` excluded extension.

## CONVENTIONS

- Rust stable, edition 2024, workspace/product MSRV 1.96, workspace resolver 3.
- Public Rust crate APIs are pre-alpha: patch releases may break APIs. Tell external crate consumers
  to pin exact patch versions.
- `CLAUDE.md` is an `@AGENTS.md` import (not a symlink); edit `AGENTS.md` only.
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
- Do not claim *CSL-styled* bibliography rendering is shipped: resolved `[@key]` markers render
  numeric labels and cited entries render as a plain-text numbered list in first-use order, but CSL
  styles, author-year citations, and uncited-entry output do not exist yet.
- Do not search or edit `crates/zed-mosaic/grammars/mosaic` as source; it is an ignored local clone
  trap from Zed grammar setup.

## COMMANDS

```bash
cargo bw
cargo cw
cargo tw
cargo lint
cargo bwa
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
- `mos build examples/*` writes one fewer PDF than it has args, and that is **by design, not a
  bug**: the glob sweeps in `examples/AGENTS.md`, and multi-entry `mos build` silently skips
  non-`.mos` *files* so globbed docs/READMEs don't error (`should_skip_glob_file` / `is_mos_source`,
  gated on `entries.len() > 1`, in `crates/mos/src/main.rs`). Directory args always resolve to
  `main.mos`, so only bare non-`.mos` files are skipped. Run alone, `mos build examples/AGENTS.md`
  has no skip and *does* build it (into the gitignored `examples/build/`). Do not re-investigate the
  8-in/7-out as a defect.
- `just sync-zed-queries` overwrites copied Zed query files from Tree-sitter query sources.
- `crates/tree-sitter-mosaic/src/parser.c`, `src/grammar.json`, and `src/node-types.json` are
  generated from `grammar.js` and scanner code.
- Release: `just bump X.Y.Z`, cut `CHANGELOG.md`, tag signed `v*` on work commit, preserve generated
  release notes. Versioning policy and decoupling playbook: `docs/versioning.md`.
