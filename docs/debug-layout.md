# Inspecting layout geometry

`mos build --debug-layout` writes a JSON report beside each generated PDF:

```sh
mos build --debug-layout main.mos
# build/main.pdf and build/main.layout.json

mos build --debug-layout one two
# Each project's configured PDF output and its sibling .layout.json report
```

The PDF path follows the normal build rules, including `[output].pdf` for projects. Replacing its
extension with `.layout.json` gives the report path: `out/book.pdf` becomes `out/book.layout.json`.
A report is written after successful PDF emission, before an optional `--open` viewer launch.
Failure to write the report makes the command fail; the PDF may already exist. Compiler errors
produce neither artifact. Existing artifacts from earlier builds are not removed when a later build
fails or omits the flag.

The report captures the final layout used for the PDF, after page-reference resolution. Tracing
preserves PDF bytes. Identical inputs, compiler version, and source paths produce identical report
bytes, including deterministic array ordering and a trailing newline. Reports contain no timestamps;
relocating a document can change source paths, and edits can change node IDs.

## Format version 1

The root contains `schema_version: 1`, `units: "pt"`, `origin: "top-left"`, and a `pages` array. One
point is 1/72 inch. All coordinates measure rightward and downward from the page's top-left corner,
including baselines. Rectangles contain `x_pt`, `y_pt`, `width_pt`, and `height_pt`.

Each page has these fields:

| Field            | Meaning                                                                |
| ---------------- | ---------------------------------------------------------------------- |
| `number`         | One-based page number, in emitted page order.                          |
| `bounds`         | Full paper rectangle.                                                  |
| `content_bounds` | Rectangle inside the resolved symmetric page margins.                  |
| `blocks`         | Per-page bounds of source blocks, sorted by semantic node ID.          |
| `lines`          | Committed text lines in placement order, with baselines and font runs. |
| `images`         | Final image placements in placement order.                             |

A block contains `source` and `bounds`. Bounds enclose all text and images emitted by that block and
its children on that page. A figure includes its image and caption; lists include their markers and
nested items. Blocks spanning pages appear on each page where they emit content. Empty blocks,
trailing paragraph spacing, and blank raw lines do not create rectangles.

Each line contains `source`, `bounds`, `baseline_from_top_pt`, and `runs`. A run contains:

- `run_index`: zero-based index into that page's `Page::runs` array.
- `text`: the final laid-out text, including generated numbering or resolved references.
- `actual_text`: optional extraction text retained by layout, such as original tabs in raw blocks.
- `font` and `size_pt`: the selected PostScript font name and size, including fallback sub-runs.
- `bounds`: shaped advance width and font ascent/descent around the baseline.

Text rectangles describe advance and font metrics. They do not trace glyph ink outlines or
individual shaping offsets. A line rectangle is the union of its runs, including gaps between words
and any list or bibliography marker. It excludes trailing space with no emitted run.

Each image contains `source`, `image_index` (zero-based within `Page::images`), `image_id` (the
shared layout image handle), original `pixel_width` and `pixel_height`, and final `bounds` in
points. Repeated placements can share an image ID. Decoded pixels and image asset paths are omitted.

## Source mapping

`source` describes the semantic block responsible for a placement:

```json
{
	"node_id": 3,
	"kind": "paragraph",
	"file": "main.mos",
	"byte_start": 80,
	"byte_end": 85
}
```

`byte_start` is inclusive and `byte_end` exclusive, in UTF-8 bytes. The file is the compiler's
source path as a display string; non-UTF-8 path bytes display as replacement characters. An optional
`label` carries the block's label. Node IDs identify nodes within this lowered document; they are
not persistent identities across edits. Kinds include `section`, `paragraph`, `image`, `figure`,
`list`, `list_item`, `bibliography`, and `raw`.

Lines and images refer to their innermost active block. Generated content uses the source spans
already supplied by lowering, such as a figure caption's directive span. These are block spans; the
report does not invent per-glyph source locations. A placement without source context has `source:
null`.

## Library API and scope

`LayoutEngine::layout_with_debug(&Document)` returns the usual `LayoutResult` plus `debug:
Some(Report)`. The report types live in `mos_layout::debug` and implement `serde::Serialize`. Layout
performs no file I/O; the CLI serializes the report. Ordinary `LayoutEngine::layout` calls return
`debug: None` and do not collect trace data.

This report covers today's page graph: boxes, baselines, text runs, images, and source blocks. SVG
overlays, float decisions, constraint graphs, dirty-node tracking, and page-break costs remain
future work. The broader debug-backend tracker stays open for those features.
