# MOS-CACHE KNOWLEDGE BASE

## OVERVIEW

`mos-cache` is a tiny cache trait plus in-memory byte map, and the typed dependency-identity
vocabulary (`DependencyId` / `DependencyKind`). Persistent incremental builds are not implemented
yet.

## WHERE TO LOOK

| Task             | Location                           | Notes                                                          |
| ---------------- | ---------------------------------- | -------------------------------------------------------------- |
| Cache API        | `src/lib.rs`                       | `Cache`, `CacheKey`, `InMemoryCache`.                          |
| Dependency ids   | `src/dependency.rs`                | `DependencyId` / `DependencyKind`; identities only.            |
| Key source       | `mos-core`                         | `CacheKey` wraps `ContentHash`; `LayoutInput` wraps `StyleId`. |
| Design boundary  | `docs/incremental-dependencies.md` | §3 maps types to the full sketch + what is deferred.           |
| Future direction | `README.md`                        | Treat as intent unless code implements it.                     |

## CURRENT SLICE

- Depends only on `mos-core`.
- `CacheKey` is opaque and currently wraps `ContentHash`.
- `Cache` stores and returns `Vec<u8>` payloads.
- `InMemoryCache` uses a `HashMap` and clones payloads on `get`.
- Serialization, validation, and type meaning of bytes are caller responsibility.
- `DependencyId` models five kinds (source/asset/bibliography file paths, label name, layout
  `StyleId`). Payloads are inline strong types under the variant tag — no wrapper newtypes. No
  hashing, no graph, not wired into `CacheKey`.

## BOUNDARY RULES

- Keep this crate below parse/eval/layout/PDF/CLI/LSP until real integration exists.
- Cache invalidation/versioning/locking/eviction must be modeled explicitly before use.
- Disk persistence needs a real schema and tests; no ad hoc `.mos-cache/` writes.

## ANTI-PATTERNS

- Do not pretend `CacheKey` is the final manifest dependency key schema.
- Do not add filesystem writes or global cache state casually.
- Do not wire into `mos build` unless implementing the full behavior slice.
- Do not model `DependencyId` kinds whose identity is still defaulted (`Node`, `Style` bundles,
  packages). Add a variant only when it has a real, stable identity scheme.
