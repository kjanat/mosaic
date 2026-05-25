# Mosaic Manifest Tracker

This is the public roadmap/status artifact for implementation work against `manifest.md`. GitHub
Project 5 is the planning cockpit: milestones define roadmap phases, issues track concrete work, and
this file stays readable for people who do not live in the project board.

Truth order:

1. Code and tests.
2. `README.md` Status.
3. GitHub Project 5 for active planning state.
4. `manifest.md` design intent.

Do not mark a manifesto idea complete unless it is implemented, tested or clearly exercised by an
example.

## Planning Workflow

- GitHub Project 5 is the active planning board.
- Milestones are roadmap phases, not giant issues.
- Issues are concrete work records with testable acceptance criteria.
- Deeply slice only the current milestone; keep one milestone ahead lightly sliced.
- Do not convert every tracker checkbox into an issue.
- Split any issue that cannot fit in roughly 3-5 focused work days.
- A Ready issue needs clear scope, likely area/crates, dependencies, estimate, and test expectation.
- A Done issue means code is merged or intentionally closed, verification is recorded, and
  docs/tracker changed if behavior changed.

## Shipped Baseline

- [x] Rust 2024 workspace with strict workspace lints.
- [x] Crate split:
  - [x] `mos`
  - [x] `mos-core`
  - [x] `mos-parse`
  - [x] `mos-eval`
  - [x] `mos-layout`
  - [x] `mos-pdf`
  - [x] `mos-html`
  - [x] `mos-fonts`
  - [x] `mos-bib`
  - [x] `mos-cache`
  - [x] `mos-lsp`
  - [x] `mos-packages`
  - [x] `adobe-font-metrics`
  - [x] `pdf-base14-metrics`
  - [x] `tree-sitter-mosaic`
  - [x] `zed-mosaic`
- [x] `mos check <entry.mos|project-dir>` parses, lowers, resolves, and reports source diagnostics.
- [x] `mos build <entry.mos|project-dir>` parses, lowers, lays out, and writes PDF output:
      `build/<entry-stem>.pdf` for direct source files or project-declared `[output].pdf` paths.
- [x] `mos check` and `mos build` accept multiple entries and skip non-`.mos` glob matches.
- [x] Parser supports:
  - [x] headings: `=`, `==`, `===`
  - [x] paragraphs
  - [x] inline emphasis, strong text, and nested bold-italic text
  - [x] inline code
  - [x] inline line-break controls: `\\` hard break, `\-` soft hyphen, U+00A0 NBSP
  - [x] labels
  - [x] references
  - [x] unordered lists
  - [x] ordered lists
  - [x] `#set`
  - [x] `#image`
  - [x] `#figure`
- [x] Lowering and resolving support:
  - [x] typed semantic `Document`
  - [x] document metadata
  - [x] section numbering
  - [x] duplicate label diagnostics
  - [x] unknown label diagnostics
  - [x] generic reference text
  - [x] raster image directives
  - [x] simple figure directives
- [x] Layout supports:
  - [x] greedy text flow
  - [x] headings
  - [x] paragraphs
  - [x] lists with hanging indents
  - [x] images
  - [x] simple figures
  - [x] captions
  - [x] pages
  - [x] paper sizes
  - [x] margins
  - [x] text styles
  - [x] author-facing line-break controls: NBSP, hard breaks, and greedy SHY breaks
- [x] PDF backend supports:
  - [x] Base-14 metrics
  - [x] `/Differences`
  - [x] bundled Noto Sans embedding
  - [x] Noto Sans subsetting
  - [x] `/ToUnicode`
  - [x] PNG image XObjects
  - [x] JPEG image XObjects
  - [x] positioned embedded glyph output for GPOS advances/offsets
  - [x] title metadata
  - [x] author metadata
- [x] Example PDF snapshots exist for current examples.

## Immediate Cleanup

