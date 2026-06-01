# MOS CORE KNOWLEDGE BASE

## OVERVIEW

`mos-core` is the lowest Mosaic layer: document graph, node IDs, attrs, spans, diagnostics, and
shared errors. Higher crates depend on it; it depends on none of them.

## WHERE TO LOOK

| Task             | Location        | Notes                                           |
| ---------------- | --------------- | ----------------------------------------------- |
| Document graph   | `Document`      | Allocation, parent/child links, traversal.      |
| Semantic nodes   | `NodeKind`      | Includes future variants; not support map.      |
| Attributes       | `AttrValue`     | Flexible semantic contract.                     |
| Diagnostics      | `Diagnostic`    | Source spans, severity, and `Suggestion` fixes. |
| User errors      | `CoreError`     | Shared result path.                             |
| Source positions | `linecol` tests | Unicode byte offset behavior.                   |

## CONVENTIONS

- Keep this crate dependency-minimal and backend-neutral.
- Model only shared semantic concepts here; local backend details stay outside.
- Preserve byte-span expectations for diagnostics.
- Internal invariant panics may exist; malformed user documents must become diagnostics elsewhere.
- `NodeKind` presence does not mean parser/eval/layout/PDF support exists.

## ANTI-PATTERNS

- Do not depend on parse/eval/layout/PDF/CLI/package/cache crates.
- Do not add PDF-only or layout-only attrs without a cross-crate contract.
- Do not treat aspirational enum variants as shipped feature proof.
- Do not put file IO, manifest loading, or compiler orchestration here.
