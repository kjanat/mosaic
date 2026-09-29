//! External files a lowering read, with the fingerprint each had at the
//! time, so a cached [`LowerResult`](crate::LowerResult) can be checked
//! against the filesystem before it is reused (issue #125).

use std::collections::BTreeMap;
use std::fs::Metadata;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use mos_core::{ContentHash, ContentHasher};

const DOMAIN_TAG: &[u8] = b"mos-eval/external-dependency/v1";

/// A modification time this close to the moment the fingerprint was taken is
/// not trusted by [`ExternalDependency::is_current`]: a second write inside
/// the same filesystem timestamp tick would leave `stat` unchanged. Two
/// seconds covers FAT's timestamp resolution.
pub const RACY_WINDOW: Duration = Duration::from_secs(2);

/// What a regular file looked like when it was read: its `stat` fields, the
/// moment that `stat` was taken, and a [`fingerprint_bytes`] hash of its
/// contents.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Fingerprint {
    /// File size in bytes.
    pub len: u64,
    /// Modification time, when the platform reports one.
    pub modified: Option<SystemTime>,
    /// Inode-level identity; all fields are `None` off Unix.
    pub identity: FileIdentity,
    /// When the `stat` was taken.
    pub observed: SystemTime,
    /// Hash of the bytes that were read.
    pub content: ContentHash,
}

/// The device and inode numbers and the status-change time of a file on
/// Unix. Unlike the modification time, none of these can be set from
/// userspace, so a rewrite that restores the old mtime still moves one of
/// them. Every field is `None` on platforms that do not expose them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FileIdentity {
    /// Device number.
    pub device: Option<u64>,
    /// Inode number.
    pub inode: Option<u64>,
    /// Status-change time (`ctime`), when it is representable.
    pub changed: Option<SystemTime>,
}

impl FileIdentity {
    /// The identity of a file on a platform that exposes none of the fields.
    pub const NONE: Self = Self {
        device: None,
        inode: None,
        changed: None,
    };

    #[cfg(unix)]
    fn of(metadata: &Metadata) -> Self {
        use std::os::unix::fs::MetadataExt;

        let changed = u64::try_from(metadata.ctime())
            .ok()
            .zip(u32::try_from(metadata.ctime_nsec()).ok())
            .and_then(|(secs, nanos)| {
                SystemTime::UNIX_EPOCH.checked_add(Duration::new(secs, nanos))
            });
        Self {
            device: Some(metadata.dev()),
            inode: Some(metadata.ino()),
            changed,
        }
    }

    #[cfg(not(unix))]
    fn of(_metadata: &Metadata) -> Self {
        Self::NONE
    }
}

impl Fingerprint {
    fn stat_matches(&self, metadata: &Metadata) -> bool {
        metadata.len() == self.len
            && metadata.modified().ok() == self.modified
            && FileIdentity::of(metadata) == self.identity
    }

    /// Whether the file was modified within [`RACY_WINDOW`] of the moment the
    /// fingerprint was taken, or has no modification time at all, so a
    /// matching `stat` is not proof that the contents are unchanged.
    #[must_use]
    pub fn is_racy(&self) -> bool {
        let Some(modified) = self.modified else {
            return true;
        };
        !self
            .observed
            .duration_since(modified)
            .is_ok_and(|age| age >= RACY_WINDOW)
    }
}

/// A resource content hash, optionally accompanied by filesystem freshness metadata.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResourceFingerprint {
    /// A regular file read by [`FileSystemReader`].
    File(Fingerprint),
    /// Caller-supplied bytes with no filesystem identity or timestamps.
    Content(ContentHash),
}

impl ResourceFingerprint {
    /// Hash of the exact bytes consumed by lowering.
    #[must_use]
    pub const fn content(self) -> ContentHash {
        match self {
            Self::File(file) => file.content,
            Self::Content(content) => content,
        }
    }
}

/// Bytes returned by a [`ResourceReader`].
#[derive(Clone, Debug)]
pub struct ResourceData {
    bytes: Arc<[u8]>,
    fingerprint: ResourceFingerprint,
}

impl ResourceData {
    /// Capture supplied bytes without consulting the filesystem.
    #[must_use]
    pub fn new(bytes: impl Into<Arc<[u8]>>) -> Self {
        let bytes = bytes.into();
        let fingerprint = ResourceFingerprint::Content(fingerprint_bytes(&bytes));
        Self { bytes, fingerprint }
    }
}

