# Page-reference and fixpoint boundary

Status: decided 2026-05-25. Tracks GitHub issue #54.

## Why this exists

`manifest.md` sketches an "internal fixpoint loop" in MVP 1, but current code and
`manifest-tracker.md` are the planning truth here: page references, layout-dependent values, and the
entire "Page Reflow And Fixpoints" section are still unchecked. The resolver in
`crates/mos-eval/src/resolve.rs` already wraps its single rewrite pass in a
`MAX_FIXPOINT_ITERATIONS` loop. That scaffold makes it easy to accidentally treat MVP 1 reference
work as the seed of a real pagination fixpoint. This note draws the line so MVP 1 stays a single,
layout-free resolver pass.

## Decision

MVP 1 ships a one-pass semantic resolver. No page numbers and no layout feedback into resolution.
The existing `mos-layout::PageGraph` remains a backend-facing layout product; it is not fed back
into semantic resolution. Page references and layout-dependent stabilization belong to the later
"Page Reflow And Fixpoints" milestone and are not in scope for MVP 1.

## In scope for MVP 1

- `@label` references resolve against a typed `label → LabelTarget` index built from the lowered
  `Document`.
- Reference text uses a section target's captured hierarchical number and a figure target's captured
  document-order number (rendered kind-aware as `Figure N`). Generic targets render as the bare
  label. See `paragraph_label_indexes_paragraph` and `figure_reference_renders_kind_aware_text` in
  `crates/mos-eval/src/resolve.rs`.
- Diagnostics: `MOS0030` duplicate label, `MOS0033` unknown reference.
- Reference text is `text`-only. The resolver never reads layout output.

## Explicitly out of scope for MVP 1

The following are deferred. Do not add scaffolding for them in MVP 1 PRs:

- Page references (`@label` resolving to a page number, or any new page-reference syntax).
- Table of contents with page numbers, list of figures/tables with page numbers, index locators.
- Layout fixpoints: iterating layout until pagination is stable.
- Oscillation detection, stabilization iteration counts, non-convergence diagnostics.
- Page boundary signatures, page graph reuse, and reflow-from-first-changed-page.
- Cross-reference text whose value depends on the target's final page.

The existing `MAX_FIXPOINT_ITERATIONS` loop in `resolve.rs` is a safety net, not an implementation
of any of the above. It will be replaced, not extended in place, when the real fixpoint lands.

## Minimum future data contract

When page references arrive (post-MVP 1), the resolver/layout boundary should be the smallest
interface that supports a fixpoint without leaking layout state into semantics. The shape below is a
target, not a commitment to any particular type names.

Resolver → layout, per build:

- A list of nodes that need a page number once layout runs. Each entry carries the target `NodeId`
  (the labelled block, not the reference) and the reference's own `NodeId` so the resolver can
  rewrite it later.

Layout → resolver, after each pagination attempt:

- A `NodeId → PageNumber` map covering at minimum every requested target, plus any node referenced
  by TOC-like constructs.
- A `LayoutSignature`: an opaque hash of page boundaries, so the driver can tell whether the new
  pagination matches the previous iteration.

Driver loop ownership: **above** both `mos-eval` and `mos-layout`. The compiler pipeline order is
parse → eval → layout, and `mos` orchestrates that pipeline. `mos-eval` cannot own the loop because
it sits upstream of `mos-layout` and must not depend on it; `mos-layout` cannot own the loop because
it must not call back into semantic resolution. The driver therefore lives in `mos` (today's
orchestrator) or in a dedicated orchestration crate introduced when the fixpoint lands. `mos-eval`
exposes the rewrite step as a function the driver can call repeatedly; `mos-layout` exposes the
page-number/signature output. Neither crate gains a dependency on the other.

1. Call `mos-eval` to resolve text-only references.
2. Call `mos-layout` with the current `NodeId → PageNumber` estimate (empty on first pass).
3. Call `mos-eval` again to rewrite page-dependent references from the new map.
4. Re-run `mos-layout`. If `LayoutSignature` matches the previous iteration, stop.
5. Cap iterations; emit a non-convergence diagnostic on overflow and fall back to the last attempt's
   numbers.

Implications this contract locks in now:

- `NodeId` must be stable across a single build's layout iterations. It already is:
  `Document::alloc` hands out monotonic IDs and the lowerer is deterministic per source.
- Layout may store rendered text in placed runs for measurement and output, but it must not own the
  canonical reference rewrite. Reference string decisions stay in `mos-eval`; layout publishes page
  numbers and signatures, not replacement strings.
- The driver owns the iteration count and non-convergence diagnostics. The resolver tags which
  references are layout-dependent (because only it knows), but it does not run the loop.

Nothing in this contract is implemented in MVP 1.

## Syntax reservation

**Decision: do not reserve page-reference syntax now, and explicitly flag the prefix-based design
space as a breaking-change risk.** Leave it undefined.

The current parser (`crates/mos-parse/src/inline.rs`) accepts any `scan_label_chars` run after `@`
as opaque label text: colons included. That means `@page:foo`, `@p:foo`, `@pg:foo`, and any other
`prefix:label` form is **already a valid label reference today**. The corollary:

- A future kind-discriminating prefix syntax such as `@page:label` would silently change the meaning
  of any document that already uses `page:` as part of a label name. That is a breaking change, not
  a free extension.
- Therefore the future page-reference issue must either pick syntax that is not a currently-legal
  label (a separate sigil, e.g. `@@label`, `@!label`, or a function form like `#pageref(label)`), or
  pay an explicit migration cost for any chosen prefix and gate the change behind a version bump.
- Picking the syntax now, before the fixpoint contract is concrete, would forfeit the cleaner sigil
  options without buying anything. Picking it later costs nothing as long as we do not bake `page:`
  (or any other plausible prefix) into existing examples in the meantime.

The resolver's reference node already carries the raw label string and a `NodeKind::Reference`.
Adding a future `kind` attribute (`section`, `figure`, `page`, ...) does not require a new parser
token, but it does require the parser to recognise a syntactic form that today is indistinguishable
from a labelled reference.

A follow-up issue decides the surface syntax at the same time as the fixpoint implementation, with
the prefix-collision constraint listed above as a hard input.

## Tracker interpretation

`manifest-tracker.md` continues to list these as unchecked, and that is correct:

- Semantic Model And Resolver → "Add page-reference support."
- Semantic Model And Resolver → "Add internal fixpoint loop for layout-dependent values."
- Diagnostics → "Add non-convergence diagnostics for future fixpoint layout."
- The entire "Page Reflow And Fixpoints" section.

The presence of `MAX_FIXPOINT_ITERATIONS` in `resolve.rs` does not satisfy any of these checkboxes.
It is a single-pass loop with a guard; the real implementation requires layout participation as
described above.

## Concrete follow-up work

Small, scoped issues only; no umbrella epics:

1. When kind-aware reference text lands (figures/equations/tables/theorems), reuse the existing
   one-pass resolver. Do not introduce layout coupling.
2. When page references are picked up, open a single issue that (a) picks syntax under the
   prefix-collision constraint above, (b) introduces the `NodeId → PageNumber` map and
   `LayoutSignature` types in `mos-core`, and (c) places the iteration driver in `mos` (or a new
   orchestration crate); not in `mos-eval` or `mos-layout`. The existing single-pass loop in
   `resolve::resolve` is removed in the same change; it does not survive as a second driver. Land
   syntax, contract, and driver together; do not merge a partial scaffold.
3. Non-convergence diagnostics are part of (2), not a separate prerequisite.
