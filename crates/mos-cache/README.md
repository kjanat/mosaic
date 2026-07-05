# mos-cache

Incremental build cache boundary for Mosaic.

Current state: small public API plus in-memory implementation. This crate is not wired into a
persistent build pipeline yet, and it does not write `.mos-cache/` on disk.

> [!WARNING]
> While this crate is in the `0.0.x` line, Mosaic treats it as pre-alpha. Breaking changes are
> acceptable between patch releases. If you depend on this crate, pin an exact version such as
> `=0.0.2`, or accept the risk of API breakage.

## API

- `CacheKey(pub ContentHash)`: opaque cache address wrapper. Today it is just a
  `mos_core::ContentHash`.
- `Cache`: trait with `get` and `put` for byte payloads.
- `InMemoryCache`: `HashMap<CacheKey, Vec<u8>>` implementation for tests and future integration
  slices.

Behavior:

- `get(&CacheKey)` returns `Some(Vec<u8>)` when present, cloned from the cache.
- `get(&CacheKey)` returns `None` for misses.
- `put(CacheKey, Vec<u8>)` stores bytes and replaces any prior value for the same key.
- Values are untyped byte payloads. Serialization and cache validity are caller responsibilities.
- Data lasts only as long as the `InMemoryCache` value.

## Example

```rust
use mos_cache::{Cache, CacheKey, InMemoryCache};
use mos_core::ContentHash;

let mut cache = InMemoryCache::default();
let key = CacheKey(ContentHash(7));

assert_eq!(cache.get(&key), None);

cache.put(key, b"layout-bytes".to_vec());

assert_eq!(cache.get(&key), Some(b"layout-bytes".to_vec()));
```

## Boundary

`mos-cache` depends only on `mos-core`. Keep it close to core types and away from parser, evaluator,
layout, PDF, CLI, package, and LSP logic until a real integration slice needs more.

The manifest describes a content-addressed dependency graph cache with persisted artifacts. That is
design direction, not shipped behavior.

## Known Non-Goals Today

- No disk persistence.
- No `.mos-cache/` directory management.
- No dependency graph or invalidation engine.
- No stable key schema for node/style/width hashes.
- No cache eviction, locking, versioning, or cross-process sharing.
- No CLI integration for `mos check`, `mos build`, `watch`, or `clean`.