- [ ] Keep `README.md` Status aligned with shipped features.
- [ ] Keep `AGENTS.md` aligned with crate layout, commands, and current shipped scope.
- [ ] Update child `AGENTS.md` files when crate-local behavior changes.
- [ ] Audit comments that still describe landed work as MVP 0-only stubs.
- [x] Audit README workspace layout for current examples and crate list.
- [ ] Add or update tests before marking any item below complete.

## CLI

- [x] Wire `mos check`.
- [x] Wire `mos build`.
- [x] Accept project directories for `check`/`build` using `[project].entry`, with `main.mos` as the
      fallback entry.
- [x] Build project-declared PDF outputs from `[output].pdf` relative to the project directory.
- [x] Accept multiple `check`/`build` inputs and ignore non-`.mos` paths from shell globs.
- [ ] Decide whether stub commands should stay visible in `--help`.
- [ ] Implement `mos init`.
- [ ] Implement `mos watch`.
- [ ] Implement `mos fmt`.
- [ ] Implement `mos test`.
- [ ] Implement `mos profile`.
- [ ] Implement `mos clean`.
- [ ] Implement `mos package`.
- [ ] Add `mos graph` for dependency inspection.
- [ ] Add `mos bundle` for archival bundles.
- [ ] Add `mos convert` only after a scoped import plan exists.
- [ ] Add `mos build --frozen`.
- [ ] Add `mos build --debug-layout`.

## Parser And Syntax

- [x] Parse headings.
- [x] Parse paragraphs.
- [x] Parse inline emphasis, strong, nested bold-italic, and code.
- [x] Parse inline line-break controls: `\\` hard break, `\-` soft hyphen shorthand, and literal
      U+00A0 NBSP preservation.
- [x] Parse labels and references.
- [x] Parse unordered and ordered lists.
- [x] Parse current directives: `#set`, `#image`, `#figure`.
- [ ] Preserve comments in the CST if formatter or tooling needs them.
- [ ] Preserve useful formatting trivia for formatter support.
- [ ] Add imports/includes.
- [ ] Add citations.
- [ ] Add inline math.
- [ ] Add display math.
- [ ] Add equations.
- [ ] Add tables.
- [ ] Add theorem-like blocks.
- [ ] Add footnotes.
- [ ] Define and test function-call syntax beyond current directives.
- [ ] Define explicit grammar for document configuration.
- [ ] Document supported syntax in user-facing docs.

## Semantic Model And Resolver

- [x] Lower syntax into typed document nodes.
- [x] Preserve source spans for diagnostics.
- [x] Resolve section hierarchy for current headings.
- [x] Resolve section counters.
- [x] Resolve labels.
- [x] Diagnose duplicate labels.
- [x] Diagnose unknown references.
- [x] Resolve document metadata.
- [ ] Make reference text kind-aware:
  - [ ] section references
  - [ ] figure references
  - [ ] equation references
  - [ ] table references
  - [ ] theorem references
- [ ] Resolve figure numbering.
- [ ] Resolve equation numbering.
- [ ] Resolve table numbering.
- [ ] Resolve theorem numbering.
- [ ] Resolve citation keys.
- [ ] Add page-reference support.
- [ ] Add internal fixpoint loop for layout-dependent values.
- [ ] Add stable node IDs derived from durable inputs.
- [ ] Add content hashes for semantic nodes.

## Diagnostics

- [x] Render source diagnostics with file, line, column, and carets.
- [x] Report parse/lower/resolve errors without panicking.
- [x] Report duplicate and unknown labels.
- [ ] Add similar-label suggestions.
- [ ] Add diagnostic codes.
- [ ] Add structured suggestions.
- [ ] Add layout warnings.
- [ ] Add float placement diagnostics.
- [ ] Add performance diagnostics.
- [ ] Add non-convergence diagnostics for future fixpoint layout.

## Layout

