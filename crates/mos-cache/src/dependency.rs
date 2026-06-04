//! Typed build-dependency identities (manifest §7, §32; MVP 5).
//!
//! Incremental builds need to name *what* a cached artifact depends on before
//! they can decide *whether* it is stale. This module supplies the vocabulary:
//! [`DependencyKind`] is the coarse category and [`DependencyId`] is the typed,
//! deterministic identity. Both are pure value types — there is no dirty-node
//! invalidation, content hashing, or persistent cache here. Those later slices
//! (see [`docs/incremental-dependencies.md`]) *consume* these identities.
//!
//! [`docs/incremental-dependencies.md`]: ../../../docs/incremental-dependencies.md
//!
//! # Scope
//!
//! Only inputs with a *real, stable identity today* are modelled: file-backed
//! inputs (their canonical project path, see [`ProjectPath`]) and labels (their
//! reference name). Categories the design note sketches but cannot yet identify
//! deterministically — `Node` and `Style` bundles (their ids are still
//! defaulted), packages, layout *inputs* (no real layout key until paragraph
//! hashing lands, §4.4), and layout *outputs* — are deferred until they have a
//! genuine identity scheme, rather than modelled as placeholders that would
//! collide.
//!
//! # What is intentionally not modelled yet
//!
//! - **Content boundaries.** A [`DependencyId`] names a dependency; it does not
//!   hash the bytes behind it. For bibliography inputs that pairing has landed
//!   as [`BibliographyDependency`], which couples a
//!   [`DependencyId::bibliography`] identity with a [`ContentHash`] boundary
//!   (the bytes are hashed by `mos_bib::bibliography_content_hash`). Other
//!   categories still carry identity only.
//! - **Serialization format.** [`DependencyId`] derives [`Eq`]/[`Ord`]/[`Hash`]
//!   so it can key in-memory maps and sets deterministically. The byte-exact
//!   on-disk form is deferred to the persistent-cache slice; [`Display`] is a
//!   stable, debuggable view, not the wire format.
//!
//! [`Display`]: core::fmt::Display

use std::fmt;

use mos_core::ContentHash;
use unicode_normalization::UnicodeNormalization;

/// Error returned when a path cannot be used as a project-relative dependency
/// identity.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum ProjectPathError {
    /// The path has no dependency identity after normalization.
    Empty,
    /// The path is absolute or carries a platform root/drive prefix.
    Absolute,
    /// The path climbs above the project root.
    ParentEscape,
}

impl fmt::Display for ProjectPathError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => f.write_str("project path is empty"),
            Self::Absolute => {
                f.write_str("project path must be relative; make absolute paths relative first")
            }
            Self::ParentEscape => f.write_str("project path must not escape the project root"),
        }
    }
}

impl std::error::Error for ProjectPathError {}

/// The category of a build dependency.
///
/// This is the coarse axis — "what kind of thing changed" — independent of the
/// concrete identity carried by [`DependencyId`]. Obtain it with
/// [`DependencyId::kind`].
///
/// # Examples
///
/// ```
/// use mos_cache::{DependencyId, DependencyKind};
///
/// assert_eq!(DependencyId::label("eq-euler").kind(), DependencyKind::Label);
/// assert_eq!(DependencyKind::Bibliography.as_str(), "bibliography");
/// ```
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
pub enum DependencyKind {
    /// A `.mos` source file.
    SourceFile,
    /// A referenced asset, such as an image.
    Asset,
    /// A bibliography input, such as a `.bib` file.
    Bibliography,
    /// A resolved label that references resolve against.
    Label,
}

impl DependencyKind {
    /// The stable lowercase tag used in [`DependencyId`]'s [`Display`] form.
    ///
    /// These tags are part of the debuggable identity and must stay stable.
    ///
    /// [`Display`]: core::fmt::Display
    ///
    /// # Examples
    ///
    /// ```
    /// use mos_cache::DependencyKind;
    ///
    /// assert_eq!(DependencyKind::SourceFile.as_str(), "source");
    /// assert_eq!(DependencyKind::Label.as_str(), "label");
    /// ```
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SourceFile => "source",
            Self::Asset => "asset",
            Self::Bibliography => "bibliography",
            Self::Label => "label",
        }
    }
}