/// Supplies image and bibliography bytes at already-resolved source paths.
///
/// Each path is read at most once per lowering, including failed reads. Implementors
/// may use memory, archives, or editor overlays; they need no filesystem metadata.
/// The reader is synchronous and need not implement `Send` or `Sync`.
pub trait ResourceReader {
    /// Read a resolved path, or return the underlying resource error.
    ///
    /// # Errors
    ///
    /// Return `NotFound` for absent resources, `InvalidInput` for paths that do
    /// not identify readable resources (such as directories), and an appropriate
    /// I/O error for other failures. Lowering preserves the error in its snapshot.
    fn read(&self, path: &Path) -> io::Result<ResourceData>;
}

impl<F> ResourceReader for F
where
    F: Fn(&Path) -> io::Result<ResourceData>,
{
    fn read(&self, path: &Path) -> io::Result<ResourceData> {
        self(path)
    }
}

/// Default resource reader, restricted to regular files.
#[derive(Clone, Copy, Debug, Default)]
pub struct FileSystemReader;

impl ResourceReader for FileSystemReader {
    fn read(&self, path: &Path) -> io::Result<ResourceData> {
        let (bytes, fingerprint) = read_fingerprinted(path)?;
        Ok(ResourceData {
            bytes: bytes.into(),
            fingerprint: ResourceFingerprint::File(fingerprint),
        })
    }
}

/// Captured external inputs belonging to one lowering result.
///
/// Bytes and failures are retained so consumers interpret external source spans
/// against the same version used by the compiler. This is a per-path snapshot,
/// not an atomic snapshot of the whole filesystem.
#[derive(Debug, Default)]
pub struct ResourceSnapshot {
    entries: BTreeMap<PathBuf, io::Result<ResourceData>>,
}

impl ResourceSnapshot {
    /// Look up a captured read. `None` means the path was never requested.
    #[must_use]
    pub fn get(&self, path: &Path) -> Option<Result<&[u8], &io::Error>> {
        self.entries
            .get(path)
            .map(|result| result.as_ref().map(|data| data.bytes.as_ref()))
    }

    pub(crate) fn read(
        &mut self,
        reader: &dyn ResourceReader,
        path: &Path,
    ) -> Result<&[u8], &io::Error> {
        self.entries
            .entry(path.to_path_buf())
            .or_insert_with(|| reader.read(path))
            .as_ref()
            .map(|data| data.bytes.as_ref())
    }

    pub(crate) fn dependencies(&self) -> Vec<ExternalDependency> {
        self.entries
            .iter()
            .map(|(path, result)| ExternalDependency {
                path: path.clone(),
                fingerprint: result.as_ref().ok().map(|data| data.fingerprint),
            })
            .collect()
    }
}

/// One external file a lowering depended on: an `#image` / `#figure` raster or
/// a `#bibliography` source.
///
/// `fingerprint` is `None` when the reader failed. Successful reads carry
/// a content hash and, for the filesystem adapter, file freshness metadata.
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
    /// The resource fingerprint, or `None` when it could not be read.
    pub fingerprint: Option<ResourceFingerprint>,
}

impl ExternalDependency {
    /// Record `path` with the fingerprint it has right now.
    #[must_use]
    pub fn observe(path: impl Into<PathBuf>) -> Self {
        let path = path.into();
        let fingerprint = fingerprint_file(&path).map(ResourceFingerprint::File);
        Self { path, fingerprint }
    }

