# mos-eval

Lowering and resolution for Mosaic documents.

Despite the package description, this crate is not a scripting engine yet. Today it turns
`mos-parse` syntax into a typed `mos-core::Document`, captures semantic attributes, and resolves the
cross-references currently supported by `mos check` / `mos build`.

## Purpose

- Consume a `mos_parse::SyntaxTree` and produce `LowerResult`.
- Build `mos_core` nodes for headings, paragraphs, inline spans, raw/code blocks, lists, images,
  figures, and `#set` directives.
- Preserve source spans and emit user-facing diagnostics instead of panicking on bad documents.
- Run reference resolution through `resolve`, assigning section numbers and rewriting `@label` text.
- Capture document metadata from `#set document(...)` for downstream emitters.

## Public API

- `lower(src, file)`: parse, lower, resolve, and concatenate diagnostics.
- `Evaluator::evaluate(tree)`: lower a parsed tree only; does not run `resolve`.
- `resolve(document)`: mutate a lowered document in place with section numbers and reference text.
- `LowerResult`: semantic `Document`, diagnostics, and `DocumentMetadata`.

## Lowering Behavior

- Headings become `NodeKind::Section` with `level`, optional `label`, and inline children.
- Paragraphs become `NodeKind::Paragraph` with optional `label` and inline children.
- Inline text/emphasis/strong/code/reference/citation become `Text`, `Emphasis`, `Strong`, `Raw`,
  `Reference`, and `Citation` nodes.
- References start with visible placeholder text like `?intro?`; resolution overwrites it when the
  label exists.
- Citations start with visible placeholder text like `[?smith2024?]`; bibliography loading and
  citation resolution are later work.
- Lists become `List` nodes with an `ordered` boolean and nested `ListItem` children.
- Raw pre/code blocks become `Raw` nodes with `text`, optional `label`, and `raw.kind` attributes.
- `#set` directives become `Raw` nodes tagged with `set` and `set.arg.*` attributes.

Supported `#set` targets are `page`, `text`, `document`, and `image`. Values are coerced at this
boundary: lengths are normalized to points, bare numbers in length slots mean points, and `em` uses
the current text size. `#set text(size: ...)` updates the later `em` base. Very small margins, text
sizes, or leading values produce warnings, not hard errors.

## Resolution Behavior

- Sections receive hierarchical `number` attributes such as `1`, `1.1`, and `2`.
- Any non-reference node with a `label` attribute can be a reference target.
- Duplicate labels emit `MOS0030`; the first declaration wins.
- Unknown references emit `MOS0033`; placeholder text remains visible in output.
- The resolver has a small fixpoint-shaped loop, but current numbering does not depend on layout or
  page positions.

## Image And Figure Behavior

- `#image("scan.png")` accepts a positional path; named `src:` and `path:` are also supported.
- `#image` accepts `alt`, `width`, `height`, and `label`.
- `#figure(image: "scan.png", caption: "...")` creates a `Figure` containing an `Image` and optional
  caption paragraph.
- `#figure("scan.png")` is accepted as captionless shorthand.
- Image paths resolve relative to the source `.mos` file unless absolute.
- PNG/JPEG bytes are decoded immediately using the `image` crate.
- Decoded pixels are stored on the image node as RGB8 bytes with `pixel_width`, `pixel_height`,
  `color_space = "DeviceRGB"`, and `bits_per_component = 8`.
- Alpha is composited onto white; no soft-mask data is stored here.
- Missing path emits `MOS0037`; unreadable files emit `MOS0012`; undecodable files emit `MOS0029`.
- Failed image loads do not allocate phantom `Image` or caption-only `Figure` nodes.

## Module Layout

- `lib.rs`: public API, top-level item dispatch, raw-block lowering.
- `inline.rs`: inline node lowering.
- `list.rs`: list and nested list lowering.
- `set.rs`: `#set` lowering, value coercion, metadata capture, sanity warnings.
- `set_schema.rs`: accepted `#set` targets and argument types.
- `image.rs`: image path resolution, file read, PNG/JPEG decode, alpha compositing.
- `image_lower.rs`: `#image` / `#figure` argument handling and semantic node creation.
- `resolve.rs`: section numbering, label index, duplicate/unknown reference diagnostics.

## Boundaries

- Depends on `mos-core` and `mos-parse`; it should not depend on layout, PDF, HTML, packages, cache,
  or CLI crates.
- Owns semantic validation that needs parsed values and source spans.
- Does not make page-layout decisions, measure text, shape fonts, emit PDF/HTML, or read
  `mosaic.toml` project semantics.
- Keeps backend-neutral attributes in the semantic graph; backend-only behavior belongs downstream.

## Examples

```rust
use std::path::Path;

let result = mos_eval::lower("= Intro <intro>\n\nSee @intro.\n", Path::new("main.mos"));

assert!(!result.has_errors());
```

```mos
#set document(title: "Demo", author: "Mosaic")
#set text(size: 12pt)

= Intro <intro>

See @intro.

#image("assets/scan.png", width: 20em, alt: "Scanned page")
#figure(image: "assets/plot.jpg", caption: "Measured values.", label: "plot")
```

## Known Non-Goals

- No user functions, `#let`, templates, or scripting runtime.
- No bibliography resolution/rendering, citation clusters, math/equation semantics,
  theorem/footnote/index/glossary handling.
- No figure/equation numbering beyond generic label lookup.
- No page references, TOC resolution, or layout-dependent fixpoint.
- No package resolution, registry access, persistent cache, or project-output semantics.
- No image defaults application from `#set image(...)` to later bare images yet.
- No formats beyond PNG/JPEG decode in this crate.
