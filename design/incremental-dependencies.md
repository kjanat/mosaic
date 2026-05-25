# Incremental dependency IDs and hash boundaries

Status: design note. No persistent `.mos-cache/` is implemented in this slice. The cache
implementation itself is out of scope; this document defines the typed identifiers and the hash
boundaries that a future incremental engine will key off.

Scope: tracks issue #48 (`MVP 5 — Incremental builds`,
`manifest-tracker.md` → _Incremental Builds And Cache_).

## 1. Why this note exists now

`mos-cache` already exposes `CacheKey(ContentHash)` and an `InMemoryCache`, but nothing in the
pipeline computes meaningful hashes or declares dependencies. `Node::content_hash` is always
`ContentHash::default()` today (see `crates/mos-eval/src/lib.rs` and the layout test fixtures), and
the dependency graph from manifest §7 is deferred.

Before paragraph-, figure-, or reference-level invalidation can ship we need a shared vocabulary:

- _what kinds of things_ can be depended on,
- _what bytes_ feed each hash,
- _what must never_ feed any hash if rebuilds are to stay deterministic.

This note fixes that vocabulary. It does not add new public APIs to the workspace; type sketches
below are illustrative and live in this document until a concrete crate slice needs them.

## 2. Truth ground today

Already in code:

- `mos_core::NodeId(u64)` — monotonic per-`Document`, allocated by `Document::alloc`. Not yet
  derived from a hash of `(file, syntactic position, label, local structure)` as manifest §5.1
  wants.
- `mos_core::ContentHash(u128)` — opaque, defaulted everywhere. Carried as `Node.content_hash`.
- `mos_core::StyleId(u32)` — defaulted everywhere; no resolved style bundle exists yet.
- `mos_cache::CacheKey(ContentHash)` + `Cache` trait + `InMemoryCache` — byte-payload key/value
  store. No schema, no eviction, no disk.
- `mos-eval::LowerResult { document, diagnostics, metadata }` — the semantic surface that a future
  cache would key on for "did anything semantic change?".
- `mos-layout::LayoutEngine::layout(&Document) -> LayoutResult` — currently recomputes everything
  from scratch every call. `layout_incremental` from manifest §31 does not exist yet.

Future work this note assumes will land later:

- Stable hash-derived `NodeId`s.
- Real population of `Node.content_hash`.
- A `DepNode { id, kind, inputs, output_hash }` graph (manifest §7) and the dirtying logic.
- A `ParagraphCacheKey` (manifest §32) and the layout-side reuse path.
- Persistent `.mos-cache/` and the on-disk schema.

None of those are implemented here. This note defines the boundaries they will share.

## 3. Dependency ID categories

A future `DepId` is a tagged union over the categories below. Each category has a stable identity
scheme (so two builds of the same input produce the same `DepId`) and a clearly-bounded hash input
set (so two `DepId`s with the same `output_hash` truly represent interchangeable artifacts).

Illustrative sketch — not added to any crate in this slice:

```text
DepId ::=
    | Source(SourceId)            // a .mos file or other text input
    | Asset(AssetId)              // a binary asset (image, font file)
    | Package(PackageId)          // a resolved package version (future MVP 4)
    | Node(NodeId)                // a semantic node in the Document arena
    | Style(StyleId)              // a resolved style bundle
    | Reference(RefKey)           // a resolved label → target binding
    | LayoutInput(LayoutKey)      // paragraph/figure/heading layout request
    | LayoutOutput(LayoutOutKey)  // shaped paragraph, figure box, page
    | Artifact(ArtifactKey)       // emitted PDF/HTML/EPUB chunk
```

### 3.1 `Source`

Identity: project-relative path, NFC-normalized, forward-slashed. Identity is path-shaped, not
content-shaped, so reading the same file twice gives the same `SourceId`. Content lives in the
hash (§4.1), not the ID.

### 3.2 `Asset`

