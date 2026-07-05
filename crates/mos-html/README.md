# mos-html

Semantic HTML backend crate for Mosaic.

Current status: stub. This crate exists to reserve the backend boundary described in `manifest.md`
section 21.2, but it does not implement HTML emission yet. Do not treat it as a shipped HTML
backend.

> [!WARNING]
> While this crate is in the `0.0.x` line, Mosaic treats it as pre-alpha. Breaking changes are
> acceptable between patch releases. If you depend on this crate, pin an exact version such as
> `=0.0.2`, or accept the risk of API breakage.

## API

```rust
pub fn emit(graph: &mos_layout::PageGraph, out: &std::path::Path) -> mos_core::Result<()>;
```

`emit` currently ignores both arguments and returns:

```rust
Err(mos_core::CoreError::Unimplemented("mos-html::emit"))
```

It does not create files, serialize markup, copy assets, or report partial success.

## Boundary

`mos-html` is a backend sink. Its input boundary is `mos_layout::PageGraph`; parsing, lowering,
semantic resolution, layout policy, and CLI orchestration belong elsewhere.

Expected dependency direction:

```text
mos-core -> mos-parse -> mos-eval -> mos-layout -> mos-html
```

The crate should preserve document semantics when implemented, but current code has no HTML tree,
CSS model, asset pipeline, or writer.

## Non-goals For Current Code

- No implemented HTML backend.
- No EPUB/SVG/web export.
- No CLI integration for HTML output.
- No layout-to-CSS mapping.
- No accessibility, heading outline, link, image, or asset handling.
- No incremental cache or watch behavior.

Trust current code and the root README status over `manifest.md` when they disagree. Manifest dreams
loud. Code grunts truth.