- [x] Lay out headings, paragraphs, lists, images, figures, captions, and pages.
- [x] Support paper sizes and margins from current settings.
- [x] Support current text styles.
- [x] Normalize layout/font text inputs to NFC before measuring and shaping (issue #19).
- [x] Shape embedded-font text through rustybuzz and preserve GPOS advances/offsets in layout
      metrics (issue #20).
- [x] Carry embedded shaped glyphs and fallback sub-runs through text layout for PDF emission.
- [x] Author-facing line-break controls (issue #26): U+00A0 NBSP preserved by the greedy breaker,
      `\\` hard line break threaded through `InlineKind::HardBreak` / `NodeKind::HardBreak` /
      `WordItem::HardBreak`, and `\-` / U+00AD soft hyphen stripped from shaping with offsets
      consumed greedily by the line-breaker via `try_shy_break` — overflowing words pick the latest
      fitting SHY position and emit `prefix-` + suffix across two lines; oversize cluster fallback
      still applies when no SHY prefix fits.
- [ ] Replace greedy line breaking with a real paragraph algorithm. (Issue #26 piece 3b-e — Knuth-
      Plass + UAX #14 + SHY-as-penalty + optimal break selection. The greedy SHY hyphenation slice
      has landed; optimal whole-paragraph selection remains separate future work.)
- [ ] Add Unicode line breaking. (Same MVP 2 slice as Knuth-Plass; `unicode-linebreak` crate as the
      planned dependency.)
- [ ] Add language-aware hyphenation.
- [ ] Add full script/language/font-run segmentation.
- [ ] Add language/script-specific OpenType feature configuration.
- [ ] Add inline math layout.
- [ ] Add displayed equation layout.
- [ ] Add table layout.
- [ ] Add code block layout.
- [ ] Add keep-with-next behavior.
- [ ] Add widow/orphan control.
- [ ] Add footnote layout.
- [ ] Add page styles.
- [ ] Add layout constraints as explicit data.
- [ ] Add layout compromise reporting.

## Figures And Floats

- [x] Support raster images.
- [x] Support simple figures with captions.
- [ ] Add figure numbering.
- [ ] Add figure references with figure-aware labels.
- [ ] Add anchored float placement.
- [ ] Add allowed float positions.
- [ ] Add float priority.
- [ ] Add max-distance constraints.
- [ ] Add ordering penalties.
- [ ] Add whitespace penalties.
- [ ] Add list of figures.
- [ ] Add debug output for float decisions.

## PDF Backend

- [x] Emit deterministic PDFs for current layout output.
- [x] Emit Base-14 text.
- [x] Embed and subset bundled Noto Sans.
- [x] Emit `/ToUnicode` maps.
- [x] Emit PNG and JPEG images.
- [x] Emit title and author metadata.
- [x] Emit embedded shaped glyph runs with GPOS positioning via `TJ`/`Tm` operators.
- [ ] Add hyperlinks.
- [ ] Add bookmarks/outlines.
- [ ] Add vector graphics.
- [ ] Add image recompression or pass-through policy.
- [ ] Add tagged PDF support.
- [ ] Add PDF/A mode.
- [ ] Add source/PDF sync metadata if chosen.

## Other Backends

- [ ] Implement semantic HTML backend.
- [ ] Add fixed-layout HTML mode only if needed.
- [ ] Implement EPUB backend.
- [ ] Implement SVG page backend.
- [ ] Implement debug layout backend:
  - [ ] boxes
  - [ ] baselines
  - [ ] constraints
  - [ ] dirty nodes
  - [ ] float decisions
  - [ ] page break costs

## Bibliography

- [ ] Define citation syntax.
- [ ] Parse citations.
- [ ] Resolve citation keys.
- [ ] Load bibliography databases.
- [ ] Import BibTeX.
- [ ] Import BibLaTeX.
- [ ] Support CSL styles.
- [ ] Render numeric citations.
- [ ] Render author-year citations.
- [ ] Render footnote citations.
- [ ] Render citation clusters.
- [ ] Render sorted bibliographies.
- [ ] Track bibliography dependencies.
- [ ] Keep `mos-bib` stub docs honest until real support lands.

## Incremental Builds And Cache

- [ ] Define dependency IDs and dependency kinds.
- [ ] Track every computed artifact dependency.
- [ ] Track paragraph layout dependencies:
  - [ ] paragraph text
  - [ ] font metrics
  - [ ] available width
  - [ ] style
- [ ] Track figure dependencies:
  - [ ] image file
  - [ ] caption node
  - [ ] figure style
  - [ ] available width
- [ ] Track reference dependencies:
  - [ ] target label
  - [ ] target number
  - [ ] target page, where relevant
- [ ] Track TOC dependencies:
  - [ ] heading text
  - [ ] heading number
  - [ ] heading page
- [ ] Add dirty-node invalidation.
- [ ] Add paragraph layout cache.
- [ ] Add persistent `.mos-cache/`.
- [ ] Reuse clean semantic nodes.
- [ ] Recompute only affected paragraphs.
- [ ] Reflow only affected pages.
- [ ] Update only affected references.
- [ ] Report what changed during incremental builds.
- [ ] Add watch mode on top of incremental invalidation.

## Page Reflow And Fixpoints

- [ ] Add page graph as a first-class output of layout.
- [ ] Store page boundary signatures.
- [ ] Reflow from first changed page.
- [ ] Recompute pages until boundary state matches old build.
- [ ] Reuse remaining pages after convergence.
- [ ] Resolve layout-dependent values through a fixpoint:
  - [ ] page references
  - [ ] table of contents page numbers
  - [ ] list of figures page numbers
  - [ ] list of tables page numbers
  - [ ] index locators
- [ ] Detect oscillating documents.
- [ ] Keep hashes of global layout states during stabilization.
- [ ] Choose and document stable fallback policies.
- [ ] Report stabilization iteration counts.

## Project And Package System

- [x] Define current project directory contract: read `mosaic.toml` if present, use
      `[project].entry` for the entry source, otherwise fall back to `main.mos`.
- [x] Use `[output].pdf` for declared project PDF output paths.
- [ ] Use `mosaic.toml` for project metadata beyond the current entry/output fields.
- [ ] Use `mosaic.toml` for document settings.
- [ ] Use `mosaic.toml` for dependencies.
- [ ] Add `mosaic.lock`.
- [ ] Support package contents:
  - [ ] functions
  - [ ] styles
  - [ ] templates
  - [ ] assets
  - [ ] bibliography styles
  - [ ] layout policies
- [ ] Define pure packages.
- [ ] Define trusted packages with explicit consent.
- [ ] Prevent arbitrary native code execution by default.
- [ ] Resolve assets relative to project root unless explicitly overridden.
- [ ] Support section imports.
- [ ] Define standard project layout:
  - [ ] `mosaic.toml`
  - [ ] `mosaic.lock`
  - [ ] `main.mos`
  - [ ] `sections/`
  - [ ] `figures/`
  - [ ] `data/`
  - [ ] `refs/`
  - [ ] `styles/`
  - [ ] `build/`
  - [ ] `.mos-cache/`

## Formatting And Editor Integration

- [x] Keep `tree-sitter-mosaic` aligned with current parser syntax, including nested emphasis and
      line-break controls.
- [x] Mirror shared Zed highlight queries from `tree-sitter-mosaic` and pin the Zed grammar
      revision.
- [ ] Build `mos fmt`.
- [ ] Define formatting rules for current syntax.
- [ ] Format multiline function calls.
- [ ] Preserve comments and meaningful trivia.
- [ ] Complete `mos-lsp` beyond the current entry point.
- [ ] Publish diagnostics over LSP.
- [ ] Add go-to-definition for labels.
- [ ] Add rename label.
- [ ] Add citation autocomplete.
- [ ] Add figure preview.
- [ ] Add outline.
- [ ] Add symbol search.
- [ ] Add hover docs.
- [ ] Add format-document support.
- [ ] Add live preview sync.
- [ ] Add bidirectional source/PDF sync.
- [ ] Decide sync storage format:
  - [ ] sidecar `.mosync`
  - [ ] PDF metadata

## Determinism And Reproducibility

- [ ] Keep output ordering deterministic.
- [ ] Use stable iteration order where output-observable.
- [ ] Pin package versions through a lockfile.
- [ ] Pin font resolution in reproducible mode.
- [ ] Prevent undeclared network access during builds.
- [ ] Prevent undeclared system time during builds.
- [ ] Track `today()` or equivalent as a dependency if introduced.
- [ ] Include engine version in reproducible build inputs.
- [ ] Include layout policy in reproducible build inputs.
- [ ] Include asset hashes in reproducible build inputs.
- [ ] Add archival bundle support.
- [ ] Include fonts in bundles only when licenses permit.

## Scripting, Styles, And Templates

- [ ] Define Mosaic expression language scope.
- [ ] Decide whether advanced scripting uses custom language, Rhai, Starlark, WASM, or another host.
- [ ] Prefer native Mosaic expressions for normal templates.
- [ ] Define WASM plugin API only after package permissions are designed.
- [ ] Require plugin/package manifests.
- [ ] Make capabilities explicit:
  - [ ] filesystem
  - [ ] network
  - [ ] determinism
- [ ] Implement predictable style cascade:
  - [ ] document defaults
  - [ ] template defaults
  - [ ] package styles
  - [ ] local style rules
  - [ ] inline overrides
- [ ] Make templates normal packages.
- [ ] Expose template parameters.

## Testing

- [x] Workspace tests run through `cargo test --workspace`.
- [x] Strict clippy command exists through `cargo lint`.
- [x] Example snapshot regeneration exists through `just examples`.
- [x] Tree-sitter corpus/highlight tests cover current line-break controls.
- [ ] Add syntax tests for every supported grammar construct.
- [ ] Add semantic lowering tests for every node type.
- [ ] Add reference resolution tests by reference kind.
- [ ] Add layout tree snapshot tests.
- [ ] Add PDF metadata tests.
- [ ] Add image PDF emission tests.
- [ ] Add visual regression tests.
- [ ] Compare layout trees instead of raw PDFs where practical.
- [ ] Keep examples current when features change.

## Later Or Parked Manifest Ideas

These are design goals, not near-term implementation commitments.

- [ ] Full constraint graph layout solver.
- [ ] Cost-based regional page optimizer.
- [ ] Table solver with intrinsic/fixed/fractional sizing.
- [ ] Multipage tables with repeated headers.
- [ ] Index rendering.
- [ ] Glossary rendering.
- [ ] DOI metadata import.
- [ ] Package registry.
- [ ] Sandboxed package execution.
- [ ] Language-version gates.
- [ ] Markdown import.
- [ ] Pandoc JSON import.
- [ ] Limited LaTeX math import.
- [ ] Best-effort LaTeX document conversion.
- [ ] Avoid arbitrary LaTeX package compatibility.

## Non-Goals To Preserve

- [ ] Do not become fully LaTeX-compatible.
- [ ] Do not become a general programming language.
- [ ] Do not become a web browser.
- [ ] Do not become a desktop publishing GUI.
- [ ] Do not become a Word clone.
- [ ] Do not become a CSS clone.
- [ ] Do not become a markdown-only toy.

## Priority Rules

- [ ] Prefer semantic correctness over visual cleverness.
- [ ] Prefer deterministic builds over hidden convenience.
- [ ] Prefer clear diagnostics over silent fallback.
- [ ] Prefer scoped MVP slices over broad systems.
- [ ] Keep compiler/domain logic out of the `mos` CLI crate.
- [ ] Keep parse, lower, layout, and emit boundaries explicit.
