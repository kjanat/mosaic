# Page-reference and fixpoint boundary

Status: decided 2026-05-25. Tracks GitHub issue #54.

## Why this exists

`manifest-tracker.md` lists both "Add page-reference support" (under Semantic
Model And Resolver) and the entire "Page Reflow And Fixpoints" section as
unchecked. The resolver in `crates/mos-eval/src/resolve.rs` already wraps its
single rewrite pass in a `MAX_FIXPOINT_ITERATIONS` loop. That scaffold makes it
easy to accidentally treat MVP 1 reference work as the seed of a real
pagination fixpoint. This note draws the line so MVP 1 stays a single,
layout-free resolver pass.

## Decision

MVP 1 ships a one-pass semantic resolver. No page numbers, no layout feedback
into resolution, no page graph. Page references and layout-dependent
stabilization belong to the later "Page Reflow And Fixpoints" milestone and are
not in scope for MVP 1.

## In scope for MVP 1

- `@label` references resolve against a `label → NodeId` index built from the
  lowered `Document`.
- Reference text is the target's resolved `number` attribute (sections today;
  figure/equation/table/theorem numbering when those kinds land — still
  layout-independent).
- Diagnostics: `E041` duplicate label, `E042` unknown reference.
- Reference text is `text`-only. The resolver never reads layout output.

## Explicitly out of scope for MVP 1

The following are deferred. Do not add scaffolding for them in MVP 1 PRs:

- Page references (`@label` resolving to a page number, or any new
  page-reference syntax).
- Table of contents with page numbers, list of figures/tables with page
  numbers, index locators.
- Layout fixpoints — iterating layout until pagination is stable.
- Oscillation detection, stabilization iteration counts, non-convergence
  diagnostics.
- Page graph as a first-class output of layout, page boundary signatures, page
  graph reuse, reflow-from-first-changed-page.
- Cross-reference text whose value depends on the target's final page.

The existing `MAX_FIXPOINT_ITERATIONS` loop in `resolve.rs` is a safety net,
not an implementation of any of the above. It will be replaced — not extended
in place — when the real fixpoint lands.

## Minimum future data contract

When page references arrive (post-MVP 1), the resolver/layout boundary should
be the smallest interface that supports a fixpoint without leaking layout
state into semantics. The shape below is a target, not a commitment to any
particular type names.

Resolver → layout, per build:

- A list of nodes that need a page number once layout runs. Each entry carries
  the target `NodeId` (the labelled block, not the reference) and the
  reference's own `NodeId` so the resolver can rewrite it later.

Layout → resolver, after each pagination attempt:

- A `NodeId → PageNumber` map covering at minimum every requested target, plus
  any node referenced by TOC-like constructs.
- A `LayoutSignature` — an opaque hash of page boundaries — so the driver can
  tell whether the new pagination matches the previous iteration.

Driver loop (owned by `mos-eval` or a new orchestration layer, not by
`mos-layout`):

1. Resolve text-only references.
2. Run layout with the current `NodeId → PageNumber` estimate (empty on first
   pass).
3. Rewrite page-dependent references from the new map.
4. Re-run layout. If `LayoutSignature` matches the previous iteration, stop.
5. Cap iterations; emit a non-convergence diagnostic on overflow and fall back
   to the last attempt's numbers.

Implications this contract locks in now:

- `NodeId` must be stable across a single build's layout iterations. It
  already is — `Document::alloc` hands out monotonic IDs and the lowerer is
  deterministic per source.
- Layout must not store reference text on placed boxes. Reference rewriting
  stays in `mos-eval`; layout publishes page numbers, not strings.
- The resolver owns the iteration count and any diagnostics about
  non-convergence, because it is the only stage that knows which references
  are layout-dependent.

Nothing in this contract is implemented in MVP 1.

## Syntax reservation

**Decision: do not reserve page-reference syntax now.** Leave it undefined.

Reasons:

- Current `@label` accepts colon-prefixed identifiers as opaque label text
  (`@sec:intro`, `@fig:ctpa`). Treating any prefix as a "page reference" kind
  today would change current diagnostics and bake a convention into the
  language before the feature exists.
- The natural design space (e.g., a separate sigil, a kind-aware modifier such
  as `@page:label`, or a function-style `#pageref(label)`) is not constrained
  by any MVP 1 commitment. Picking one now would be guesswork; picking one
  later costs nothing because no existing source uses it.
- The resolver's reference node already carries the raw label string and a
  `NodeKind::Reference`. Adding a future "kind" attribute (`section`,
  `figure`, `page`, ...) does not require a new parser token.

A follow-up issue should decide the surface syntax at the same time as the
fixpoint implementation, so the choice can be evaluated against the actual
data contract above instead of in isolation.

## Tracker interpretation

`manifest-tracker.md` continues to list these as unchecked, and that is
correct:

- Semantic Model And Resolver → "Add page-reference support."
- Semantic Model And Resolver → "Add internal fixpoint loop for
  layout-dependent values."
- Diagnostics → "Add non-convergence diagnostics for future fixpoint layout."
- The entire "Page Reflow And Fixpoints" section.

The presence of `MAX_FIXPOINT_ITERATIONS` in `resolve.rs` does not satisfy any
of these checkboxes. It is a single-pass loop with a guard; the real
implementation requires layout participation as described above.

## Concrete follow-up work

Small, scoped issues only — no umbrella epics:

1. When kind-aware reference text lands (figures/equations/tables/theorems),
   reuse the existing one-pass resolver. Do not introduce layout coupling.
2. When page references are picked up, open a single issue that (a) picks
   syntax, (b) introduces the `NodeId → PageNumber` map and `LayoutSignature`
   types in `mos-core`, and (c) moves the iteration driver out of
   `resolve::resolve`. Land syntax, contract, and driver together; do not
   merge a partial scaffold.
3. Non-convergence diagnostics are part of (2), not a separate prerequisite.
