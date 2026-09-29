# mos-text-layout

A dependency-free, source-preserving index over measured text segments, owned by Mosaic and usable
by interactive editors or typesetters.

`SegmentIndex` accepts original byte lengths and nonnegative measured advances. It provides
logarithmic source-offset, horizontal-position, and viewport-range lookup. Segments may represent
shaped clusters, runs, or inline elements. It never normalizes source text or chooses fonts.

```rust
use mos_text_layout::SegmentIndex;

let index = SegmentIndex::new([(3, 12.0), (4, 20.0)])?;
assert_eq!(index.segment_for_offset(3), Some(1));
assert_eq!(index.intersecting(10.0..15.0), 0..2);
# Ok::<(), mos_text_layout::IndexError>(())
```

Segments must be in source and left-to-right spatial order. Bidirectional reordering within a
segment belongs to the shaper; arbitrarily reordered source segments need a separate mapping. UTF-8
and shaping-cluster boundaries are the caller's responsibility. Advance-based viewport lookup
requires padding for visible glyph overhang. Zero lengths and advances are supported.

This is an index over existing measurements, not a shaper or a complete virtualization engine.
Construction and memory are linear in segment count. Initial shaping, work budgets, invalidation,
and eviction remain with consumers. It is not yet wired into Mosaic's PDF layout pipeline.

The API is pre-alpha. External consumers should pin an exact version or Git revision. This
standalone crate supports Rust 1.85; the rest of Mosaic may require a newer compiler.
