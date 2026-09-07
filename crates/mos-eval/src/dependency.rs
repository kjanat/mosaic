//! External files a lowering read, with the fingerprint each had at the
//! time, so a cached [`LowerResult`](crate::LowerResult) can be checked
//! against the filesystem before it is reused (issue #125).

use std::collections::BTreeMap;
use std::fs::Metadata;
use std::io;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use mos_core::{ContentHash, ContentHasher};

const DOMAIN_TAG: &[u8] = b"mos-eval/external-dependency/v1";

/// What a regular file looked like when it was read: its size and
/// modification time from `stat`, and a [`fingerprint_bytes`] hash of its
/// contents.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Fingerprint {
    /// File size in bytes.
    pub len: u64,
    /// Modification time, when the platform reports one.
    pub modified: Option<SystemTime>,
    /// Hash of the bytes that were read.
    pub content: ContentHash,
}

/// One external file a lowering depended on: an `#image` / `#figure` raster or
/// a `#bibliography` source.
///
/// `fingerprint` is `None` when the path was not a readable regular file at
/// lowering time, so a file that later appears is as much a change as one
/// that is edited.
///
/// # Examples
///
/// ```
/// use std::path::Path;
///
/// use mos_eval::ExternalDependency;
///
/// let dep = ExternalDependency::observe(Path::new("/nonexistent/x.png"));
/// assert_eq!(dep.fingerprint, None);
/// assert!(dep.is_current());
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExternalDependency {
    /// The resolved filesystem path that was read.
    pub path: PathBuf,
    /// The file's [`Fingerprint`], or `None` when it could not be read.
    pub fingerprint: Option<Fingerprint>,
}

impl ExternalDependency {
    /// Record `path` with the fingerprint it has right now.
    #[must_use]
    pub fn observe(path: impl Into<PathBuf>) -> Self {
        let path = path.into();
        let fingerprint = fingerprint_file(&path);
        Self { path, fingerprint }
    }

    /// Whether the file on disk still matches the recorded fingerprint.
    ///
    /// A `stat` settles the common cases: a file whose size and modification
    /// time are unchanged is current, and a file that appeared or vanished is
    /// not. Only when the `stat` differs are the bytes re-read and hashed, so
    /// a `touch` that leaves the contents alone still counts as current.
    #[must_use]
    pub fn is_current(&self) -> bool {
        let Some(recorded) = &self.fingerprint else {
            return regular_file_metadata(&self.path).is_none();
        };
        let Some(metadata) = regular_file_metadata(&self.path) else {
            return false;
        };
        if metadata.len() == recorded.len && metadata.modified().ok() == recorded.modified {
            return true;
        }
        fingerprint_file(&self.path).is_some_and(|now| now.content == recorded.content)
    }
}

/// Fingerprint raw file bytes for dependency comparison.
///
/// # Examples
///
/// ```
/// use mos_eval::fingerprint_bytes;
///
/// assert_eq!(fingerprint_bytes(b"a"), fingerprint_bytes(b"a"));
/// assert_ne!(fingerprint_bytes(b"a"), fingerprint_bytes(b"b"));
/// ```
#[must_use]
pub fn fingerprint_bytes(bytes: &[u8]) -> ContentHash {
    let mut hasher = ContentHasher::new();
    hasher.field(DOMAIN_TAG).field(bytes);
    hasher.finish()
}

/// The [`Fingerprint`] of the regular file at `path`, or `None` when there is
/// no readable regular file there. Directories, devices, and pipes are never
/// opened.
#[must_use]
pub fn fingerprint_file(path: &Path) -> Option<Fingerprint> {
    read_fingerprinted(path)
        .ok()
        .map(|(_, fingerprint)| fingerprint)
}

/// Read the regular file at `path` and fingerprint it in one pass. The
/// `stat` is taken before the read, so a write that lands between the two
/// shows up as a changed `stat` on the next [`ExternalDependency::is_current`]
/// and forces a hash comparison.
pub(crate) fn read_fingerprinted(path: &Path) -> io::Result<(Vec<u8>, Fingerprint)> {
    let metadata = std::fs::metadata(path)?;
    if !metadata.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "not a regular file",
        ));
    }
    let bytes = std::fs::read(path)?;
    let fingerprint = Fingerprint {
        len: metadata.len(),
        modified: metadata.modified().ok(),
        content: fingerprint_bytes(&bytes),
    };
    Ok((bytes, fingerprint))
}

fn regular_file_metadata(path: &Path) -> Option<Metadata> {
    std::fs::metadata(path).ok().filter(Metadata::is_file)
}

