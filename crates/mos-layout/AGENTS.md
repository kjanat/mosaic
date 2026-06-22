# MOSAIC-LAYOUT KNOWLEDGE BASE

## OVERVIEW

`mos-layout` converts `mos-core::Document` into `PageGraph`. It is current biggest hotspot: style
resolution, greedy text flow, lists, images, figures, and pagination live mostly in `src/lib.rs`.

## CURRENT SCOPE

Implemented:

- Paper sizes, margins, body text size, leading, font family.
- Headings, paragraphs, inline style runs, generic references already lowered to text.
- Greedy word wrapping and oversized-word character wrapping.
- Unordered/ordered lists with hanging indents and nesting.
- Images and simple figures with caption keep-together when possible.
- Raw blocks with monospace flow, tab expansion, and original text preservation.
- Page breaking and layout diagnostics for invalid config.

Not implemented here yet:

- Constraint graph, Knuth-Plass, hyphenation, float solver, tables, equations, footnotes.
- Widow/orphan/keep-with-next constraints, TOC generation, broad pagination stabilization,
  incremental reflow/cache.

## WHERE TO LOOK

| Task             | Location                   | Notes                                                                                     |
| ---------------- | -------------------------- | ----------------------------------------------------------------------------------------- |
| Public types     | `src/lib.rs` top section   | `PageStyle`, `TextStyle`, `PageGraph`, `TextRun`.                                         |
| Page signatures  | `src/boundary.rs`          | `PageBoundarySignature`/`PageGraphSignature`; §4.5 `PageOutputHash` + `first_divergence`. |
| Label → page map | `LayoutResult.label_pages` | label → start page; bound via `pending_labels`/`bind_pending_labels` (issue #72).         |
| Entry point      | `LayoutEngine::layout`     | Root node dispatch.                                                                       |
| `#set` page/text | `resolve_styles` helpers   | Invalid values warn and keep prior style.                                                 |
| Mutable engine   | `LayoutState`              | Cursor, pages, styles, pending list marker.                                               |
| Images           | `layout_image`             | Sizing, page breaks, image placement.                                                     |
| Figures          | `layout_figure`            | Dry-run measurement must match real flow.                                                 |
| Lists            | `layout_list`              | Nesting, marker gutter, pending marker restore.                                           |
| Raw blocks       | `layout_raw_block`         | Monospace, tab expansion, hard line preservation.                                         |
| Text flow        | `flow_words`, `flush_line` | Core wrapping and pagination behavior.                                                    |
| Word/break items | `src/word.rs`              | `Word`, `WordItem`, `split_soft_hyphens`.                                                 |

## CONVENTIONS

- Consume `Document`; do not parse source or inspect `.mos` syntax.
- Emit `LayoutResult`; do not write files or PDF objects.
- Layout warnings do not fail `mos build` today.
- Unknown/future top-level node kinds are ignored for forward compatibility.
- Invalid style changes keep previous value, not default value.
- Tests live inline in `src/lib.rs`; add semantic tests near related behavior.

## HOT INVARIANTS

- `cursor_y` semantics are delicate: sometimes top edge, sometimes next baseline. Read local
  comments.
- Figure dry-run and real text flow must not drift.
- `current_left_pt` and `pending_marker` must restore after nested lists.
- Font family names come through semantic attrs; unknown fonts warn and fall back.
- Keep layout deterministic. No hash-map-dependent output order.
- NBSP (U+00A0) must stay inside a `Word`. The text extractor in `collect_words` splits on ASCII
  whitespace only: do not swap in a UAX #14 splitter without preserving this contract.
- `flow_words` consumes `Vec<WordItem>`; `WordItem::HardBreak` flushes the current line (or advances
  one blank line if the buffer is empty) without paragraph spacing. Anything that builds word
  streams (headings, raw blocks, list items) must wrap `Word`s in `WordItem::Word`.
- `Word.shy_break_offsets` records SHY (U+00AD) byte positions in the stripped `text`. The greedy
  breaker consumes them via `try_shy_break` in `src/word.rs`: when a word would overflow the line it
  picks the latest fitting SHY offset, emits `prefix-` (with a visible hyphen) and threads the
  suffix back as the next word via a `pending` slot in `flow_words`. Boundary offsets (`0` /
  `text.len()`) are ignored; if no SHY prefix fits even an empty line, `flush_oversize_word`'s
  cluster fallback runs. The future Knuth-Plass breaker will reuse the same offsets as Penalty
  points for optimal (non-greedy) selection.

## ANTI-PATTERNS

- Do not sneak in PDF/backend logic.
- Do not start full constraint-solver work unless user asked for that MVP slice.
- Do not panic on bad document/style input. Emit diagnostics and keep going when safe.