Identity: project-relative path of the asset as referenced (`#image("figures/x.png")`), plus the
resolution rule used (project root vs. package). Decoding/transcoding output (e.g. RGB8 pixel
buffer) is _not_ part of the asset's identity — it is part of an `Artifact` derived from the
asset.

### 3.3 `Package`

Identity: registry-qualified name + resolved version + manifest hash. Deferred to MVP 4. Listed
here so the dependency graph can already model "this paragraph used `physics@0.4`" without a later
schema break.

### 3.4 `Node`

Identity: a `NodeId` allocated by the lowerer. Today these are monotonic; the migration target
(manifest §5.1) derives them from `hash(file_path, syntactic_position, explicit_label,
local_structure)`. The `DepId::Node` variant survives that migration unchanged — only the
allocator behind `NodeId` changes.

`Node` IDs cover semantic structure (paragraphs, headings, figures, list items, references). They
do _not_ cover layout output; that is `LayoutOutput`.

### 3.5 `Style`

Identity: a `StyleId` pointing at a resolved style bundle (page geometry, text style, list rules,
etc.). Today `StyleId` is defaulted; the future resolver produces a small interned set of bundles
per document.

### 3.6 `Reference`

Identity: `(scope, label_kind, label_name)`, e.g. `(Document, Figure, "fig:loss")`. Resolution
state (the target `NodeId`, number, page) lives in the hash, not the ID, so a reference that
re-resolves to the same target produces the same `output_hash`.

### 3.7 `LayoutInput` / `LayoutOutput`

Inputs and outputs are different `DepId`s on purpose:

- `LayoutInput` is "we asked layout to typeset paragraph N inside a region of width W with style
  S using font set F"; its hash is the manifest §32 cache key.
- `LayoutOutput` is "the resulting lines, height, baselines"; its hash is the digest of those
  bytes.

Separating the two lets the engine reuse a `LayoutOutput` across documents that happen to produce
the same `LayoutInput` hash, without conflating "we ran layout for node N" with "this is the
shape".

### 3.8 `Artifact`

Identity: `(output_kind, range)`, e.g. `(Pdf, Page(14..=16))`. Backends key their persisted output
here. Today only the PDF backend exists and it writes a single file; the variant exists so the
graph stays sound when HTML/EPUB land.

## 4. Hash boundaries

Each category has exactly one hash function. The point of writing them down is to nail the
_input set_: what bytes go in, what must not. Anything outside the input set is invisible to the
cache and must therefore not change the output.

Notation: `H(...)` means "deterministic 128-bit hash of the canonical encoding of these fields".
The concrete hasher is BLAKE3 or SipHash-2-4 keyed by an engine-version constant; the choice is
deferred. Whatever is picked, it MUST be portable and version-stamped.

### 4.1 Source hash

```text
SourceHash = H(
    engine_version,
    source_kind,                  // .mos / .toml / .bib / ...
    file_bytes_after_nfc          // NFC-normalized UTF-8
)
```

Out of inputs (must not affect the hash): mtime, inode, absolute path, byte order mark variations
that NFC collapses, trailing newline that the parser normalizes.

### 4.2 Semantic node hash

```text
NodeHash(node) = H(
    engine_version,
    node.kind,
    canonical_attrs(node.attributes),
    [NodeHash(child) for child in node.children],
    span_kind_only(node.span)     // file role, not byte offsets
)
```

Notes:

- Span byte offsets are _excluded_. A paragraph that shifts down because an earlier paragraph grew
  must hash identically — that is the whole point of the boundary.
- `canonical_attrs` sorts by key (already a `BTreeMap`), normalizes float NaNs, and hashes
  `AttrValue::Bytes` by their content digest, not their `Arc` identity.
- Child hashes feed in as a flat list, not a Merkle root, so adding/removing a sibling re-hashes
  the parent but does not re-hash unaffected siblings.

### 4.3 Asset hash

```text
AssetHash(asset) = H(
    engine_version,
    asset_kind,                   // png/jpeg/font/...
    asset_bytes
)
```

