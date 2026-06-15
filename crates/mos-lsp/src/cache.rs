//! A lazily-populated, per-document cache of `mos-eval` lowerings.
//!
//! Go-to-definition (and, later, hover/rename) re-derives the same
//! [`LowerResult`] from a document's source on every request. For an
//! interactive, potentially high-frequency request on a large document,
//! repeating parse + lower per call is pure waste — the source only
//! changes on `didChange`.
//!
//! This cache stores one [`LowerResult`] per document URI, lowered on
//! first demand and reused until the source mutates. The server
//! invalidates an entry whenever it overwrites or removes the document
//! (`didOpen` / `didChange` / `didClose`), so a cached lowering is always
//! derived from the *current* source — there is no document version to
//! track and no staleness to reconcile here.
//!
//! The boundary stays thin: the cache owns no parse/lower policy, it only
//! memoises [`mos_eval::lower`].

use std::collections::HashMap;
use std::path::Path;

use mos_eval::LowerResult;

/// Per-URI memoisation of [`mos_eval::lower`] output.
///
/// An entry is only valid while the document's source is unchanged; the
/// owner ([`crate::server`]) is responsible for calling [`Self::invalidate`]
/// on every source mutation, which is what keeps a cache hit honest.
#[derive(Default, Debug)]
pub(crate) struct LoweringCache {
    entries: HashMap<String, LowerResult>,
}

impl LoweringCache {
    /// Return the lowering for `uri`, reusing a cached [`LowerResult`] when
    /// one is present and lowering `src` (then storing it) on a miss.
    ///
    /// The caller guarantees `src` is the current source for `uri`: because
    /// the server invalidates the entry on every source mutation, a hit can
    /// never be stale, so `src` is ignored whenever an entry already exists.
    pub(crate) fn get_or_lower(&mut self, uri: &str, src: &str, file: &Path) -> &LowerResult {
        self.entries
            .entry(uri.to_owned())
            .or_insert_with(|| mos_eval::lower(src, file))
    }

    /// Drop any cached lowering for `uri`, forcing the next
    /// [`Self::get_or_lower`] to re-lower. Called when the document's source
    /// changes (`didOpen` overwrite / `didChange`) or the document closes.
    pub(crate) fn invalidate(&mut self, uri: &str) {
        self.entries.remove(uri);
    }

    /// Whether a lowering is currently cached for `uri`. Test-only: used to
    /// assert that publishing diagnostics leaves the lowering available for a
    /// later definition request (issue #106) rather than re-lowering.
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
    fn reuses_until_invalidated() {
        let file = PathBuf::from("/virtual/main.mos");
        let mut cache = LoweringCache::default();
        let small = "= A\n";
        let large = "= A\n\n= B\n";

        let small_len = cache.get_or_lower("u", small, &file).document.len();
        // Same URI, a *different* source, but no invalidation: the cached
        // `small` lowering is reused verbatim — the new source is ignored.
        assert_eq!(
            cache.get_or_lower("u", large, &file).document.len(),
            small_len,
            "an un-invalidated hit must reuse the cached lowering, not re-lower"
        );

        cache.invalidate("u");

        // After invalidation the next call lowers the current `large` source.
        let large_len = cache.get_or_lower("u", large, &file).document.len();
        assert!(
            large_len > small_len,
            "after invalidation the cache must re-lower the current source \
             (expected a larger document, got {large_len} vs {small_len})"
        );
    }

    #[test]
    fn distinct_uris_cache_independently() {
        let file = PathBuf::from("/virtual/main.mos");
        let mut cache = LoweringCache::default();
        let a_len = cache.get_or_lower("a", "= A\n", &file).document.len();
        let b_len = cache
            .get_or_lower("b", "= A\n\n= B\n", &file)
            .document
            .len();
        assert!(b_len > a_len, "each URI keeps its own lowering");
        // Re-fetching `a` still yields its own (smaller) lowering.
        assert_eq!(
            cache.get_or_lower("a", "= A\n", &file).document.len(),
            a_len
        );
    }
}