impl fmt::Display for DependencyKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A canonical, project-relative resource path used as a file dependency's
/// identity.
///
/// The stored string *is* the identity, in canonical form:
///
/// - backslashes folded to `/` (so `a\b` and `a/b` agree across platforms),
/// - `.` and empty segments dropped, `..` resolved lexically,
/// - each segment NFC-normalized.
///
/// So `./a.mos`, `a.mos`, and `dir\..\dir/a.mos` all yield the same
/// `ProjectPath` — which is exactly what makes a file [`DependencyId`]
/// deterministic for the same logical input (design note §3.1).
///
/// Normalization is **lexical only**: it never touches the filesystem, so it
/// cannot turn a relative path absolute or leak machine layout into the
/// identity. Absolute filesystem paths are valid inputs at outer boundaries,
/// but they must be made project-relative before becoming a `ProjectPath`.
///
/// # Examples
///
/// ```
/// use mos_cache::ProjectPath;
///
/// assert_eq!(
///     ProjectPath::new("./ch/../ch/intro.mos").map(|path| path.as_str().to_owned()),
///     Ok("ch/intro.mos".to_owned())
/// );
/// assert_eq!(
///     ProjectPath::new(r"figures\logo.png").map(|path| path.as_str().to_owned()),
///     Ok("figures/logo.png".to_owned())
/// );
/// ```
#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
pub struct ProjectPath(String);

impl ProjectPath {
    /// Canonicalize a project-relative path into a [`ProjectPath`]. Absolute
    /// paths must be relativized against the project root before calling this.
    ///
    /// # Examples
    ///
    /// ```
    /// use mos_cache::ProjectPath;
    ///
    /// // Decomposed "é" (e + combining acute) folds to the composed form.
    /// assert_eq!(ProjectPath::new("e\u{0301}.bib"), ProjectPath::new("\u{00e9}.bib"));
    /// ```
    pub fn new(path: impl AsRef<str>) -> Result<Self, ProjectPathError> {
        normalize(path.as_ref()).map(Self)
    }

