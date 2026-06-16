# MOSAIC-EVAL KNOWLEDGE BASE

## OVERVIEW

`mos-eval` lowers parsed syntax into `mos-core::Document` and resolves current semantic features. It
is not a general scripting engine yet.

## CURRENT SCOPE

Implemented:

- Headings, paragraphs, inline text/emphasis/strong/code/reference/citation nodes.
- `InlineKind::HardBreak` lowered to `NodeKind::HardBreak` with empty attributes (no `text` payload;
  structural marker only).
- Lists lowered to `List`/`ListItem` nodes.
- `#set` nodes for document/page/text/image settings.
- Document metadata: title, author, language.
- `#image` and `#figure` with PNG/JPEG decode and image attrs.
- `#bibliography("refs.bib")` source nodes, BibTeX file loading via `mos-bib`, citation-key
  resolution, `MOS0045` missing-key diagnostics, and `MOS0046` duplicate-key diagnostics across
  declared bibliography sources.
- Label index, duplicate label diagnostics, unknown reference diagnostics.
- Section numbering; figure numbering with kind-aware `Figure N` references and stamped `Figure N:`
  caption labels; generic reference text rewrite and citation placeholder text.

Not implemented yet:

- User functions, `#let`, scripting, templates, bibliography rendering, citation display numbering,
  citation clusters, math/equation semantics, equation numbering, package resolution, full fixpoint
  over layout/page references.

## WHERE TO LOOK

| Task          | Location                         | Notes                                                                                                                                      |
| ------------- | -------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------ |
| Entry point   | `Evaluator::evaluate`            | Dispatch from syntax items.                                                                                                                |
| Public helper | `lower`                          | Parse result to `LowerResult`.                                                                                                             |
| `#set` schema | `src/set_schema.rs`              | Supported targets and args.                                                                                                                |
| Images        | `src/image.rs` + lowerer helpers | Decode, attrs, path resolution.                                                                                                            |
| Figures       | `lower_figure_directive`         | Image + caption semantic node.                                                                                                             |
| References    | `src/resolve.rs`                 | Label index, section/figure numbering, figure-aware refs.                                                                                  |
| Page refs     | `src/pageref.rs`                 | `@page(label)`: `resolve_page_references` + `resolve_page_reference_fixpoint` (injected layout); undeclared-label MOS0033 in `resolve.rs`. |
| Bibliography  | `src/bibliography.rs`            | Source paths, `.bib` loading, citation-key diagnostics.                                                                                    |
| Unit coercion | length helpers                   | `em` depends on current text size.                                                                                                         |

## CONVENTIONS

- Use `DirectiveKind`; do not route by raw directive name when typed kind exists.
- Keep layout and backend concerns out. Store semantic attrs only.
- Resolve paths relative to the source file/project context used by existing code.
- Image decode failures should prevent phantom figure output.
- `resolve` is public and re-entrant; the fixpoint reruns it and future page-ref passes will too, so
  every pass must be idempotent. Numbering overwrites; caption labelling re-derives `text` from a
  preserved `caption_source` rather than re-reading the already-stamped text (which would nest
  `Figure 1: Figure 1: …`).
- Preserve diagnostics with useful spans; user input errors are not panics.
- Citation key checks run even without declared bibliography sources; a missing key against an empty
  complete record set is `MOS0045`. If any declared bibliography source is missing/unreadable/
  malformed, suppress missing-key diagnostics to avoid false negatives from an incomplete record
  set.
- `current_text_size_pt` affects `em` conversion order. Be careful around `#set text(size: ...)`.

## STATUS WARNINGS

- `NodeKind` has future variants not produced today. Enum presence is not feature support.
- Fixpoint logic is only for current reference needs, not manifest-wide pagination stabilization.
- `mosaic.toml` package/output semantics are not wired through this crate today.

## ANTI-PATTERNS

- Do not add layout measurements here.
- Do not emit PDF/HTML attrs that only one backend understands unless core/layout contract exists.
- Do not implement broad scripting from manifest without a narrow user request.
