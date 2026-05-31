# mos-core

Core document model, IDs, source spans, diagnostics, and shared error types for Mosaic.

This is the lowest Mosaic crate. Every compiler phase may depend on it; it must not depend on
parsing, evaluation, layout, fonts, CLI, or backends.

## Purpose

- Provide the lowered semantic `Document` graph used after parsing/evaluation.
- Define stable-ish IDs and typed node/attribute structures shared by later phases.
- Carry source locations and diagnostics in a backend-agnostic shape.
- Offer a small `CoreError` / `Result` convenience layer for crates that need one.

## Public Model

`Document` owns a node arena keyed by `NodeId`. `Document::new(file)` eagerly creates the root
`NodeKind::Document` at `NodeId(0)`. Lowering code adds nodes with `alloc` or `alloc_child`; unknown
parents in `alloc_child` panic intentionally because detached nodes are compiler bugs, not user
input.

Main public types:

- `NodeId`, `ContentHash`, `StyleId`: small opaque identifiers/newtypes.
- `NodeKind`: semantic node kinds known to Mosaic, including current shipped shapes such as
  documents, sections, paragraphs, inline text/emphasis/strong, lists/list items, references,
  images, and figures. Some enum variants exist for planned domains but do not imply implementation.
- `Node`: semantic node with `id`, `kind`, `SourceSpan`, `content_hash`, `style_id`, child IDs, and
  attributes.
- `AttrMap` / `AttrValue`: string-keyed semantic attributes. Values currently cover booleans,
  integers, floats, strings, lists, point lengths, and shared byte buffers for decoded image data.
- `SourceSpan`: byte range in a source file.
- `Document::{get,get_mut,nodes,len,is_empty}`: read/update/traverse the arena.

Example:

```rust
use std::path::PathBuf;

use mos_core::{AttrMap, ContentHash, Document, Node, NodeId, NodeKind, SourceSpan, StyleId};

let file = PathBuf::from("main.mos");
let mut doc = Document::new(file.clone());

let para = doc.alloc_child(doc.root, Node {
    id: NodeId::default(),
    kind: NodeKind::Paragraph,
    span: SourceSpan::placeholder(file),
    content_hash: ContentHash::default(),
    style_id: StyleId::default(),
    children: Vec::new(),
    attributes: AttrMap::new(),
});

assert_eq!(doc.get(doc.root).map(|node| node.children.as_slice()), Some(&[para][..]));
```

## Diagnostics And Errors

`Diagnostic` is the user-facing reporting type. It stores:

- `Severity`: error, warning, or notice.
- `DiagnosticCode`: stable opaque code such as `MOS0033`.
- message, optional `SourceSpan`, and `DiagnosticAnnotation` submessages.

`Diagnostic::simple(def, None, message)` builds from a registered diagnostic definition; `with_span`
attaches source location. `linecol(src, byte_offset)` converts byte offsets to 1-based line/column
pairs, counting Unicode scalar values and clamping invalid or out-of-range offsets safely.

`CoreError` is intentionally small:

- `Unimplemented(&'static str)` for explicit stubs.
- `Diagnostic(Box<Diagnostic>)` for user-facing compiler errors.

Bad documents should flow through diagnostics/errors. Panics are reserved for internal invariant
violations such as linking a node to an unknown parent.

## Crate Boundaries

Allowed here:

- semantic document data structures;
- source spans and diagnostic data;
- minimal shared error types;
- domain-neutral helpers such as `linecol`.

Keep out:

- parser/CST behavior and directive parsing;
- lowering, label resolution, metadata policy, image decoding;
- layout, pagination, font metrics, shaping;
- PDF/HTML/EPUB emission;
- CLI output formatting, exit codes, project manifest semantics, cache persistence.

Current downstream flow is roughly:

```text
mos-parse -> mos-eval -> mos-core::Document -> mos-layout -> mos-pdf
```

`mos-core` supplies the shared model; those crates own phase-specific behavior.

## Known Non-Goals

- Hash-derived stable `NodeId`s are not implemented; current IDs are monotonic allocation order.
- `ContentHash` and `StyleId` are carried but not a persistent incremental cache contract.
- Presence of node kinds such as math, tables, theorems, footnotes, bibliography, or raw nodes does
  not mean those language features are shipped. Citations currently have only minimal placeholder
  support; bibliography resolution/rendering is not shipped.
- No backend-specific attributes should be introduced unless all consumers can tolerate them.
- No file IO, package registry, watcher, formatter, LSP behavior, or build orchestration lives here.
