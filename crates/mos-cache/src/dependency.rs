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
//! - **Content boundaries.** An id names a dependency; it does not hash the
//!   bytes behind it. Bibliography content hashing arrives with its own slice,
//!   built on [`DependencyId::bibliography`].
//! - **Serialization format.** [`DependencyId`] derives [`Eq`]/[`Ord`]/[`Hash`]
//!   so it can key in-memory maps and sets deterministically. The byte-exact
//!   on-disk form is deferred to the persistent-cache slice; [`Display`] is a
//!   stable, debuggable view, not the wire format.
//!
//! [`Display`]: core::fmt::Display

use std::fmt;

use unicode_normalization::UnicodeNormalization;

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
/// So `./a.mos`, `a.mos`, and `dir\..\a.mos` all yield the same `ProjectPath` —
/// which is exactly what makes a file [`DependencyId`] deterministic for the
/// same logical input (design note §3.1).
///
/// Normalization is **lexical only**: it never touches the filesystem, so it
/// cannot turn a relative path absolute or leak machine layout into the
/// identity. Supplying a project-relative path is the caller's contract; an
/// absolute path stays absolute and simply forms a different identity.
///
/// # Examples
///
/// ```
/// use mos_cache::ProjectPath;
///
/// assert_eq!(ProjectPath::new("./ch/../ch/intro.mos").as_str(), "ch/intro.mos");
/// assert_eq!(ProjectPath::new(r"figures\logo.png").as_str(), "figures/logo.png");
/// ```
#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
pub struct ProjectPath(String);

impl ProjectPath {
    /// Canonicalize a project-relative path into a [`ProjectPath`].
    ///
    /// # Examples
    ///
    /// ```
    /// use mos_cache::ProjectPath;
    ///
    /// // Decomposed "é" (e + combining acute) folds to the composed form.
    /// assert_eq!(ProjectPath::new("e\u{0301}.bib"), ProjectPath::new("\u{00e9}.bib"));
    /// ```
    pub fn new(path: impl AsRef<str>) -> Self {
        Self(normalize(path.as_ref()))
    }

    /// The canonical path string.
    ///
    /// # Examples
    ///
    /// ```
    /// use mos_cache::ProjectPath;
    ///
    /// assert_eq!(ProjectPath::new("a//b/").as_str(), "a/b");
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

/// Lexically canonicalize a project path: fold `\` to `/`, drop `.`/empty
/// segments, resolve `..`, and NFC-normalize. No filesystem access.
fn normalize(input: &str) -> String {
    let forward = input.replace('\\', "/");
    let is_absolute = forward.starts_with('/');
    let mut segments: Vec<&str> = Vec::new();
    for segment in forward.split('/') {
        match segment {
            "" | "." => {}
            ".." => match segments.last() {
                // Pop a real parent segment.
                Some(&last) if last != ".." => {
                    segments.pop();
                }
                // Relative path climbing past its start keeps the `..`;
                // an absolute path at root simply drops it.
                _ if !is_absolute => segments.push(".."),
                _ => {}
            },
            other => segments.push(other),
        }
    }
    let body: String = segments.join("/").nfc().collect();
    if is_absolute {
        format!("/{body}")
    } else {
        body
    }
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
/// let bib = DependencyId::bibliography("./refs.bib");
///
/// assert_eq!(bib.kind(), DependencyKind::Bibliography);
/// assert_eq!(bib.to_string(), "bibliography:refs.bib");
/// assert_eq!(bib.path().map(|p| p.as_str()), Some("refs.bib"));
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
    /// assert_eq!(DependencyId::source_file("a.mos").to_string(), "source:a.mos");
    /// ```
    pub fn source_file(path: impl AsRef<str>) -> Self {
        Self::SourceFile(ProjectPath::new(path))
    }

    /// An asset dependency, such as an image.
    ///
    /// # Examples
    ///
    /// ```
    /// use mos_cache::DependencyId;
    ///
    /// assert_eq!(DependencyId::asset("logo.png").to_string(), "asset:logo.png");
    /// ```
    pub fn asset(path: impl AsRef<str>) -> Self {
        Self::Asset(ProjectPath::new(path))
    }

    /// A bibliography-input dependency, such as a `.bib` file.
    ///
    /// # Examples
    ///
    /// ```
    /// use mos_cache::DependencyId;
    ///
    /// assert_eq!(DependencyId::bibliography("refs.bib").to_string(), "bibliography:refs.bib");
    /// ```
    pub fn bibliography(path: impl AsRef<str>) -> Self {
        Self::Bibliography(ProjectPath::new(path))
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
    /// assert_eq!(DependencyId::asset("x.png").kind(), DependencyKind::Asset);
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
    /// assert_eq!(DependencyId::asset("logo.png").path().map(|p| p.as_str()), Some("logo.png"));
    /// assert_eq!(DependencyId::label("eq-1").path(), None);
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

#[cfg(test)]
mod tests {
    use std::collections::{BTreeSet, HashSet};

    use super::{DependencyId, DependencyKind, ProjectPath};

    #[test]
    fn kind_matches_variant() {
        let cases = [
            (
                DependencyId::source_file("a.mos"),
                DependencyKind::SourceFile,
            ),
            (DependencyId::asset("a.png"), DependencyKind::Asset),
            (
                DependencyId::bibliography("a.bib"),
                DependencyKind::Bibliography,
            ),
            (DependencyId::label("a"), DependencyKind::Label),
        ];
        for (id, kind) in cases {
            assert_eq!(id.kind(), kind);
        }
    }

    #[test]
    fn display_is_stable_per_kind() {
        assert_eq!(
            DependencyId::source_file("ch/intro.mos").to_string(),
            "source:ch/intro.mos"
        );
        assert_eq!(
            DependencyId::asset("logo.png").to_string(),
            "asset:logo.png"
        );
        assert_eq!(
            DependencyId::bibliography("refs.bib").to_string(),
            "bibliography:refs.bib"
        );
        assert_eq!(
            DependencyId::label("eq-euler").to_string(),
            "label:eq-euler"
        );
    }

    #[test]
    fn path_covers_file_variants_only() {
        assert_eq!(
            DependencyId::source_file("a.mos")
                .path()
                .map(ProjectPath::as_str),
            Some("a.mos")
        );
        assert_eq!(
            DependencyId::asset("a.png").path().map(ProjectPath::as_str),
            Some("a.png")
        );
        assert_eq!(
            DependencyId::bibliography("a.bib")
                .path()
                .map(ProjectPath::as_str),
            Some("a.bib")
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
    fn relative_climb_is_preserved_absolute_root_is_clamped() {
        assert_eq!(ProjectPath::new("a/../../b").as_str(), "../b");
        assert_eq!(ProjectPath::new("/a/../../b").as_str(), "/b");
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
}
