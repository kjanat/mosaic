# Incremental dependency IDs and hash boundaries

Status: design note. No persistent `.mos-cache/` is implemented in this slice. The cache
implementation itself is out of scope; this document defines the typed identifiers and the hash
boundaries that a future incremental engine will key off.

Scope: tracks issue #48 (`MVP 5 — Incremental builds`, `manifest-tracker.md` → *Incremental Builds
And Cache*).

## 1. Why this note exists now

`mos-cache` already exposes `CacheKey(ContentHash)` and an `InMemoryCache`, but nothing in the
pipeline computes meaningful hashes or declares dependencies. `Node::content_hash` is always
`ContentHash::default()` today (see `crates/mos-eval/src/lib.rs` and the layout test fixtures), and
the dependency graph from manifest §7 is deferred.

Before paragraph-, figure-, or reference-level invalidation can ship we need a shared vocabulary:

- *what kinds of things* can be depended on,
- *what bytes* feed each hash,
- *what must never* feed any hash if rebuilds are to stay deterministic.

This note fixes that vocabulary. The identity types for the categories with a real identity today
have landed (`mos_cache::{DependencyId, DependencyKind, ProjectPath}`, see §3); the remaining
categories, the hash boundaries, the dependency graph, and any `CacheKey` wiring are still sketches
that live in this document until a concrete crate slice needs them.

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

#### What has landed: `DependencyId` / `DependencyKind`

The first concrete slice (`crates/mos-cache/src/dependency.rs`) introduces
[`DependencyId`](../crates/mos-cache/src/dependency.rs), `DependencyKind`, `ProjectPath`, and
`ProjectPathError` as real public types. It **deliberately models a subset** of the sketch above —
only the categories that have a *stable identity today*:

| `DependencyKind` | Identity (payload) | Sketch category                                                        |
| ---------------- | ------------------ | ---------------------------------------------------------------------- |
| `SourceFile`     | `ProjectPath`      | `Source`                                                               |
| `Asset`          | `ProjectPath`      | `Asset`                                                                |
| `Bibliography`   | `ProjectPath`      | `Source` (`.bib`) — split out because §4 hashes it on its own boundary |
| `Label`          | reference name     | `Reference` (name only; target binding lives in the hash, §3.6)        |