    /// The canonical path string.
    ///
    /// # Examples
    ///
    /// ```
    /// use mos_cache::ProjectPath;
    ///
    /// assert_eq!(
    ///     ProjectPath::new("a//b/").map(|path| path.as_str().to_owned()),
    ///     Ok("a/b".to_owned())
    /// );
    /// ```
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ProjectPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Lexically canonicalize a project-relative path: fold `\` to `/`, drop
/// `.`/empty segments, resolve `..`, and NFC-normalize. No filesystem access.
fn normalize(input: &str) -> Result<String, ProjectPathError> {
    let forward = input.replace('\\', "/");
    if forward.is_empty() {
        return Err(ProjectPathError::Empty);
    }
    if forward.starts_with('/') || starts_with_windows_drive(&forward) {
        return Err(ProjectPathError::Absolute);
    }
    let mut segments: Vec<&str> = Vec::new();
    for segment in forward.split('/') {
        match segment {
            "" | "." => {}
            ".." => match segments.last() {
                // Pop a real parent segment.
                Some(&last) if last != ".." => {
                    segments.pop();
                }
                _ => return Err(ProjectPathError::ParentEscape),
            },
            other => segments.push(other),
        }
    }
    let body: String = segments.join("/").nfc().collect();
    if body.is_empty() {
        return Err(ProjectPathError::Empty);
    }
    Ok(body)
}

fn starts_with_windows_drive(path: &str) -> bool {
    let mut chars = path.chars();
    matches!(
        (chars.next(), chars.next()),
        (Some(letter), Some(':')) if letter.is_ascii_alphabetic()
    )
}

/// A typed, deterministic identity for one build dependency.
///
/// Each variant carries the payload appropriate to its [`DependencyKind`], so
/// mismatched combinations cannot be constructed. File inputs use the canonical
/// [`ProjectPath`]; labels use their name. The derived [`Eq`]/[`Ord`]/[`Hash`]
/// make ids usable as keys in deterministic maps and sets; [`Display`] gives a
/// stable `kind:payload` view for logs and debugging.
///
/// [`Display`]: core::fmt::Display
///
/// # Examples
///
/// ```
/// use mos_cache::{DependencyId, DependencyKind};
///
/// # fn main() -> Result<(), mos_cache::ProjectPathError> {
/// let bib = DependencyId::bibliography("./refs.bib")?;
///
/// assert_eq!(bib.kind(), DependencyKind::Bibliography);
/// assert_eq!(bib.to_string(), "bibliography:refs.bib");
/// assert_eq!(bib.path().map(|p| p.as_str()), Some("refs.bib"));
/// # Ok(())
/// # }
/// ```
#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
pub enum DependencyId {
    /// A `.mos` source file, identified by its canonical project path.
    SourceFile(ProjectPath),
    /// A referenced asset (such as an image), identified by its canonical path.
    Asset(ProjectPath),
    /// A bibliography input (such as a `.bib` file), by its canonical path.
    Bibliography(ProjectPath),
    /// A resolved label, identified by its reference name.
    Label(String),
}

impl DependencyId {
    /// A `.mos` source-file dependency.
    ///
    /// # Examples
    ///
    /// ```
    /// use mos_cache::DependencyId;
    ///
    /// # fn main() -> Result<(), mos_cache::ProjectPathError> {
    /// assert_eq!(DependencyId::source_file("a.mos")?.to_string(), "source:a.mos");
    /// # Ok(())
    /// # }
    /// ```
    pub fn source_file(path: impl AsRef<str>) -> Result<Self, ProjectPathError> {
        ProjectPath::new(path).map(Self::SourceFile)
    }

    /// An asset dependency, such as an image.
    ///
    /// # Examples
    ///
    /// ```
    /// use mos_cache::DependencyId;
    ///
    /// # fn main() -> Result<(), mos_cache::ProjectPathError> {
    /// assert_eq!(DependencyId::asset("logo.png")?.to_string(), "asset:logo.png");
    /// # Ok(())
    /// # }
    /// ```
    pub fn asset(path: impl AsRef<str>) -> Result<Self, ProjectPathError> {
        ProjectPath::new(path).map(Self::Asset)
    }

    /// A bibliography-input dependency, such as a `.bib` file.
    ///
    /// # Examples
    ///
    /// ```
    /// use mos_cache::DependencyId;
    ///
    /// # fn main() -> Result<(), mos_cache::ProjectPathError> {
    /// assert_eq!(DependencyId::bibliography("refs.bib")?.to_string(), "bibliography:refs.bib");
    /// # Ok(())
    /// # }
    /// ```
    pub fn bibliography(path: impl AsRef<str>) -> Result<Self, ProjectPathError> {
        ProjectPath::new(path).map(Self::Bibliography)
    }

    /// A label dependency.
    ///
    /// # Examples
    ///
    /// ```
    /// use mos_cache::DependencyId;
    ///
    /// assert_eq!(DependencyId::label("eq-1").to_string(), "label:eq-1");
    /// ```
    #[must_use]
    pub fn label(name: impl Into<String>) -> Self {
        Self::Label(name.into())
    }

    /// The [`DependencyKind`] this id belongs to.
    ///
    /// # Examples
    ///
    /// ```
    /// use mos_cache::{DependencyId, DependencyKind};
    ///
    /// # fn main() -> Result<(), mos_cache::ProjectPathError> {
    /// assert_eq!(DependencyId::asset("x.png")?.kind(), DependencyKind::Asset);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub const fn kind(&self) -> DependencyKind {
        match self {
            Self::SourceFile(_) => DependencyKind::SourceFile,
            Self::Asset(_) => DependencyKind::Asset,
            Self::Bibliography(_) => DependencyKind::Bibliography,
            Self::Label(_) => DependencyKind::Label,
        }
    }