Decoding parameters (e.g. requested DPI, target colorspace) are _not_ part of the asset hash; they
belong to whatever `LayoutInput` consumes the asset. This keeps one PNG ↔ one `AssetHash` even
when many figures reference it at different sizes.

### 4.4 Style / layout input hash

Paragraph (mirrors manifest §32):

```text
ParagraphInputHash = H(
    engine_version,
    NodeHash(paragraph),
    StyleHash(style),
    width_pt_quantized,           // see §6
    FontSetHash(font_set),
    language_tag                  // None until MVP 2; reserve the slot
)
```

Figure:

```text
FigureInputHash = H(
    engine_version,
    NodeHash(figure),
    AssetHash(image),
    StyleHash(figure_style),
    available_width_pt_quantized,
    caption_input_hash            // a nested ParagraphInputHash
)
```

Reference:

```text
ReferenceInputHash = H(
    engine_version,
    ref_kind,                     // section / figure / equation / ...
    target_node_id_stable,
    target_number,                // e.g. "3.2"
    target_page_or_none           // None until page-refs ship
)
```

### 4.5 Layout / page output hash

```text
PageOutputHash = H(
    engine_version,
    [LayoutOutputHash(box) for box on page],
    page_boundary_signature       // manifest §33 input boundary
)
```

The page boundary signature is the small struct that manifest §33's reflow algorithm reads to
decide whether downstream pages are reusable. It is hashed, not stored verbatim, so cache lookups
stay cheap.

## 5. Determinism expectations

These are the rules a future implementation must follow for the boundaries above to be sound.

1. Hashes MUST NOT depend on:
   - wall-clock time, locale, time zone, the user's `$HOME` or `$USER`,
   - absolute filesystem paths, mtimes, inodes, case-folding done by the OS,
   - environment variables other than ones the engine explicitly declares as build inputs,
   - hash-map iteration order, pointer addresses, or `Arc` identity,
   - parse-order-assigned `NodeId`s once `NodeId` is hash-derived (manifest §5.1).
2. Hashes MUST depend on a stamped `engine_version`. Bumping the engine invalidates everything.
3. Text inputs MUST be NFC-normalized before hashing; the layout engine already normalizes its
   text inputs to NFC (`manifest-tracker.md` → Layout), and the source hash matches that.
4. Floating-point inputs (widths, leading, sizes) MUST be quantized to the same fixed-point
   resolution used by layout before hashing. See §6.
5. `Node.content_hash` is _not_ identity. Two nodes with the same `content_hash` are
   interchangeable as layout inputs but are still distinct `NodeId`s.
6. The cache is a hint, not a source of truth. A cold cache and a warm cache MUST produce
   byte-identical output artifacts. This is the property reproducible builds (manifest §22.1,
   `manifest-tracker.md` → Reproducible Builds) will lean on.

Anything that violates rules 1–4 is a determinism bug, not a cache bug.

## 6. Quantization of layout dimensions

Layout currently uses `f32` points throughout (`A4_WIDTH_PT`, `MARGIN_PT`, `TextStyle.size_pt`).
Float bit patterns are unsuitable as hash inputs: two builds that arrive at the same width through
slightly different arithmetic must hash equally.

Rule: every layout dimension that feeds a hash is first converted to an integer count of 1/64 pt
(a "shaper unit" — same granularity HarfBuzz uses internally), saturating-cast to `i32`. The
engine version stamps the granularity so it can be tightened later without silently invalidating
caches.

This applies to `width_pt`, `available_width_pt`, `margin_pt`, `size_pt`, and any future
layout-input length. It does _not_ apply to `f32`s that only live inside an output box (e.g. the
exact baseline of a glyph); those go through whichever encoding `LayoutOutputHash` uses.

## 7. How this supports later invalidation

The boundaries above give the future engine enough structure to do the work `manifest-tracker.md`
describes under _Layout_ and _Page Reflow And Fixpoints_:

- **Reuse clean semantic nodes.** Compare `NodeHash` before vs. after a parse. Unchanged hashes →
  the `Node` is clean and its downstream `LayoutInput`/`LayoutOutput` entries stay valid.
- **Recompute only affected paragraphs.** A paragraph layout entry is keyed by
  `ParagraphInputHash`. A change confined to that paragraph changes only its `NodeHash`, hence its
  `ParagraphInputHash`, hence its line set. Surrounding paragraphs hit the cache.
- **Reflow only affected pages.** Page reflow consumes page-boundary signatures (manifest §33).
  `PageOutputHash` is the cache key; downstream pages whose input boundary matches an old
  `PageOutputHash` are reused wholesale.
- **Update only affected references.** A reference's `ReferenceInputHash` changes iff its target
  node, target number, or target page changes. Re-resolution stays a local edit on the dependency
  graph.
- **Figure invalidation.** Changing the image bytes flips `AssetHash`, which flips
  `FigureInputHash`, which flips the figure box's `LayoutOutputHash`. Changing only the caption
  text flips `NodeHash` on the caption, which flips the nested `caption_input_hash` field of
  `FigureInputHash`, which flips the figure box — without touching the image asset's hash. The
  list-of-figures entry depends on the figure's resolved number/page, so it follows the reference
  pathway above.
- **Report what changed.** Because every artifact has a typed `DepId` and an `output_hash`, the
  engine can diff hashes and produce the "Reused 842/917 semantic nodes" output style from
  manifest §8.

## 8. Out of scope for this issue

Explicitly _not_ designed here:

- The disk layout of `.mos-cache/`. There is no on-disk schema yet.
- Eviction, locking, GC, cross-process sharing.
- The actual hasher choice and the on-wire encoding for `canonical_attrs`.
- Watch-mode loop and CLI surface (`mos watch`, `mos graph`, `mos profile`).
- Float solver and Knuth-Plass; those produce `LayoutOutput` shapes whose hashing this document
  already permits, but the algorithms themselves are MVP 2+.
- Concrete public Rust types in `mos-cache` or `mos-core`. The sketches above stay in this
  document until a code slice needs them.

## 9. Concrete follow-up issue candidates

These are scope-sized, one-PR work items, not umbrella epics. Each can become its own GitHub
issue when it is ready to start:

1. _Stable `NodeId` derivation._ Replace `Document::alloc`'s monotonic counter with a hash of
   `(source_id, syntactic_position, explicit_label, local_structure)`. Land behind a feature
   flag if needed; the public `NodeId(u64)` type stays.
2. _Populate `Node.content_hash` in the lowerer._ Compute `NodeHash` per §4.2 during
   `mos-eval`'s lowering pass. Cache stays untouched.
3. _Source and asset hashing helpers in `mos-core`._ Provide functions that produce `SourceHash`
   / `AssetHash` per §4.1 and §4.3 with the agreed engine-version stamping. No public `DepId`
   yet.
4. _Layout-dimension quantization helper._ A small `i32`-of-1/64-pt newtype used wherever layout
   currently passes `f32` widths into anything hash-bound. Lives in `mos-layout`.
5. _`DepNode` graph in `mos-cache` (in-memory only)._ Introduce the `DepId`/`DepKind`/`DepNode`
   types from §3 and wire them into the in-memory cache key. No persistence.
6. _Paragraph layout cache (in-memory)._ Add `ParagraphCacheKey` and store one `LayoutOutput` per
   key in `InMemoryCache`. Measure reuse on the existing examples.
7. _Page boundary signature._ Introduce the small struct that manifest §33 reflow consumes, hash
   it, and start emitting `PageOutputHash`.
8. _Persistent `.mos-cache/`._ Only after items 1–7 land. Schema and disk layout get their own
   design note.

Items 1–4 are pure refactors with no behavior change; items 5–7 add an opt-in cache layer; item 8
is a separate, larger slice.
