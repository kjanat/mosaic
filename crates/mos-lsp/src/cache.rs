//! A per-document cache of `mos-eval` lowerings, shared by diagnostics and
//! go-to-definition so a document is lowered once per edit (issue #106)
//! rather than once per feature per request.
//!
//! The server [`store`](Store::store)s a [`LowerResult`] keyed by
//! document URI and [`invalidate`](Store::invalidate)s the entry on
//! every source mutation (`didOpen` / `didChange` / `didClose`), so a cached
//! lowering is always derived from the *current* source text.
//!
//! [`mos_eval::lower`] is not a pure function of the source: `#image` /
//! `#figure` and `#bibliography` read external files, so the same text can
//! lower differently as those files appear, change, or fail to load. Every
//! [`LowerResult`] records those files with their fingerprints in
//! `external_dependencies`, and [`get_if_current`](Store::get_if_current)
//! checks each one on every hit (a `stat`, and a re-hash only when the size
//! or modification time moved), evicting the entry the moment any file
//! differs (issue #125). A pure lowering has no dependencies and is reused
//! without touching the filesystem.
//!
//! The boundary stays thin: the cache owns no parse/lower policy, it only
//! holds the [`LowerResult`] values the server hands it.

use std::collections::HashMap;

use mos_eval::LowerResult;

/// Per-URI store of [`mos_eval::LowerResult`] values.
///
/// An entry stays valid on two conditions. The document's source must be
/// unchanged: the owner ([`crate::server`]) calls [`Self::invalidate`] on
/// every source mutation. And every external file the lowering read must
/// still be current: [`Self::get_if_current`] checks that on each hit.
#[derive(Default, Debug)]
pub struct Store {
    entries: HashMap<String, LowerResult>,
}

impl Store {
    /// The cached lowering for `uri` when every external file it read is
    /// still [current](mos_eval::ExternalDependency::is_current). A stale
    /// entry is evicted and `None` is returned so the caller lowers fresh.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::path::Path;
    ///
    /// use mos_lsp::cache::Store;
    ///
    /// let mut cache = Store::default();
    /// cache.store("file:///doc.mos", mos_eval::lower("= Title\n", Path::new("doc.mos")));
    /// assert!(cache.get_if_current("file:///doc.mos").is_some());
    /// ```
    #[must_use]
    pub fn get_if_current(&mut self, uri: &str) -> Option<&LowerResult> {
        let current = self
            .entries
            .get(uri)?
            .external_dependencies
            .iter()
            .all(mos_eval::ExternalDependency::is_current);
        if !current {
            self.entries.remove(uri);
            return None;
        }
        self.entries.get(uri)
    }

    /// Store `lowered` for `uri`, overwriting any prior entry.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::path::Path;
    ///
    /// use mos_lsp::cache::Store;
    ///
    /// let mut cache = Store::default();
    /// let lowered = mos_eval::lower("= Title\n", Path::new("doc.mos"));
    /// cache.store("file:///doc.mos", lowered);
    /// assert!(cache.get_if_current("file:///doc.mos").is_some());
    /// ```
    pub fn store(&mut self, uri: &str, lowered: LowerResult) {
        self.entries.insert(uri.to_owned(), lowered);
    }

    /// Drop any cached lowering for `uri`. Called when the document's source
    /// changes (`didOpen` overwrite / `didChange`) or the document closes.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::path::Path;
    ///
    /// use mos_lsp::cache::Store;
    ///
    /// let mut cache = Store::default();
    /// cache.store("file:///doc.mos", mos_eval::lower("body", Path::new("doc.mos")));
    /// cache.invalidate("file:///doc.mos");
    /// assert!(cache.get_if_current("file:///doc.mos").is_none());
    /// ```
    pub fn invalidate(&mut self, uri: &str) {
        self.entries.remove(uri);
    }

    /// Whether a lowering is currently cached for `uri`. Test-only: used to
    /// assert that publishing diagnostics leaves a lowering available for a
    /// later definition request and that a stale one is evicted.
    #[cfg(test)]
    #[must_use]
    pub fn is_cached(&self, uri: &str) -> bool {
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
        let mut cache = Store::default();
        assert!(
            cache.get_if_current("u").is_none(),
            "an empty cache returns nothing"
        );

        let lowered = mos_eval::lower("= A\n", &file);
        let stored_len = lowered.document.len();
        cache.store("u", lowered);
        assert_eq!(
            cache.get_if_current("u").map(|l| l.document.len()),
            Some(stored_len),
            "a stored pure lowering is returned"
        );

        cache.invalidate("u");
        assert!(
            cache.get_if_current("u").is_none(),
            "invalidate drops the entry"
        );
    }