    /// The canonical path of a file-backed dependency, or [`None`] for labels.
    ///
    /// Covers the [`SourceFile`](Self::SourceFile), [`Asset`](Self::Asset), and
    /// [`Bibliography`](Self::Bibliography) variants uniformly.
    ///
    /// # Examples
    ///
    /// ```
    /// use mos_cache::DependencyId;
    ///
    /// # fn main() -> Result<(), mos_cache::ProjectPathError> {
    /// assert_eq!(DependencyId::asset("logo.png")?.path().map(|p| p.as_str()), Some("logo.png"));
    /// assert_eq!(DependencyId::label("eq-1").path(), None);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub const fn path(&self) -> Option<&ProjectPath> {
        match self {
            Self::SourceFile(path) | Self::Asset(path) | Self::Bibliography(path) => Some(path),
            Self::Label(_) => None,
        }
    }
}

impl fmt::Display for DependencyId {
    /// Renders a stable `kind:payload` view. Equality and hashing use the exact
    /// payloads, which this string faithfully reflects for file paths (already
    /// canonical) and labels.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:", self.kind())?;
        match self {
            Self::SourceFile(path) | Self::Asset(path) | Self::Bibliography(path) => {
                f.write_str(path.as_str())
            }
            Self::Label(name) => f.write_str(name),
        }
    }
}

/// A bibliography input paired with its content-hash boundary.
///
/// [`DependencyId::Bibliography`] answers *which* `.bib` file this is (its
/// canonical [`ProjectPath`]); the paired [`ContentHash`] answers *what was in
/// it* at build time. Together they are what a future incremental engine needs
/// to decide that cached citation data is stale: the id is the cache slot, the
/// content hash is the staleness check (design note §4.1, §7).
///
/// Construction guarantees the id is always the [`Bibliography`] variant, so a
/// `BibliographyDependency` cannot be built over a source/asset/label identity
/// by mistake — [`path`] and [`kind`] are therefore infallible.
///
/// The content hash is supplied by the caller rather than computed here, which
/// keeps `mos-cache` free of any bibliography-format knowledge. Produce it from
/// the source bytes with `mos_bib::bibliography_content_hash`; `mos-eval` (which
/// reads the `.bib` and already depends on both crates) is the natural wiring
/// point.
///
/// [`Bibliography`]: DependencyId::Bibliography
/// [`path`]: BibliographyDependency::path
/// [`kind`]: BibliographyDependency::kind
///
/// # Examples
///
/// ```
/// use mos_cache::{BibliographyDependency, DependencyId, DependencyKind};
/// use mos_core::ContentHash;
///
/// # fn main() -> Result<(), mos_cache::ProjectPathError> {
/// // The content hash would come from `mos_bib::bibliography_content_hash`.
/// let dep = BibliographyDependency::new("./refs.bib", ContentHash(0x1234))?;
///
/// assert_eq!(dep.kind(), DependencyKind::Bibliography);
/// assert_eq!(dep.id(), DependencyId::bibliography("refs.bib")?);
/// assert_eq!(dep.path().as_str(), "refs.bib");
/// assert_eq!(dep.content(), ContentHash(0x1234));
/// # Ok(())
/// # }
/// ```
#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
pub struct BibliographyDependency {
    path: ProjectPath,
    content: ContentHash,
}

impl BibliographyDependency {
    /// Pair a bibliography source `path` with the `content` hash of its bytes.
    ///
    /// The path is canonicalized into a [`ProjectPath`] (§3.1), so logically
    /// equal paths yield equal dependencies; an invalid path returns
    /// [`ProjectPathError`].
    ///
    /// # Examples
    ///
    /// ```
    /// use mos_cache::BibliographyDependency;
    /// use mos_core::ContentHash;
    ///
    /// // `./ch/../refs.bib` and `refs.bib` canonicalize to one identity.
    /// assert_eq!(
    ///     BibliographyDependency::new("./ch/../refs.bib", ContentHash(7)),
    ///     BibliographyDependency::new("refs.bib", ContentHash(7)),
    /// );
    /// ```
    pub fn new(path: impl AsRef<str>, content: ContentHash) -> Result<Self, ProjectPathError> {
        ProjectPath::new(path).map(|path| Self { path, content })
    }

