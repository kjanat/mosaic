# MOS-CACHE KNOWLEDGE BASE

## OVERVIEW

`mos-cache` is a tiny cache trait plus in-memory byte map, and the typed dependency-identity
vocabulary (`DependencyId` / `DependencyKind` / `ProjectPath`). Persistent incremental builds are
not implemented yet.

## WHERE TO LOOK

| Task             | Location                           | Notes                                                        |
| ---------------- | ---------------------------------- | ------------------------------------------------------------ |
| Cache API        | `src/lib.rs`                       | `Cache`, `CacheKey`, `InMemoryCache`.                        |
| Dependency ids   | `src/dependency.rs`                | `DependencyId` / `DependencyKind`; identities only.          |
| Path identity    | `src/dependency.rs`                | `ProjectPath` canonicalizes file paths (slash/`.`/`..`/NFC). |
| Key source       | `mos-core`                         | `CacheKey` wraps `ContentHash`.                              |
| Design boundary  | `docs/incremental-dependencies.md` | §3 maps types to the full sketch + what is deferred.         |
| Future direction | `README.md`                        | Treat as intent unless code implements it.                   |

## CURRENT SLICE

- Depends only on `mos-core`.
- `CacheKey` is opaque and currently wraps `ContentHash`.
- `Cache` stores and returns `Vec<u8>` payloads.
- `InMemoryCache` uses a `HashMap` and clones payloads on `get`.
- Serialization, validation, and type meaning of bytes are caller responsibility.
- `DependencyId` models four kinds: source/asset/bibliography files (canonical `ProjectPath`) and a
  label name (`String`). `ProjectPath` enforces the §3.1 canonical form so equal logical inputs
  share one identity. No hashing, no graph, not wired into `CacheKey`.
- Layout inputs are intentionally deferred: `StyleId` is defaulted (`0`) so it is not yet a real
  identity; wait for the `ParagraphInputHash` layout key (§4.4) before adding the kind.

## BOUNDARY RULES

- Keep this crate below parse/eval/layout/PDF/CLI/LSP until real integration exists.
- Cache invalidation/versioning/locking/eviction must be modeled explicitly before use.
- Disk persistence needs a real schema and tests; no ad hoc `.mos-cache/` writes.

## ANTI-PATTERNS

- Do not pretend `CacheKey` is the final manifest dependency key schema.
- Do not add filesystem writes or global cache state casually.
- Do not wire into `mos build` unless implementing the full behavior slice.
- Do not model `DependencyId` kinds whose identity is still defaulted (`Node`, `Style` bundles,
  layout inputs keyed on bare `StyleId`, packages). Add a variant only when it has a real, stable
  identity scheme.