    #[test]
    fn distinct_uris_store_independently() {
        let file = PathBuf::from("/virtual/main.mos");
        let mut cache = Store::default();
        cache.store("a", mos_eval::lower("= A\n", &file));
        cache.store("b", mos_eval::lower("= A\n\n= B\n", &file));
        let a_len = cache.get_if_current("a").map(|l| l.document.len());
        let b_len = cache.get_if_current("b").map(|l| l.document.len());
        assert!(b_len > a_len, "each URI keeps its own lowering");
    }

    #[test]
    fn pure_source_lowers_without_external_reads() {
        let file = PathBuf::from("/virtual/main.mos");
        assert!(
            !mos_eval::lower("= A\n\nSee @a\n", &file).reads_external_resources(),
            "a source with no external directives lowers purely"
        );
        assert!(
            mos_eval::lower("#figure(image: \"x.png\", label: \"fig\")\n", &file)
                .reads_external_resources(),
            "a `#figure` image load makes the lowering depend on external files"
        );
    }

    fn unique_temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "mos-lsp-cache-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos())
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn impure_entry_is_current_while_its_files_are_unchanged() {
        let dir = unique_temp_dir("unchanged");
        let main = dir.join("main.mos");
        std::fs::write(dir.join("x.png"), b"not really a png").unwrap();
        let mut cache = Store::default();
        cache.store("u", mos_eval::lower("#image(\"x.png\")\n", &main));

        assert!(
            cache.get_if_current("u").is_some(),
            "an impure lowering stays reusable while its dependencies match"
        );
        assert!(cache.is_cached("u"));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn impure_entry_is_evicted_when_a_file_changes() {
        let dir = unique_temp_dir("changed");
        let main = dir.join("main.mos");
        let image = dir.join("x.png");
        std::fs::write(&image, b"version one").unwrap();
        let mut cache = Store::default();
        cache.store("u", mos_eval::lower("#image(\"x.png\")\n", &main));

        std::fs::write(&image, b"version two").unwrap();
        assert!(
            cache.get_if_current("u").is_none(),
            "changed file contents invalidate the cached lowering"
        );
        assert!(!cache.is_cached("u"), "the stale entry is dropped");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn impure_entry_is_evicted_by_a_same_size_rewrite_that_restores_the_mtime() {
        let dir = unique_temp_dir("same-size");
        let main = dir.join("main.mos");
        let image = dir.join("x.png");
        std::fs::write(&image, b"version one").unwrap();
        let modified = std::fs::metadata(&image).unwrap().modified().unwrap();
        let mut cache = Store::default();
        cache.store("u", mos_eval::lower("#image(\"x.png\")\n", &main));

        std::fs::write(&image, b"version two").unwrap();
        std::fs::File::options()
            .write(true)
            .open(&image)
            .unwrap()
            .set_modified(modified)
            .unwrap();
        assert!(
            cache.get_if_current("u").is_none(),
            "same size and same mtime must not hide new contents"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn impure_entry_is_evicted_when_a_missing_file_appears() {
        let dir = unique_temp_dir("appears");
        let main = dir.join("main.mos");
        let mut cache = Store::default();
        cache.store("u", mos_eval::lower("#image(\"x.png\")\n", &main));
        assert!(
            cache.get_if_current("u").is_some(),
            "still missing, still current"
        );

        std::fs::write(dir.join("x.png"), b"now it exists").unwrap();
        assert!(
            cache.get_if_current("u").is_none(),
            "a dependency that appears invalidates the cached lowering"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn impure_entry_is_evicted_when_a_file_disappears() {
        let dir = unique_temp_dir("disappears");
        let main = dir.join("main.mos");
        let bib = dir.join("refs.bib");
        std::fs::write(&bib, "@book{k, title={T}}\n").unwrap();
        let mut cache = Store::default();
        cache.store(
            "u",
            mos_eval::lower("#bibliography(\"refs.bib\")\n\nsee [@k]\n", &main),
        );
        assert!(cache.get_if_current("u").is_some());

        std::fs::remove_file(&bib).unwrap();
        assert!(
            cache.get_if_current("u").is_none(),
            "a dependency that disappears invalidates the cached lowering"
        );
        std::fs::remove_dir_all(&dir).ok();
    }
}
