# MOSAIC-PDF KNOWLEDGE BASE

## OVERVIEW

`mosaic-pdf` consumes `mosaic-layout::PageGraph` and emits PDF. It owns object planning, content
streams, fonts, encodings, embedded subsets, images, and PDF metadata currently supported.

## CURRENT SCOPE

Implemented:

- PDF file emission through `emit`.
- Pages, content streams, text runs, image XObjects.
- Base-14 font resources and custom `/Differences` for extended glyphs.
- Embedded Noto Sans Type0/CID fonts with subsets and `/ToUnicode`.
- PNG/JPEG image emission after decode/layout prepared image data.
- Title/author Info metadata.

Not implemented yet:

- Hyperlinks, bookmarks, tagged PDF, PDF/A, vector graphics, full catalog language metadata, debug
  layout backend, SVG pages.

## WHERE TO LOOK

| Task             | Location                    | Notes                               |
| ---------------- | --------------------------- | ----------------------------------- |
| Public API       | `emit`, `PdfMetadata`       | Filesystem-facing entry.            |
| Object planning  | `build_pdf`                 | Ref allocation and resource setup.  |
| Content stream   | `build_content_stream`      | Text/image draw ordering.           |
| Base-14 encoding | `src/encoding.rs`           | Deterministic differences planning. |
| Embedded fonts   | `src/embedded.rs`           | Type0/CID subset objects.           |
| Images           | `src/images.rs`             | XObject data and stream details.    |
| Tests            | `tests/*.rs` + inline tests | Structural PDF checks via `lopdf`.  |

## CONVENTIONS

- Consume `PageGraph`; do not parse, lower, or layout source.
- Keep output deterministic. Fixed font/resource order matters.
- Prefer structural tests over byte-for-byte PDF snapshots.
- Images are drawn before text in current content stream behavior.
- `PdfMetadata.language` exists but is not fully emitted yet; do not claim language-tagged PDF.

## HOT INVARIANTS

- Resource names and allocation order affect PDF stability.
- Base-14 fallback may become `?` for unsupported codepoints.
- Embedded text needs a planned subset before content stream emission.
- `Symbol`/`ZapfDingbats` differences are intentionally not handled like Latin faces today.

## ANTI-PATTERNS

- Do not add layout policy or line breaking in this crate.
- Do not use hash-map iteration for object/resource order.
- Do not promise manifest PDF features until tests prove them.
