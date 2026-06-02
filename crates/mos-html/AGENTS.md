# MOS-HTML KNOWLEDGE BASE

## OVERVIEW

`mos-html` is currently a backend stub. It names the HTML direction but does not ship HTML output.

## WHERE TO LOOK

| Task          | Location     | Notes                                               |
| ------------- | ------------ | --------------------------------------------------- |
| Public API    | `src/lib.rs` | `emit(&PageGraph, &Path)` returns unimplemented.    |
| Crate status  | `README.md`  | Treat code as truth if README and manifesto differ. |
| Backend input | `mos-layout` | HTML consumes `PageGraph`, not source text.         |

## CURRENT SLICE

- `emit` returns `CoreError::Unimplemented("mos-html::emit")`.
- No file writing, asset pipeline, CSS generation, semantic tree builder, or CLI integration.
- The doc comments describe desired semantic HTML, not shipped behavior.

## BOUNDARY RULES

- Backend sink only: consume `mos_layout::PageGraph`.
- No parsing, lowering, resolving, layout policy, project manifest handling, or CLI orchestration.
- When implementation lands, preserve semantic HTML direction instead of PDF-style rectangles.
- Keep unsupported output explicit; silent success is worse than angry cave bear.

## ANTI-PATTERNS

- Do not claim HTML/EPUB/SVG support from this crate existing.
- Do not add filesystem side effects before an actual implementation slice.
- Do not move page layout concerns into HTML emission.
