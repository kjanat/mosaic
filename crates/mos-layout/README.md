# mos-layout

`mos-layout` turns a lowered `mos_core::Document` into a paginated `PageGraph`. It is layout policy
only: it consumes semantic nodes, resolves page/text style, flows blocks, and returns text/image
placements plus diagnostics. It does not parse `.mos`, lower syntax, write files, or emit PDF
objects.

## Current Support

- Entry point: `LayoutEngine::layout(&Document) -> LayoutResult`.
- Output: `PageGraph { pages, images }` with `TextRun` and `ImagePlacement`.
- Defaults: A4 page, symmetric 24mm margin, 11pt Noto Sans body text, 1.35 leading.
- `#set page(...)`: paper size and symmetric margin.
- `#set text(...)`: size, leading, font family.
- Blocks: headings, paragraphs, lists, raw code blocks, images, simple figures.
- Inline styles: regular text, emphasis, strong, inline raw/code, resolved references as text.
- Text flow: greedy word wrapping, pagination, oversized-word cluster wrapping.
- Lists: unordered and ordered markers, hanging indent, nested list indentation.
- Images: decoded RGB data from `mos-eval`, natural or declared size, width capped to column,
  centered placement, deduped handles by resolved path.
- Figures: image plus caption flow, keep-together when remaining page space allows.
- Fonts: uses `mos-fonts` for Base-14 metrics, bundled Noto Sans, shaping, and fallback sub-runs.

Invalid layout config emits diagnostics and keeps the prior valid style where possible. Layout
warnings/errors currently do not make `mos build` fail by themselves.

## Module Layout

- `src/lib.rs`: public re-exports, `LayoutEngine`, root-node dispatch, `LayoutState`, text flow, raw
  blocks, pagination, and most inline collection.
- `src/types.rs`: public page/text/image/run/result types and default page constants.
- `src/style.rs`: `#set page(...)` and `#set text(...)` folding, paper-size resolution, validation.
- `src/image.rs`: image sizing, image handle interning, image placement, figure measurement/flow.
- `src/list.rs`: ordered/unordered list layout, marker gutter, nested list state restore.
- `src/word.rs`: shaped word representation and cluster splitting for oversized words.
- `src/support.rs`: small attribute readers, blank pages, tab expansion.

Tests live inline near the behavior they cover.

## Invariants

- Consume `mos_core::Document`; never inspect source syntax directly.
- Return `LayoutResult`; never write files, bytes, or backend-specific PDF structures.
- Keep output deterministic. Page/image order is source/layout order.
- Always emit at least one page, even for an empty document.
- Unknown/future top-level node kinds are ignored rather than panicking.
- Invalid style changes retain the previous valid value, not a reset default.
- `cursor_y`, `current_left_pt`, and `pending_marker` are stateful and fragile; restore list state
  after nested layout.
- Figure dry-run measurement and real flow must stay aligned enough for keep-together behavior.
- Text coordinates use page-top origin; PDF backend flips them later.

## Example

```rust
use std::path::PathBuf;

use mos_core::Document;
use mos_layout::LayoutEngine;

let doc = Document::new(PathBuf::from("main.mos"));
let result = LayoutEngine::new().layout(&doc);

assert_eq!(result.graph.pages.len(), 1);
```

In the real pipeline, `mos-eval` builds the `Document`, `mos-layout` builds the `PageGraph`, and
`mos-pdf` consumes the graph.

## Known Non-Goals

- No parsing or semantic lowering.
- No PDF, HTML, EPUB, SVG, or file emission.
- No full constraint graph, Knuth-Plass, hyphenation, float solver, or layout fixpoint.
- No tables, equations, footnotes, bibliography, TOC, or page-reference resolution here.
- No widow/orphan or keep-with-next constraints.
- No incremental reflow/cache boundary reuse.
- No font discovery or advanced shaping policy beyond what `mos-fonts` exposes.
