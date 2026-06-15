//! A per-document cache of `mos-eval` lowerings, shared by diagnostics and
//! go-to-definition so a document is lowered once per edit (issue #106)
//! rather than once per feature per request.
//!
//! The server [`store`](LoweringCache::store)s a [`LowerResult`] keyed by
//! document URI and [`invalidate`](LoweringCache::invalidate)s the entry on
//! every source mutation (`didOpen` / `didChange` / `didClose`), so a cached
//! lowering is always derived from the *current* source text.
//!
//! **Only pure lowerings are cached.** [`mos_eval::lower`] is not a pure
//! function of the source: `#image` / `#figure` and `#bibliography` read
//! external files, so the same text can lower differently as those files
//! appear, change, or fail to load. A [`LowerResult`] with
//! `reads_external_resources` set is therefore never stored — the server
//! lowers such documents fresh on every request (matching pre-cache
//! behavior) rather than risk serving a lowering gone stale against the
//! filesystem with no source edit to invalidate it (issue #106 review).
//!
//! The boundary stays thin: the cache owns no parse/lower policy, it only
//! holds the [`LowerResult`] values the server hands it.

use std::collections::HashMap;

use mos_eval::LowerResult;

/// Per-URI store of pure [`mos_eval::LowerResult`] values.
///
/// An entry is only valid while the document's source is unchanged; the
/// owner ([`crate::server`]) is responsible for calling [`Self::invalidate`]
/// on every source mutation, which is what keeps a cache hit honest.
#[derive(Default, Debug)]
pub(crate) struct LoweringCache {
    entries: HashMap<String, LowerResult>,
}

impl LoweringCache {
    /// The cached lowering for `uri`, or `None` when nothing is stored —
    /// either because the document was never lowered, was invalidated, or its
    /// last lowering was impure and therefore deliberately not cached.
    pub(crate) fn get(&self, uri: &str) -> Option<&LowerResult> {
        self.entries.get(uri)
    }

    /// Store `lowered` for `uri`, overwriting any prior entry.
    ///
    /// Callers must only store **pure** lowerings (`!reads_external_resources`):
    /// caching one that read external files could serve a result gone stale
    /// against the filesystem with no source edit to invalidate it.
    pub(crate) fn store(&mut self, uri: &str, lowered: LowerResult) {
        self.entries.insert(uri.to_owned(), lowered);
    }

    /// Drop any cached lowering for `uri`. Called when the document's source
    /// changes (`didOpen` overwrite / `didChange`) or the document closes.
    pub(crate) fn invalidate(&mut self, uri: &str) {
        self.entries.remove(uri);
    }

    /// Whether a lowering is currently cached for `uri`. Test-only: used to
    /// assert that publishing diagnostics leaves a (pure) lowering available
    /// for a later definition request, and that impure lowerings are not
    /// cached (issue #106).
    #[cfg(test)]
    pub(crate) fn is_cached(&self, uri: &str) -> bool {
        self.entries.contains_key(uri)
    }
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        reason = "tests panic loudly on setup failure; matches crate-wide test-module convention"
    )]

    use std::path::PathBuf;

    use super::*;

    #[test]
    fn stores_and_reuses_until_invalidated() {
        let file = PathBuf::from("/virtual/main.mos");
        let mut cache = LoweringCache::default();
        assert!(cache.get("u").is_none(), "an empty cache returns nothing");

        let lowered = mos_eval::lower("= A\n", &file);
        let stored_len = lowered.document.len();
        cache.store("u", lowered);
        assert_eq!(
            cache.get("u").map(|l| l.document.len()),
            Some(stored_len),
            "a stored lowering is returned by `get`"
        );

        cache.invalidate("u");
        assert!(cache.get("u").is_none(), "invalidate drops the entry");
    }

    #[test]
    fn distinct_uris_store_independently() {
        let file = PathBuf::from("/virtual/main.mos");
        let mut cache = LoweringCache::default();
        cache.store("a", mos_eval::lower("= A\n", &file));
        cache.store("b", mos_eval::lower("= A\n\n= B\n", &file));
        let a_len = cache.get("a").map(|l| l.document.len());
        let b_len = cache.get("b").map(|l| l.document.len());
        assert!(b_len > a_len, "each URI keeps its own lowering");
    }

    #[test]
    fn pure_source_lowers_without_external_reads() {
        // A plain document touches no filesystem, so its lowering is pure and
        // safe to cache. A document with `#figure` (which loads an image file)
        // is flagged impure — the server must not cache it.
        let file = PathBuf::from("/virtual/main.mos");
        assert!(
            !mos_eval::lower("= A\n\nSee @a\n", &file).reads_external_resources,
            "a source with no external directives lowers purely"
        );
        assert!(
            mos_eval::lower("#figure(image: \"x.png\", label: \"fig\")\n", &file)
                .reads_external_resources,
            "a `#figure` image load makes the lowering depend on external files"
        );
    }
}
