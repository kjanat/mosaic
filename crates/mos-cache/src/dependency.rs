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
//! Only inputs with a *real identity today* are modelled: file-backed inputs
//! (their project-relative path) and labels (their reference name). Categories
//! the design note sketches but cannot yet identify — `Node`, `Style` bundles,
//! resolved references, packages — are deferred until they have a stable
//! identity scheme, rather than modelled as defaulted placeholders.
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
use std::path::{Path, PathBuf};

use mos_core::StyleId;

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
    /// A layout input, identified by the resolved style bundle it belongs to.
    LayoutInput,
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
    /// assert_eq!(DependencyKind::LayoutInput.as_str(), "layout");
    /// ```
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SourceFile => "source",
            Self::Asset => "asset",
            Self::Bibliography => "bibliography",
            Self::Label => "label",
            Self::LayoutInput => "layout",
        }
    }
}

impl fmt::Display for DependencyKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A typed, deterministic identity for one build dependency.
///
/// Each variant carries the payload appropriate to its [`DependencyKind`], so
/// mismatched combinations (a bibliography id holding a style, say) cannot be
/// constructed. The payloads are strong types already — a [`PathBuf`] for file
/// inputs, a label name, a [`StyleId`] — so no extra wrappers are needed; the
/// variant tag supplies the role. The derived [`Eq`]/[`Ord`]/[`Hash`] make ids
/// usable as keys in deterministic maps and sets; [`Display`] gives a stable
/// `kind:payload` view for logs and debugging.
///
/// File-input path normalization (absolute vs relative, separator casing) is
/// the caller's responsibility — equal ids must come from equal payloads.
///
/// [`Display`]: core::fmt::Display
///
/// # Examples
///
/// ```
/// use mos_cache::{DependencyId, DependencyKind};
///
/// let bib = DependencyId::bibliography("refs.bib");
///
/// assert_eq!(bib.kind(), DependencyKind::Bibliography);
/// assert_eq!(bib.to_string(), "bibliography:refs.bib");
/// assert_eq!(bib.path(), Some(std::path::Path::new("refs.bib")));
/// ```
#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
pub enum DependencyId {
    /// A `.mos` source file, identified by its path.
    SourceFile(PathBuf),
    /// A referenced asset (such as an image), identified by its path.
    Asset(PathBuf),
    /// A bibliography input (such as a `.bib` file), identified by its path.
    Bibliography(PathBuf),
    /// A resolved label, identified by its reference name.
    Label(String),
    /// A layout input, identified by its resolved style bundle.
    ///
    /// `StyleId` is defaulted everywhere today; this is the reserved slot the
    /// future style resolver and page geometry will key off.
    LayoutInput(StyleId),
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
    pub fn source_file(path: impl Into<PathBuf>) -> Self {
        Self::SourceFile(path.into())
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
    pub fn asset(path: impl Into<PathBuf>) -> Self {
        Self::Asset(path.into())
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
    pub fn bibliography(path: impl Into<PathBuf>) -> Self {
        Self::Bibliography(path.into())
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

    /// A layout-input dependency for a resolved style bundle.
    ///
    /// # Examples
    ///
    /// ```
    /// use mos_cache::DependencyId;
    /// use mos_core::StyleId;
    ///
    /// assert_eq!(DependencyId::layout_input(StyleId(2)).to_string(), "layout:style#2");
    /// ```
    #[must_use]
    pub const fn layout_input(style: StyleId) -> Self {
        Self::LayoutInput(style)
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
            Self::LayoutInput(_) => DependencyKind::LayoutInput,
        }
    }

    /// The path of a file-backed dependency, or [`None`] for non-file kinds.
    ///
    /// Covers the [`SourceFile`](Self::SourceFile), [`Asset`](Self::Asset), and
    /// [`Bibliography`](Self::Bibliography) variants uniformly.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::path::Path;
    ///
    /// use mos_cache::DependencyId;
    /// use mos_core::StyleId;
    ///
    /// assert_eq!(DependencyId::asset("logo.png").path(), Some(Path::new("logo.png")));
    /// assert_eq!(DependencyId::label("eq-1").path(), None);
    /// assert_eq!(DependencyId::layout_input(StyleId(0)).path(), None);
    /// ```
    #[must_use]
    pub fn path(&self) -> Option<&Path> {
        match self {
            Self::SourceFile(path) | Self::Asset(path) | Self::Bibliography(path) => Some(path),
            Self::Label(_) | Self::LayoutInput(_) => None,
        }
    }
}

impl fmt::Display for DependencyId {
    /// Renders a stable `kind:payload` view. Paths use lossy display, so this
    /// is a debug/log form — equality and hashing use the exact payloads, not
    /// this string.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:", self.kind())?;
        match self {
            Self::SourceFile(path) | Self::Asset(path) | Self::Bibliography(path) => {
                write!(f, "{}", path.display())
            }
            Self::Label(name) => f.write_str(name),
            Self::LayoutInput(style) => write!(f, "style#{}", style.0),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeSet, HashSet};
    use std::path::Path;

    use mos_core::StyleId;

    use super::{DependencyId, DependencyKind};

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
            (
                DependencyId::layout_input(StyleId(1)),
                DependencyKind::LayoutInput,
            ),
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
        assert_eq!(
            DependencyId::layout_input(StyleId(3)).to_string(),
            "layout:style#3"
        );
    }

    #[test]
    fn path_covers_file_variants_only() {
        assert_eq!(
            DependencyId::source_file("a.mos").path(),
            Some(Path::new("a.mos"))
        );
        assert_eq!(
            DependencyId::asset("a.png").path(),
            Some(Path::new("a.png"))
        );
        assert_eq!(
            DependencyId::bibliography("a.bib").path(),
            Some(Path::new("a.bib"))
        );
        assert_eq!(DependencyId::label("a").path(), None);
        assert_eq!(DependencyId::layout_input(StyleId(0)).path(), None);
    }

    #[test]
    fn equal_inputs_produce_equal_ids() {
        assert_eq!(
            DependencyId::bibliography("refs.bib"),
            DependencyId::bibliography("refs.bib")
        );
        assert_eq!(
            DependencyId::layout_input(StyleId(2)),
            DependencyId::layout_input(StyleId(2))
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
            DependencyKind::LayoutInput,
        ] {
            assert_eq!(kind.to_string(), kind.as_str());
        }
    }
}
