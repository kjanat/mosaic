//! Incremental build cache (manifest §7, §32).
//!
//! The dependency graph (`DepNode`) and content-addressed cache live
//! here. The MVP 5 implementation will persist to `.mosaic-cache/`.

use std::collections::HashMap;

use mosaic_core::ContentHash;

/// A cache entry's address. Real keys include node, style, and width
/// hashes (manifest §32). For now the type is opaque.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct CacheKey(pub ContentHash);

/// Cache trait. Implementations: in-memory (default), on-disk (MVP 5).
pub trait Cache {
    fn get(&self, key: &CacheKey) -> Option<Vec<u8>>;
    fn put(&mut self, key: CacheKey, value: Vec<u8>);
}

#[derive(Default, Debug)]
pub struct InMemoryCache {
    entries: HashMap<CacheKey, Vec<u8>>,
}

impl Cache for InMemoryCache {
    fn get(&self, key: &CacheKey) -> Option<Vec<u8>> {
        self.entries.get(key).cloned()
    }

    fn put(&mut self, key: CacheKey, value: Vec<u8>) {
        self.entries.insert(key, value);
    }
}