    /// Whether the file on disk still matches the recorded fingerprint.
    /// Caller-supplied content fingerprints conservatively return false; this
    /// method checks the filesystem, not a custom reader or overlay.
    ///
    /// A `stat` settles the common cases: a file whose size, modification
    /// time, and [`FileIdentity`] are unchanged is current, and a file that
    /// appeared or vanished is not. The bytes are re-read and hashed only when
    /// the `stat` differs or the fingerprint [is racy](Fingerprint::is_racy),
    /// so a `touch` that leaves the contents alone still counts as current,
    /// while a same-size rewrite that restores the old mtime is caught by the
    /// inode identity on Unix and by the racy window everywhere.
    #[must_use]
    pub fn is_current(&self) -> bool {
        let Some(recorded) = &self.fingerprint else {
            return regular_file_metadata(&self.path).is_none();
        };
        let ResourceFingerprint::File(recorded) = recorded else {
            return false;
        };
        let Some(metadata) = regular_file_metadata(&self.path) else {
            return false;
        };
        if recorded.stat_matches(&metadata) && !recorded.is_racy() {
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
    let observed = SystemTime::now();
    let bytes = std::fs::read(path)?;
    let fingerprint = Fingerprint {
        len: metadata.len(),
        modified: metadata.modified().ok(),
        identity: FileIdentity::of(&metadata),
        observed,
        content: fingerprint_bytes(&bytes),
    };
    Ok((bytes, fingerprint))
}

fn regular_file_metadata(path: &Path) -> Option<Metadata> {
    std::fs::metadata(path).ok().filter(Metadata::is_file)
}

/// Resource access for a directive, using the enclosing lowering's snapshot.
pub(crate) struct ExternalInputs<'a> {
    pub(crate) source_file: &'a Path,
    pub(crate) reader: &'a dyn ResourceReader,
    pub(crate) resources: &'a mut ResourceSnapshot,
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
        let recorded = fingerprint_file(&path).expect("fingerprint");
        let moved = ExternalDependency {
            path: path.clone(),
            fingerprint: Some(ResourceFingerprint::File(Fingerprint {
                modified: Some(SystemTime::UNIX_EPOCH),
                ..recorded
            })),
        };
        assert!(moved.is_current(), "a stat mismatch falls back to the hash");
        let stale = ExternalDependency {
            path: path.clone(),
            fingerprint: Some(ResourceFingerprint::File(Fingerprint {
                modified: Some(SystemTime::UNIX_EPOCH),
                content: fingerprint_bytes(b"other"),
                ..recorded
            })),
        };
        assert!(!stale.is_current());
        cleanup(&path);
    }

    #[test]
    fn a_fingerprint_taken_right_after_the_write_is_racy_and_always_hashed() {
        let path = unique_temp_file("racy");
        std::fs::write(&path, b"same").expect("write");
        let recorded = fingerprint_file(&path).expect("fingerprint");
        assert!(recorded.is_racy());

        let lying = ExternalDependency {
            path: path.clone(),
            fingerprint: Some(ResourceFingerprint::File(Fingerprint {
                content: fingerprint_bytes(b"other"),
                ..recorded
            })),
        };
        assert!(
            !lying.is_current(),
            "a matching stat inside the racy window is not trusted"
        );

        let settled = Fingerprint {
            observed: recorded.observed + RACY_WINDOW * 2,
            ..recorded
        };
        assert!(!settled.is_racy());
        let trusted = ExternalDependency {
            path: path.clone(),
            fingerprint: Some(ResourceFingerprint::File(Fingerprint {
                content: fingerprint_bytes(b"other"),
                ..settled
            })),
        };
        assert!(
            trusted.is_current(),
            "outside the racy window a matching stat is proof enough"
        );
        cleanup(&path);
    }

    #[test]
    fn a_fingerprint_without_mtime_is_racy() {
        let fingerprint = Fingerprint {
            len: 0,
            modified: None,
            identity: FileIdentity::NONE,
            observed: SystemTime::UNIX_EPOCH,
            content: ContentHash(0),
        };
        assert!(fingerprint.is_racy());
    }

    #[cfg(unix)]
    #[test]
    fn same_size_replacement_that_restores_the_mtime_moves_the_identity() {
        let path = unique_temp_file("identity");
        std::fs::write(&path, b"aaaa").expect("write");
        let recorded = fingerprint_file(&path).expect("fingerprint");
        let settled = ExternalDependency {
            path: path.clone(),
            fingerprint: Some(ResourceFingerprint::File(Fingerprint {
                observed: recorded.observed + RACY_WINDOW * 2,
                ..recorded
            })),
        };
        assert!(settled.is_current());

        let staging = path.with_extension("new");
        std::fs::write(&staging, b"bbbb").expect("write replacement");
        std::fs::File::options()
            .write(true)
            .open(&staging)
            .expect("open replacement")
            .set_modified(recorded.modified.expect("mtime"))
            .expect("restore mtime");
        std::fs::rename(&staging, &path).expect("swap in");
        let metadata = std::fs::metadata(&path).expect("stat");
        assert_eq!(metadata.len(), recorded.len);
        assert_eq!(metadata.modified().ok(), recorded.modified);
        assert_ne!(FileIdentity::of(&metadata), recorded.identity);
        assert!(
            !settled.is_current(),
            "a new inode falls through the stat gate to the hash"
        );
        cleanup(&path);
    }
}