    /// The typed dependency identity (always the [`Bibliography`] variant).
    ///
    /// [`Bibliography`]: DependencyId::Bibliography
    ///
    /// # Examples
    ///
    /// ```
    /// use mos_cache::{BibliographyDependency, DependencyId};
    /// use mos_core::ContentHash;
    ///
    /// # fn main() -> Result<(), mos_cache::ProjectPathError> {
    /// let dep = BibliographyDependency::new("refs.bib", ContentHash(1))?;
    /// assert_eq!(dep.id(), DependencyId::bibliography("refs.bib")?);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn id(&self) -> DependencyId {
        DependencyId::Bibliography(self.path.clone())
    }

    /// The canonical project path of the bibliography source.
    ///
    /// # Examples
    ///
    /// ```
    /// use mos_cache::BibliographyDependency;
    /// use mos_core::ContentHash;
    ///
    /// # fn main() -> Result<(), mos_cache::ProjectPathError> {
    /// assert_eq!(
    ///     BibliographyDependency::new("refs.bib", ContentHash(1))?.path().as_str(),
    ///     "refs.bib",
    /// );
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub const fn path(&self) -> &ProjectPath {
        &self.path
    }

    /// The content-hash boundary of the source bytes at build time.
    ///
    /// # Examples
    ///
    /// ```
    /// use mos_cache::BibliographyDependency;
    /// use mos_core::ContentHash;
    ///
    /// # fn main() -> Result<(), mos_cache::ProjectPathError> {
    /// assert_eq!(
    ///     BibliographyDependency::new("refs.bib", ContentHash(42))?.content(),
    ///     ContentHash(42),
    /// );
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub const fn content(&self) -> ContentHash {
        self.content
    }

    /// The [`DependencyKind`] of this dependency: always
    /// [`Bibliography`](DependencyKind::Bibliography).
    ///
    /// # Examples
    ///
    /// ```
    /// use mos_cache::{BibliographyDependency, DependencyKind};
    /// use mos_core::ContentHash;
    ///
    /// # fn main() -> Result<(), mos_cache::ProjectPathError> {
    /// let dep = BibliographyDependency::new("refs.bib", ContentHash(1))?;
    /// assert_eq!(dep.kind(), DependencyKind::Bibliography);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub const fn kind(&self) -> DependencyKind {
        DependencyKind::Bibliography
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeSet, HashSet};

    use mos_core::ContentHash;

    use super::{
        BibliographyDependency, DependencyId, DependencyKind, ProjectPath, ProjectPathError,
    };

    fn id_text(id: Result<DependencyId, ProjectPathError>) -> Result<String, ProjectPathError> {
        id.map(|id| id.to_string())
    }

    fn id_path_text(
        id: Result<DependencyId, ProjectPathError>,
    ) -> Result<Option<String>, ProjectPathError> {
        id.map(|id| id.path().map(ProjectPath::as_str).map(str::to_owned))
    }

    fn path_text(path: Result<ProjectPath, ProjectPathError>) -> Result<String, ProjectPathError> {
        path.map(|path| path.as_str().to_owned())
    }

    #[test]
    fn kind_matches_variant() {
        let cases = [
            (
                DependencyId::source_file("a.mos").map(|id| id.kind()),
                Ok(DependencyKind::SourceFile),
            ),
            (
                DependencyId::asset("a.png").map(|id| id.kind()),
                Ok(DependencyKind::Asset),
            ),
            (
                DependencyId::bibliography("a.bib").map(|id| id.kind()),
                Ok(DependencyKind::Bibliography),
            ),
            (
                Ok(DependencyId::label("a").kind()),
                Ok(DependencyKind::Label),
            ),
        ];
        for (actual, expected) in cases {
            assert_eq!(actual, expected);
        }
    }

    #[test]
    fn display_is_stable_per_kind() {
        assert_eq!(
            id_text(DependencyId::source_file("ch/intro.mos")),
            Ok("source:ch/intro.mos".to_owned())
        );
        assert_eq!(
            id_text(DependencyId::asset("logo.png")),
            Ok("asset:logo.png".to_owned())
        );
        assert_eq!(
            id_text(DependencyId::bibliography("refs.bib")),
            Ok("bibliography:refs.bib".to_owned())
        );
        assert_eq!(
            DependencyId::label("eq-euler").to_string(),
            "label:eq-euler"
        );
    }

    #[test]
    fn path_covers_file_variants_only() {
        assert_eq!(
            id_path_text(DependencyId::source_file("a.mos")),
            Ok(Some("a.mos".to_owned()))
        );
        assert_eq!(
            id_path_text(DependencyId::asset("a.png")),
            Ok(Some("a.png".to_owned()))
        );
        assert_eq!(
            id_path_text(DependencyId::bibliography("a.bib")),
            Ok(Some("a.bib".to_owned()))
        );
        assert_eq!(DependencyId::label("a").path(), None);
    }

    #[test]
    fn canonical_paths_collapse_to_one_identity() {
        let canonical = DependencyId::source_file("ch/intro.mos");
        for variant in [
            "./ch/intro.mos",
            "ch/./intro.mos",
            "ch/../ch/intro.mos",
            r"ch\intro.mos",
        ] {
            assert_eq!(DependencyId::source_file(variant), canonical, "{variant}");
        }
    }

    #[test]
    fn nfc_variants_share_one_identity() {
        // Decomposed vs composed "é" must hash and compare equal.
        assert_eq!(
            DependencyId::bibliography("e\u{0301}.bib"),
            DependencyId::bibliography("\u{00e9}.bib")
        );
    }

    #[test]
    fn invalid_project_paths_are_rejected() {
        assert_eq!(ProjectPath::new(""), Err(ProjectPathError::Empty));
        assert_eq!(ProjectPath::new("."), Err(ProjectPathError::Empty));
        assert_eq!(ProjectPath::new("a/.."), Err(ProjectPathError::Empty));
        assert_eq!(
            ProjectPath::new("../b"),
            Err(ProjectPathError::ParentEscape)
        );
        assert_eq!(
            ProjectPath::new("a/../../b"),
            Err(ProjectPathError::ParentEscape)
        );
        assert_eq!(ProjectPath::new("/a/b"), Err(ProjectPathError::Absolute));
        assert_eq!(ProjectPath::new(r"C:\a\b"), Err(ProjectPathError::Absolute));
    }

    #[test]
    fn canonical_path_text_is_available() {
        assert_eq!(path_text(ProjectPath::new("a//b/")), Ok("a/b".to_owned()));
    }

    #[test]
    fn equal_inputs_produce_equal_ids() {
        assert_eq!(
            DependencyId::bibliography("refs.bib"),
            DependencyId::bibliography("refs.bib")
        );
    }

    #[test]
    fn distinct_inputs_and_kinds_differ() {
        // Same path, different kind: not equal.
        assert_ne!(DependencyId::source_file("x"), DependencyId::asset("x"));
        // Same kind, different payload: not equal.
        assert_ne!(DependencyId::label("a"), DependencyId::label("b"));
    }

    #[test]
    fn ids_are_hashable_and_orderable() {
        let mut set = HashSet::new();
        assert!(set.insert(DependencyId::label("a")));
        assert!(!set.insert(DependencyId::label("a")));

        // BTreeSet exercises Ord and yields a deterministic order.
        let ordered: BTreeSet<_> = [DependencyId::label("b"), DependencyId::label("a")]
            .into_iter()
            .collect();
        let names: Vec<_> = ordered.iter().map(ToString::to_string).collect();
        assert_eq!(names, ["label:a", "label:b"]);
    }

    #[test]
    fn kind_tags_round_trip_through_display() {
        for kind in [
            DependencyKind::SourceFile,
            DependencyKind::Asset,
            DependencyKind::Bibliography,
            DependencyKind::Label,
        ] {
            assert_eq!(kind.to_string(), kind.as_str());
        }
    }

    #[test]
    fn bibliography_dependency_is_always_bibliography_kind() {
        let dep = BibliographyDependency::new("refs.bib", ContentHash(1));
        assert_eq!(dep.map(|dep| dep.kind()), Ok(DependencyKind::Bibliography));
    }

    #[test]
    fn bibliography_dependency_id_round_trips_to_bibliography_variant() {
        assert_eq!(
            BibliographyDependency::new("refs.bib", ContentHash(1)).map(|dep| dep.id()),
            DependencyId::bibliography("refs.bib"),
        );
    }

    #[test]
    fn bibliography_dependency_exposes_path_and_content() {
        let dep = BibliographyDependency::new("ch/refs.bib", ContentHash(0x99));
        assert_eq!(
            dep.as_ref().map(|dep| dep.path().as_str().to_owned()),
            Ok("ch/refs.bib".to_owned()),
        );
        assert_eq!(dep.map(|dep| dep.content()), Ok(ContentHash(0x99)));
    }

    #[test]
    fn equal_path_and_content_produce_equal_dependencies() {
        // Path canonicalization is inherited from `ProjectPath`.
        assert_eq!(
            BibliographyDependency::new("./ch/../refs.bib", ContentHash(7)),
            BibliographyDependency::new("refs.bib", ContentHash(7)),
        );
    }

    #[test]
    fn differing_content_or_path_makes_dependencies_differ() {
        // Same path, different content boundary: not equal.
        assert_ne!(
            BibliographyDependency::new("refs.bib", ContentHash(1)),
            BibliographyDependency::new("refs.bib", ContentHash(2)),
        );
        // Same content, different path: not equal.
        assert_ne!(
            BibliographyDependency::new("a.bib", ContentHash(1)),
            BibliographyDependency::new("b.bib", ContentHash(1)),
        );
    }

    #[test]
    fn bibliography_dependencies_are_hashable_and_orderable() {
        let built: Result<Vec<_>, _> = [
            ("refs.bib", ContentHash(1)),
            ("refs.bib", ContentHash(1)), // exact duplicate of the first
            ("refs.bib", ContentHash(2)), // same path, distinct content boundary
            ("b.bib", ContentHash(1)),
            ("a.bib", ContentHash(1)),
        ]
        .into_iter()
        .map(|(path, content)| BibliographyDependency::new(path, content))
        .collect();

        // HashSet dedups by value: 5 inputs, one exact duplicate -> 4 unique.
        let unique = built
            .as_ref()
            .map(|deps| deps.iter().cloned().collect::<HashSet<_>>().len());
        assert_eq!(unique, Ok(4));

        // BTreeSet exercises Ord and yields a deterministic order, sorted by
        // (path, content). Project the full key so the content tie-break is
        // actually asserted: the two refs.bib entries must order 1 before 2.
        let ordered = built.as_ref().map(|deps| {
            deps.iter()
                .cloned()
                .collect::<BTreeSet<_>>()
                .iter()
                .map(|dep| (dep.path().as_str().to_owned(), dep.content()))
                .collect::<Vec<_>>()
        });
        assert_eq!(
            ordered,
            Ok(vec![
                ("a.bib".to_owned(), ContentHash(1)),
                ("b.bib".to_owned(), ContentHash(1)),
                ("refs.bib".to_owned(), ContentHash(1)),
                ("refs.bib".to_owned(), ContentHash(2)),
            ])
        );
    }

    #[test]
    fn invalid_path_is_rejected() {
        assert_eq!(
            BibliographyDependency::new("../escape.bib", ContentHash(1)),
            Err(ProjectPathError::ParentEscape),
        );
    }
}
