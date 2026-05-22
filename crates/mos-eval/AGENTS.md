# MOSAIC-EVAL KNOWLEDGE BASE

## OVERVIEW

`mos-eval` lowers parsed syntax into `mos-core::Document` and resolves current semantic features. It
is not a general scripting engine yet.

## CURRENT SCOPE

Implemented:

- Headings, paragraphs, inline text/emphasis/strong/code/reference nodes.
- Lists lowered to `List`/`ListItem` nodes.
- `#set` nodes for document/page/text/image settings.
- Document metadata: title, author, language.
- `#image` and `#figure` with PNG/JPEG decode and image attrs.
- Label index, duplicate label diagnostics, unknown reference diagnostics.
- Section numbering and generic reference text rewrite.

Not implemented yet:

- User functions, `#let`, scripting, templates, bibliography, math/equation semantics,
  figure/equation numbering, package resolution, full fixpoint over layout/page references.

## WHERE TO LOOK

| Task          | Location                         | Notes                                   |
| ------------- | -------------------------------- | --------------------------------------- |
| Entry point   | `Evaluator::evaluate`            | Dispatch from syntax items.             |
| Public helper | `lower`                          | Parse result to `LowerResult`.          |
| `#set` schema | `src/set_schema.rs`              | Supported targets and args.             |
| Images        | `src/image.rs` + lowerer helpers | Decode, attrs, path resolution.         |
| Figures       | `lower_figure_directive`         | Image + caption semantic node.          |
| References    | `src/resolve.rs`                 | Label index and generic reference text. |
| Unit coercion | length helpers                   | `em` depends on current text size.      |

## CONVENTIONS

- Use `DirectiveKind`; do not route by raw directive name when typed kind exists.
- Keep layout and backend concerns out. Store semantic attrs only.
- Resolve paths relative to the source file/project context used by existing code.
- Image decode failures should prevent phantom figure output.
- Preserve diagnostics with useful spans; user input errors are not panics.
- `current_text_size_pt` affects `em` conversion order. Be careful around `#set text(size: ...)`.

## STATUS WARNINGS

- `NodeKind` has future variants not produced today. Enum presence is not feature support.
- Fixpoint logic is only for current reference needs, not manifest-wide pagination stabilization.
- `mosaic.toml` package/output semantics are not wired through this crate today.

## ANTI-PATTERNS

- Do not add layout measurements here.
- Do not emit PDF/HTML attrs that only one backend understands unless core/layout contract exists.
- Do not implement broad scripting from manifest without a narrow user request.