Naming follows the workspace convention (`NodeId`, `StyleId`, `CacheKey`): the types spell out
`Dependency…` rather than the `Dep…` shorthand the prose uses. The label payload is an inline
`String` under the variant tag — the tag already prevents mixing it with a path, so no wrapper is
needed. File payloads, by contrast, use the checked `ProjectPath` newtype, which *earns* its
wrapper: it canonicalizes the path on construction (fold `\`→`/`, drop `.`/empty segments, resolve
`..`, NFC-normalize per §3.1) so `./a.mos`, `a.mos`, and `a\b\..\b/a.mos` collapse to one identity.
Empty paths, raw absolute paths, drive-prefixed paths, and paths that escape above the project root
are rejected with `ProjectPathError`. Absolute filesystem paths remain valid at outer I/O
boundaries, but they must be made project-relative before they become dependency identities. Without
that, raw paths would hand out distinct ids for the same logical input and the determinism the cache
relies on would be a lie.

Deliberately **not** modelled yet:

- `Package`, `Node`, `Style` — their identities are still defaulted (`NodeId` is monotonic,
  `StyleId` is `0` everywhere), so an id over them would collide unrelated inputs. They graduate
  once they have a real scheme (see §9).
- `LayoutInput` — there is no real layout key until paragraph hashing lands (`ParagraphInputHash`,
  §4.4, folds node + style + width + font set). Keying it on the defaulted `StyleId` alone would
  conflate every layout request under `StyleId(0)`, so it waits for the genuine key.
- `LayoutOutput`, `Artifact` — output-side, not dependencies.

The id/kind types carry no hashing themselves. The first content boundary built on one has since
landed for bibliography inputs — `BibliographyDependency` pairs a `Bibliography` id with a
`ContentHash` (§4.1). None of these are wired into `CacheKey` or a `DepNode` graph yet; that is a
later slice (§9.6).

### 3.1 `Source`

Identity: project-relative path, NFC-normalized, forward-slashed. Identity is path-shaped, not
content-shaped, so reading the same file twice gives the same `SourceId`. Content lives in the hash
(§4.1), not the ID.

### 3.2 `Asset`

Identity: project-relative path of the asset as referenced (`#image("figures/x.png")`), plus the
resolution rule used (project root vs. package). Decoding/transcoding output (e.g. RGB8 pixel
buffer) is *not* part of the asset's identity — it is part of an `Artifact` derived from the asset.

### 3.3 `Package`

Identity: registry-qualified name + resolved version + manifest hash. Deferred to MVP 4. Listed here
so the dependency graph can already model "this paragraph used `physics@0.4`" without a later schema
break.

### 3.4 `Node`

Identity: a `NodeId` allocated through the document arena in `mos-core` but *derived* by the lowerer
in `mos-eval`. Today `Document::alloc` hands out a monotonic counter; the migration target (manifest
§5.1) computes the stable ID from
`hash(source_id, syntactic_position, explicit_label,
local_structure)` inside the lowerer — which is
the only stage that has the parse tree — and passes the precomputed ID into the arena.

This split matters for crate boundaries. `mos-core` must not learn about syntax trees, so a future
`Document::alloc_with_id(id, node)` (or equivalent) keeps the derivation in `mos-eval` without
dragging parser types into core. The public `NodeId(u64)` newtype is unchanged either way; only the
producer rule shifts. The `DepId::Node` variant survives that migration unchanged.

`Node` IDs cover semantic structure (paragraphs, headings, figures, list items, references). They do
*not* cover layout output; that is `LayoutOutput`.

### 3.5 `Style`

Identity: a `StyleId` pointing at a resolved style bundle (page geometry, text style, list rules,
etc.). Today `StyleId` is defaulted; the future resolver produces a small interned set of bundles
per document.

### 3.6 `Reference`

Identity: `(scope, label_kind, label_name)`, e.g. `(Document, Figure, "fig:loss")`. Resolution state
(the target `NodeId`, number, page) lives in the hash, not the ID, so a reference that re-resolves
to the same target produces the same `output_hash`.

### 3.7 `LayoutInput` / `LayoutOutput`

Inputs and outputs are different `DepId`s on purpose:

- `LayoutInput` is "we asked layout to typeset paragraph N inside a region of width W with style S
  using font set F"; its hash is the manifest §32 cache key.
- `LayoutOutput` is "the resulting lines, height, baselines"; its hash is the digest of those bytes.

Separating the two lets the engine reuse a `LayoutOutput` across documents that happen to produce
the same `LayoutInput` hash, without conflating "we ran layout for node N" with "this is the shape".

### 3.8 `Artifact`

Identity: `(output_kind, range)`, e.g. `(Pdf, Page(14..=16))`. Backends key their persisted output
here. Today only the PDF backend exists and it writes a single file; the variant exists so the graph
stays sound when HTML/EPUB land.

## 4. Hash boundaries

Each category has exactly one hash function. The point of writing them down is to nail the *input
set*: what bytes go in, what must not. Anything outside the input set is invisible to the cache and
must therefore not change the output.

Notation: `H(...)` means "deterministic 128-bit hash of the canonical encoding of these fields". The
concrete hasher is deferred: BLAKE3 truncated to 128 bits is acceptable; plain SipHash-2-4 is not,
unless wrapped in an explicitly specified two-lane construction that yields 128 bits. Whatever is
picked, it MUST be portable and version-stamped.

### 4.1 Source hash

```text
SourceHash = H(
    engine_version,
    source_kind,                  // .mos / .toml / .bib / ...
    file_bytes                    // raw bytes as read, no normalization
)
```

Source hashing is intentionally byte-for-byte. The parser does *not* NFC-normalize source today, and
the source hash must match what the parser actually consumed — otherwise the cache would "forget"
cosmetic edits the parser is sensitive to. NFC handling enters the pipeline later at the
layout-input boundary (§4.4), where two paragraphs whose authored text is NFC-equivalent should hit
the same `ParagraphInputHash`.

Out of inputs (must not affect the hash): mtime, inode, absolute path, owning user, line-ending
auto-conversion by the OS or the user's editor. If a future parser normalizes line endings or strips
a BOM before lowering, that normalization must happen *before* the bytes are hashed and be stamped
into `engine_version`.

#### What has landed: bibliography content hash

The first concrete `SourceHash` to ship is the bibliography boundary, because the `Bibliography`
dependency kind is split out from `Source` in §3 expressly so `.bib` inputs hash on their own
boundary. `mos_bib::bibliography_content_hash(&[u8]) -> mos_core::ContentHash` implements exactly
the `SourceHash` shape above, specialized to `source_kind = bibliography`:

```text
BibliographyContentHash = H(
    engine_version,               // CARGO_PKG_VERSION; bumping it invalidates (§5 rule 2)
    domain_tag,                   // "mos-bib/bibliography-source/v1" — separates this boundary
    file_bytes                    // raw bytes as read, byte-for-byte (no NFC / line-ending / BOM)
)
```

It honors the determinism rules: byte-for-byte input (§4.1), `engine_version` stamped (§5 rule 2),
no filesystem-derived data, and a fixed `u64`-width length prefix per field so the hash is identical
on 32- and 64-bit targets. `H` is currently **FNV-1a over 128 bits** — fully specified, portable,
and deterministic, unlike the randomly-seeded `SipHash` §4 rules out. This is an *interim* hasher:
the construction may be replaced with BLAKE3-truncated-to-128 (the note's preference) by the §9.4
source/asset hashing slice without changing the `&[u8] -> ContentHash` signature; the stamped
`engine_version` absorbs the resulting value change. FNV is not collision-hardened, and no shipped
path yet depends on adversarial collision resistance.

`mos-cache` pairs this content boundary with the path identity as
`BibliographyDependency { DependencyId::Bibliography(ProjectPath), ContentHash }` (§3): the id is
the cache slot, the content hash is the staleness check. `mos-cache` stays free of
bibliography-format knowledge — the caller (`mos-eval`, which reads the `.bib` and depends on both
crates) supplies the hash. Neither type is wired into `CacheKey` or a `DepNode` graph yet; that
remains §9.6.

### 4.2 Semantic node hash

```text
NodeHash(node) = H(
    engine_version,
    node.kind,
    canonical_attrs(authored_attrs(node)),
    asset_refs(node),             // AssetHash(...) per referenced asset
    [NodeHash(child) for child in node.children],
    span_kind_only(node.span)     // file role, not byte offsets
)
```

`NodeHash` covers *authored* semantic state, not resolution residue. The lowerer in `mos-eval`
currently stashes filesystem- and decoder-derived data directly onto node attributes — see
`crates/mos-eval/src/image_lower.rs`, which writes `resolved_path` (an absolute path), `pixels`
(decoded RGB8 bytes), `pixel_width`, `pixel_height`, `colorspace`, and `bits_per_component` onto an
`Image` node. These must *not* feed `NodeHash`:

- `resolved_path` leaks the building user's filesystem layout into the cache and would force a miss
  on every machine.
- `pixels` and decoded dimensions duplicate the asset's bytes into the "semantic" hash and would
  bind a paragraph's `NodeHash` to a transcoder version it should not care about.

The carve-out: `authored_attrs(node)` is the subset of attributes that the parser produced — `src`
(the source-relative path token written by the author), `alt`, `width`, `height`, `label`, `role`,
`text`, list `ordered`-style flags, and so on. Anything the lowerer added during resolution is
addressed indirectly through `asset_refs(node)` (each referenced asset's `AssetHash` per §4.3) or
omitted entirely because it is rederivable.

Concretely for `NodeKind::Image`: `NodeHash` consumes `src`, `alt`, requested `width`/`height`,
`label`, and `AssetHash(resolved asset)`. It does not consume `resolved_path`, `pixels`,
`pixel_width`, `pixel_height`, `colorspace`, or `bits_per_component`. The decoded pixel buffer is
addressed through the asset's content hash; the resolved path is a build-machine detail.

Other notes:

- Span byte offsets are *excluded*. A paragraph that shifts down because an earlier paragraph grew
  must hash identically — that is the whole point of the boundary.
- `canonical_attrs` sorts by key (already a `BTreeMap`), normalizes float NaNs, quantizes authored
  layout dimensions per §6, and rejects `AttrValue::Bytes` entries (which today only appear as
  derived pixel buffers and therefore fail the carve-out above).
- Child hashes feed in as a flat list, not a Merkle root, so adding/removing a sibling re-hashes the
  parent but does not re-hash unaffected siblings.
- The carve-out implies an `mos-eval` follow-up: tag each attribute as authored vs. derived, or move
  derived attributes off the `Node` and onto a side-table the lowerer owns. The design here treats
  that as a precondition for ever computing a meaningful `NodeHash`.

### 4.3 Asset hash

```text
AssetHash(asset) = H(
    engine_version,
    asset_kind,                   // png/jpeg/font/...
    asset_bytes
)
```

Decoding parameters (e.g. requested DPI, target colorspace) are *not* part of the asset hash; they
belong to whatever `LayoutInput` consumes the asset. This keeps one PNG ↔ one `AssetHash` even when
many figures reference it at different sizes.

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

Pages need two hashes, not one. The lookup key and the result digest are different objects and
cannot be the same value, because the result is not known at lookup time.

```text
PageInputHash = H(                // cache lookup key — known before layout
    engine_version,
    StyleHash(page_style),
    page_boundary_signature_in,   // manifest §33 input boundary
    [LayoutInputHash(box) for box queued on this page]
)

PageOutputHash = H(               // result digest — known after layout
    engine_version,
    [LayoutOutputHash(box) for box on page],
    page_boundary_signature_out   // manifest §33 output boundary
)
```

Usage:

- The cache is indexed by `PageInputHash`. Looking up a `PageInputHash` returns the laid-out page
  bytes plus its `PageOutputHash` and output boundary.
- Reflow (manifest §33) compares the cached `page_boundary_signature_out` of page *n* with the fresh
  `page_boundary_signature_in` of page *n+1*. When they match, downstream pages stay cached; when
  they diverge, layout continues until the boundaries reconverge.
- `PageOutputHash` is the convergence digest, not a lookup key. It lets the engine answer "did the
  laid-out page actually change?" without re-comparing the full output box list.

Boundary signatures are hashed rather than stored verbatim so equality checks stay cheap and so the
same boundary state from a different build path collides correctly.

## 5. Determinism expectations

These are the rules a future implementation must follow for the boundaries above to be sound.

1. Hashes MUST NOT depend on:
   - wall-clock time, locale, time zone, the user's `$HOME` or `$USER`,
   - absolute filesystem paths, mtimes, inodes, case-folding done by the OS,
   - environment variables other than ones the engine explicitly declares as build inputs,
   - hash-map iteration order, pointer addresses, or `Arc` identity,
   - parse-order-assigned `NodeId`s once `NodeId` is hash-derived (manifest §5.1).
2. Hashes MUST depend on a stamped `engine_version`. Bumping the engine invalidates everything.
3. NFC normalization is a property of the *layout-input* hash (§4.4), not the source hash. The
   layout engine already normalizes its text inputs to NFC (`manifest-tracker.md` → Layout), so
   paragraphs whose authored text is NFC-equivalent must converge on the same `ParagraphInputHash`.
   The source hash deliberately hashes raw bytes (§4.1) because the parser does not currently
   NFC-normalize and the cache must reflect what the parser actually consumed.
4. Floating-point inputs (widths, leading, sizes) MUST be quantized to the same fixed-point
   resolution used by layout before hashing. See §6.
5. `Node.content_hash` is *not* identity. Two nodes with the same `content_hash` are interchangeable
   as layout inputs but are still distinct `NodeId`s.
6. The cache is a hint, not a source of truth. A cold cache and a warm cache MUST produce
   byte-identical output artifacts. This is the property reproducible builds (manifest §22.1,
   `manifest-tracker.md` → Reproducible Builds) will lean on.

Anything that violates rules 1–4 is a determinism bug, not a cache bug.

## 6. Quantization of layout dimensions

Layout currently uses `f32` points throughout (`A4_WIDTH_PT`, `MARGIN_PT`, `TextStyle.size_pt`).
Float bit patterns are unsuitable as hash inputs: two builds that arrive at the same width through
slightly different arithmetic must hash equally.

Rule: every layout dimension that feeds a hash is first converted to an integer count of 1/64 pt (a
"shaper unit" — same granularity HarfBuzz uses internally), saturating-cast to `i32`. The engine
version stamps the granularity so it can be tightened later without silently invalidating caches.

This applies to `width_pt`, `available_width_pt`, `margin_pt`, `size_pt`, and any future
layout-input length. It does *not* apply to `f32`s that only live inside an output box (e.g. the
exact baseline of a glyph); those go through whichever encoding `LayoutOutputHash` uses.

## 7. How this supports later invalidation

The boundaries above give the future engine enough structure to do the work `manifest-tracker.md`
describes under *Layout* and *Page Reflow And Fixpoints*:

- **Reuse clean semantic nodes.** Compare `NodeHash` before vs. after a parse. Unchanged hashes →
  the `Node` is clean and its downstream `LayoutInput`/`LayoutOutput` entries stay valid.
- **Recompute only affected paragraphs.** A paragraph layout entry is keyed by `ParagraphInputHash`.
  A change confined to that paragraph changes only its `NodeHash`, hence its `ParagraphInputHash`,
  hence its line set. Surrounding paragraphs hit the cache.
- **Reflow only affected pages.** Page reflow consumes page-boundary signatures (manifest §33).
  `PageInputHash` (§4.5) is the cache lookup key; `PageOutputHash` is the convergence digest the
  reflow loop compares against the next page's incoming boundary. Downstream pages whose incoming
  boundary matches an old outgoing boundary are reused wholesale.
- **Update only affected references.** A reference's `ReferenceInputHash` changes iff its target
  node, target number, or target page changes. Re-resolution stays a local edit on the dependency
  graph.
- **Figure invalidation.** Changing the image bytes flips `AssetHash`, which flips
  `FigureInputHash`, which flips the figure box's `LayoutOutputHash`. Changing only the caption text
  flips `NodeHash` on the caption, which flips the nested `caption_input_hash` field of
  `FigureInputHash`, which flips the figure box — without touching the image asset's hash. The
  list-of-figures entry depends on the figure's resolved number/page, so it follows the reference
  pathway above.
- **Invalidate citation data on bibliography edits.** Each declared `.bib` source has a
  `BibliographyDependency`: a `Bibliography` id (the cache slot) plus a `BibliographyContentHash`
  (§4.1, the staleness check). Editing a `.bib`'s bytes flips its content hash, so the engine can
  see that the source changed and recompute only the citation-resolution work that consumed it —
  parsed records, key-existence checks, and the downstream `ReferenceInputHash` of any `[@key]` that
  resolved against it — while sources whose hash is unchanged stay cached. Moving or renaming the
  file changes the id (a different slot) rather than the hash. Today this is the identity/boundary
  pair only; the dependency graph that consumes it is §9.6.
- **Report what changed.** Because every artifact has a typed `DepId` and an `output_hash`, the
  engine can diff hashes and produce the "Reused 842/917 semantic nodes" output style from manifest
  §8.

## 8. Out of scope for this issue

Explicitly *not* designed here:

- The disk layout of `.mos-cache/`. There is no on-disk schema yet.
- Eviction, locking, GC, cross-process sharing.
- The actual hasher choice and the on-wire encoding for `canonical_attrs`.
- Watch-mode loop and CLI surface (`mos watch`, `mos graph`, `mos profile`).
- Float solver and Knuth-Plass; those produce `LayoutOutput` shapes whose hashing this document
  already permits, but the algorithms themselves are MVP 2+.
- The `DepNode` graph, hashing, and any wiring into `CacheKey`. The landed `DependencyId` /
  `DependencyKind` types (§3) are *identities only*; the remaining sketch categories and the graph
  stay design-side until §9 lands them.

## 9. Concrete follow-up issue candidates

These are scope-sized, one-PR work items, not umbrella epics. Each can become its own GitHub issue
when it is ready to start:

1. *Stable `NodeId` derivation in `mos-eval`.* Compute the stable ID —
   `hash(source_id,
   syntactic_position, explicit_label, local_structure)` — inside the lowerer,
   which is the only stage with the parse tree. Add a `Document::alloc_with_id` (or equivalent) on
   `mos-core` so the lowerer can hand precomputed IDs to the arena without `mos-core` learning about
   syntax. Public `NodeId(u64)` stays.
2. *Separate authored vs. derived node attributes.* Precondition for any meaningful `NodeHash`.
   Either tag each entry in `AttrMap` as authored/derived or move derived attributes (today:
   `resolved_path`, `pixels`, `pixel_width`, `pixel_height`, `colorspace`, `bits_per_component` on
   `Image` nodes) off `Node` and onto a side-table the lowerer owns.
3. *Populate `Node.content_hash` in the lowerer.* Compute `NodeHash` per §4.2 during `mos-eval`'s
   lowering pass, drawing only from authored attributes plus `AssetHash` for any referenced asset.
   Cache stays untouched.
4. *Source and asset hashing helpers in `mos-core`.* Provide functions that produce `SourceHash` /
   `AssetHash` per §4.1 and §4.3 with the agreed engine-version stamping. No public `DepId` yet.
5. *Layout-dimension quantization helper.* A small `i32`-of-1/64-pt newtype used wherever layout
   currently passes `f32` widths into anything hash-bound. Lives in `mos-layout`.
6. *`DepNode` graph in `mos-cache` (in-memory only).* The id/kind types landed as `DependencyId` /
   `DependencyKind` (§3); the remaining work is the `DepNode` graph over them and wiring into the
   in-memory cache key. No persistence.
7. *Paragraph layout cache (in-memory).* Add `ParagraphCacheKey` and store one `LayoutOutput` per
   key in `InMemoryCache`. Measure reuse on the existing examples.
8. *Page boundary signature plus `PageInputHash` / `PageOutputHash`.* Introduce the small struct
   that manifest §33 reflow consumes, hash it as the lookup key, and emit the output-side
   convergence digest separately (§4.5).
9. *Persistent `.mos-cache/`.* Only after items 1–8 land. Schema and disk layout get their own
   design note.

Items 1–2 and 4–5 are pure refactors with no behavior change; item 3 populates real hash data and
affects incremental invalidation behavior; items 6–8 add an opt-in cache layer; item 9 is a
separate, larger slice.
