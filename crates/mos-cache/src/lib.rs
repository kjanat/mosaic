//! Incremental build cache (manifest §7, §32).
//!
//! Today this crate holds the typed dependency-identity vocabulary
//! ([`DependencyId`] / [`DependencyKind`] / [`ProjectPath`], plus the
//! [`BibliographyDependency`] content boundary) and a tiny in-memory byte
//! [`Cache`]. The `DepNode` graph and the persistent content-addressed
//! `.mos-cache/` are MVP 5 work, still design-side in
//! `docs/incremental-dependencies.md` (§9).

#![doc(
    html_logo_url = "https://mosaic.kjanat.dev/assets/A4.svg",
    html_favicon_url = "https://mosaic.kjanat.dev/assets/A4.svg"
)]

use std::collections::HashMap;

use mos_core::ContentHash;

mod dependency;

pub use dependency::{
    BibliographyDependency, DependencyId, DependencyKind, ProjectPath, ProjectPathError,
};

/// A cache entry's address. Real keys include node, style, and width
/// hashes (manifest §32). For now the type is opaque.
///
/// # Examples
///
/// ```
/// use mos_cache::CacheKey;
/// use mos_core::ContentHash;
///
/// let key = CacheKey(ContentHash(42));
///
/// assert_eq!(key.0, ContentHash(42));
/// ```
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct CacheKey(pub ContentHash);

/// Cache trait. Implementations: in-memory (default), on-disk (MVP 5).
///
/// # Examples
///
/// ```
/// use mos_cache::{Cache, CacheKey, InMemoryCache};
/// use mos_core::ContentHash;
///
/// let mut cache = InMemoryCache::default();
/// let key = CacheKey(ContentHash(7));
/// cache.put(key, vec![1, 2, 3]);
///
/// assert_eq!(cache.get(&key), Some(vec![1, 2, 3]));
/// ```
pub trait Cache {
    /// Return a cached payload for `key`, if present.
    ///
    /// # Examples
    ///
    /// ```
    /// use mos_cache::{Cache, CacheKey, InMemoryCache};
    /// use mos_core::ContentHash;
    ///
    /// let cache = InMemoryCache::default();
    ///
    /// assert_eq!(cache.get(&CacheKey(ContentHash(1))), None);
    /// ```
    fn get(&self, key: &CacheKey) -> Option<Vec<u8>>;

    /// Store `value` under `key`, replacing any previous value.
    ///
    /// # Examples
    ///
    /// ```
    /// use mos_cache::{Cache, CacheKey, InMemoryCache};
    /// use mos_core::ContentHash;
    ///
    /// let mut cache = InMemoryCache::default();
    /// let key = CacheKey(ContentHash(1));
    /// cache.put(key, vec![9]);
    ///
    /// assert_eq!(cache.get(&key), Some(vec![9]));
    /// ```
    fn put(&mut self, key: CacheKey, value: Vec<u8>);
}

/// Hash-map backed cache used by tests and the current MVP pipeline.
///
/// # Examples
///
/// ```
/// use mos_cache::{Cache, CacheKey, InMemoryCache};
/// use mos_core::ContentHash;
///
/// let mut cache = InMemoryCache::default();
/// let key = CacheKey(ContentHash(5));
/// cache.put(key, b"pdf".to_vec());
///
/// assert_eq!(cache.get(&key), Some(b"pdf".to_vec()));
/// ```
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