/// The dependencies observed so far while lowering one document, keyed by
/// path so a file read twice is recorded once.
#[derive(Debug, Default)]
pub(crate) struct DependencySet {
    entries: BTreeMap<PathBuf, Option<Fingerprint>>,
}

impl DependencySet {
    pub(crate) fn record(&mut self, path: PathBuf, fingerprint: Option<Fingerprint>) {
        self.entries.insert(path, fingerprint);
    }

    pub(crate) fn into_vec(self) -> Vec<ExternalDependency> {
        self.entries
            .into_iter()
            .map(|(path, fingerprint)| ExternalDependency { path, fingerprint })
            .collect()
    }
}

impl From<Vec<ExternalDependency>> for DependencySet {
    fn from(dependencies: Vec<ExternalDependency>) -> Self {
        Self {
            entries: dependencies
                .into_iter()
                .map(|dep| (dep.path, dep.fingerprint))
                .collect(),
        }
    }
}

/// What a directive lowerer needs to touch the filesystem: the `.mos` file
/// paths resolve against, and the set that records every file it reads.
pub(crate) struct ExternalInputs<'a> {
    pub(crate) source_file: &'a Path,
    pub(crate) dependencies: &'a mut DependencySet,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unique_temp_file(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "mos-eval-dependency-{name}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos())
        ));
        std::fs::create_dir_all(&dir).expect("temp dir");
        dir.join("file.bin")
    }

    fn cleanup(path: &Path) {
        std::fs::remove_dir_all(path.parent().expect("parent")).ok();
    }

    #[test]
    fn missing_file_has_no_fingerprint_and_is_current_while_still_missing() {
        let path = unique_temp_file("missing");
        assert_eq!(fingerprint_file(&path), None);
        let dep = ExternalDependency::observe(&path);
        assert!(dep.is_current());
        std::fs::write(&path, b"x").expect("write");
        assert!(!dep.is_current(), "appearing counts as a change");
        cleanup(&path);
    }

    #[test]
    fn directory_is_never_read() {
        let path = unique_temp_file("dir");
        let dir = path.parent().expect("parent");
        assert_eq!(fingerprint_file(dir), None);
        assert!(ExternalDependency::observe(dir).is_current());
        cleanup(&path);
    }

    #[test]
    fn same_bytes_fingerprint_equal_and_different_bytes_diverge() {
        let path = unique_temp_file("bytes");
        std::fs::write(&path, b"one").expect("write");
        assert_eq!(
            fingerprint_file(&path).map(|f| f.content),
            Some(fingerprint_bytes(b"one"))
        );
        let dep = ExternalDependency::observe(&path);
        std::fs::write(&path, b"two").expect("write");
        assert!(!dep.is_current());
        std::fs::remove_file(&path).expect("remove");
        assert!(!dep.is_current(), "disappearing counts as a change");
        cleanup(&path);
    }

    #[test]
    fn unchanged_contents_stay_current_when_only_the_stat_moved() {
        let path = unique_temp_file("touch");
        std::fs::write(&path, b"same").expect("write");
        let dep = ExternalDependency::observe(&path);
        let recorded = dep.fingerprint.expect("fingerprint");
        let moved = ExternalDependency {
            path: path.clone(),
            fingerprint: Some(Fingerprint {
                len: recorded.len,
                modified: Some(SystemTime::UNIX_EPOCH),
                content: recorded.content,
            }),
        };
        assert!(moved.is_current(), "a stat mismatch falls back to the hash");
        let stale = ExternalDependency {
            path: path.clone(),
            fingerprint: Some(Fingerprint {
                len: recorded.len,
                modified: Some(SystemTime::UNIX_EPOCH),
                content: fingerprint_bytes(b"other"),
            }),
        };
        assert!(!stale.is_current());
        cleanup(&path);
    }

    #[test]
    fn dependency_set_dedupes_by_path_and_orders_deterministically() {
        let fp = |n: u128| {
            Some(Fingerprint {
                len: 1,
                modified: None,
                content: ContentHash(n),
            })
        };
        let mut set = DependencySet::default();
        set.record(PathBuf::from("b"), None);
        set.record(PathBuf::from("a"), fp(1));
        set.record(PathBuf::from("b"), fp(2));
        let deps = set.into_vec();
        assert_eq!(
            deps,
            vec![
                ExternalDependency {
                    path: PathBuf::from("a"),
                    fingerprint: fp(1),
                },
                ExternalDependency {
                    path: PathBuf::from("b"),
                    fingerprint: fp(2),
                },
            ]
        );
        assert_eq!(DependencySet::from(deps.clone()).into_vec(), deps);
    }
}
