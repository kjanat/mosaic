//! Portable path resolution and display helpers.
//!
//! Source documents and manifests spell relative paths with `/` (manifest
//! convention). These helpers resolve such paths to the platform separator
//! ([`resolve_relative`], [`resolve_source_path`]) and render any path back
//! with forward slashes for user-facing output ([`display_path`]). Nothing
//! here touches the filesystem; these are pure path-string operations.

use std::ffi::OsStr;
use std::path::{Component, Path, PathBuf};

/// Why a portable, `/`-separated path could not be resolved.
///
/// Resolution is infallible for well-formed manifest paths; the sole failure
/// mode is a segment that would smuggle platform path semantics past the
/// `/`-only lexical model in [`resolve_relative`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PathError {
    /// A `/`-delimited segment was not a single portable name: it held a
    /// platform separator (`\`), a drive prefix, or otherwise resolved to more
    /// than one path component, so the OS would re-split it and bypass the
    /// lexical `..` handling. The offending segment is reported verbatim.
    UnsafeSegment(String),
}

impl core::fmt::Display for PathError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::UnsafeSegment(segment) => write!(
                f,
                "path segment `{segment}` is not a portable name; manifest paths use `/` as the \
                 only separator and a segment may not contain a `\\`, a drive prefix, or an \
                 absolute root"
            ),
        }
    }
}

impl core::error::Error for PathError {}

/// Whether `segment` is a single portable filename component: it holds no
/// platform separator and parses to exactly one [`Component::Normal`] equal to
/// itself.
///
/// Manifest paths spell separators with `/` only, so a `\` (a separator on
/// Windows), a drive prefix (`C:`), or any rooted form would let the OS
/// re-split the segment and slip past the lexical `..` normalization in
/// [`resolve_relative`]. Backslash is rejected on every platform so a manifest
/// resolves identically everywhere, not only where `\` happens to be a
/// separator.
fn is_plain_name(segment: &str) -> bool {
    if segment.contains('\\') {
        return false;
    }
    let mut components = Path::new(segment).components();
    matches!(components.next(), Some(Component::Normal(name)) if name == OsStr::new(segment))
        && components.next().is_none()
}

/// Render a filesystem path for user-facing output with forward slashes on
/// every platform.
///
/// On Windows, `Path::join` appends the native `\` separator, so a path built
/// from a forward-slash input (such as a shell-glob argument like
/// `examples/foo`) comes out mixed: `examples/foo\bar.pdf`. Normalizing to `/`
/// keeps CLI output, diagnostics, and error messages consistent across
/// platforms. On Unix the platform separator is already `/`, so this is a
/// no-op.
///
/// # Examples
///
/// ```
/// use std::path::Path;
///
/// use mos_core::display_path;
///
/// // Unchanged on Unix; on Windows any `\` separators become `/`.
/// assert_eq!(display_path(Path::new("a/b/c.pdf")), "a/b/c.pdf");
/// ```
#[must_use]
pub fn display_path(path: &Path) -> String {
    path.to_string_lossy()
        .replace(std::path::MAIN_SEPARATOR, "/")
}

/// Join a portable, `/`-separated `relative` path onto `base`, using the
/// platform path separator throughout.
///
/// Source documents and manifests spell relative paths with `/` (manifest convention).
/// Joining such a string with `Path::join` directly leaves the `/` embedded on Windows,
/// producing mixed paths like `dir\sub/file`.
/// This rebuilds the relative portion component-by-component so the result is
/// uniformly native-separated. A `relative` value that is absolute *or rooted*
/// passes through unchanged; on Windows a leading `/` is rooted but
/// prefix-less, which `Path::is_absolute` alone misses, so `has_root` is
/// checked too.
///
/// Normalization is lexical and stack-based: `.` and empty (`a//b`) segments are
/// dropped; a `..` pops the last resolved *name*; a `..` that escapes a
/// **relative** base is **preserved** as a leading `..` (`proj/sub` +
/// `../../../shared/x` -> `../shared/x`), not silently swallowed; and a `..` at
/// the root of an **absolute** base is clamped (`/a` + `../../x` -> `/x`),
/// matching the OS. Nothing here touches the filesystem, so `..` is resolved
/// textually and ignores symlinks; identity-grade canonicalization (NFC, escape
/// rejection) lives in `mos_cache::ProjectPath`. Each `/`-delimited segment must
/// be a single portable name -- manifest paths use `/` only, so a segment that
/// smuggles in a platform separator (`\`), a drive prefix, or an absolute root
/// is rejected (see Errors) rather than handed to `PathBuf::push`, which would
/// let the OS re-split it past this lexical model.
///
/// # Errors
///
/// Returns [`PathError::UnsafeSegment`] when a `/`-delimited segment is not a
/// single portable component. An absolute or rooted `relative` is *not* a
/// segment and still passes through unchanged.
///
/// # Examples
///
/// ```
/// use std::path::Path;
///
/// use mos_core::resolve_relative;
///
/// assert_eq!(
///     resolve_relative(Path::new("proj"), "sub/file.txt"),
///     Ok(Path::new("proj").join("sub").join("file.txt")),
/// );
/// // `.` and `..` segments are normalized away within the base:
/// assert_eq!(
///     resolve_relative(Path::new("proj"), "sub/../assets/./logo.png"),
///     Ok(Path::new("proj").join("assets").join("logo.png")),
/// );
/// // `..` past a relative base is preserved, not eaten:
/// assert_eq!(
///     resolve_relative(Path::new("proj/sub"), "../../../shared/x"),
///     Ok(Path::new("..").join("shared").join("x")),
/// );
/// ```
pub fn resolve_relative(base: &Path, relative: &str) -> Result<PathBuf, PathError> {
    let candidate = Path::new(relative);
    if candidate.is_absolute() || candidate.has_root() {
        return Ok(candidate.to_path_buf());
    }
    // `resolved` accumulates the path at or below `base`; `ascend` counts the
    // leading `..` that escaped a relative base and must survive. `pop()` only
    // removes a real component, so when it fails we ascend (relative base) or
    // clamp at the root (absolute base) -- the `..` is never silently dropped.
    let mut resolved = base.to_path_buf();
    let mut ascend = 0usize;
    for segment in relative.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                if !resolved.pop() && !resolved.has_root() {
                    ascend += 1;
                }
            }
            other => {
                if !is_plain_name(other) {
                    return Err(PathError::UnsafeSegment(other.to_owned()));
                }
                resolved.push(other);
            }
        }
    }
    if ascend == 0 {
        return Ok(resolved);
    }
    let mut out = PathBuf::new();
    for _ in 0..ascend {
        out.push("..");
    }
    // `resolved` is the (relative) remainder below the escape point, if any.
    if !resolved.as_os_str().is_empty() {
        out.push(resolved);
    }
    Ok(out)
}

/// Resolve a portable, `/`-separated `src_path` (as written in a source file)
/// relative to the directory containing `source_file`.
///
/// A convenience wrapper over [`resolve_relative`]: the base is `source_file`'s
/// parent, or the current directory when `source_file` is a bare filename.
/// Absolute or rooted `src_path` values pass through unchanged.
///
/// # Errors
///
/// Propagates [`PathError`] from [`resolve_relative`] when `src_path` has a
/// segment that is not a single portable name.
///
/// # Examples
///
/// ```
/// use std::path::Path;
///
/// use mos_core::resolve_source_path;
///
/// assert_eq!(
///     resolve_source_path("img/a.png", Path::new("proj/main.mos")),
///     Ok(Path::new("proj").join("img").join("a.png")),
/// );
/// ```
pub fn resolve_source_path(src_path: &str, source_file: &Path) -> Result<PathBuf, PathError> {
    // `Path::parent()` returns `Some("")` (not `None`) for a bare filename like
    // `main.mos`, so the empty-component guard is required to fall back to the
    // current directory rather than joining against an empty base.
    match source_file.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => resolve_relative(parent, src_path),
        _ => resolve_relative(Path::new(""), src_path),
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{PathError, resolve_relative, resolve_source_path};

    #[test]
    fn resolve_relative_normalizes_dot_dotdot_and_empty_segments() {
        // `.`/empty are dropped and `..` is resolved lexically, so displayed
        // paths stay clean (no `proj/sub/../file`).
        assert_eq!(
            resolve_relative(Path::new("proj"), "sub/../assets/./logo.png"),
            Ok(Path::new("proj").join("assets").join("logo.png")),
        );
        assert_eq!(
            resolve_relative(Path::new("proj"), "a//b"),
            Ok(Path::new("proj").join("a").join("b")),
        );
        // `..` may ascend past the base lexically (sibling directory).
        assert_eq!(
            resolve_relative(Path::new("proj"), "../shared/x"),
            Ok(Path::new("shared").join("x")),
        );
    }

    #[test]
    fn resolve_relative_passes_absolute_and_rooted_through() {
        assert_eq!(
            resolve_relative(Path::new("proj"), "/etc/refs.bib"),
            Ok(Path::new("/etc/refs.bib").to_path_buf()),
        );
    }

    #[test]
    fn resolve_relative_preserves_excess_parents_past_a_relative_base() {
        // More `..` than the base has components: the excess survives as leading
        // `..` instead of being silently swallowed by `PathBuf::pop`.
        assert_eq!(
            resolve_relative(Path::new("proj/sub"), "../../../shared/x"),
            Ok(Path::new("..").join("shared").join("x")),
        );
        // Exhausting the base with nothing below leaves bare `..`s.
        assert_eq!(
            resolve_relative(Path::new("proj"), "../../.."),
            Ok(Path::new("..").join("..")),
        );
        // An empty base ascends from the current directory.
        assert_eq!(
            resolve_relative(Path::new(""), "../x"),
            Ok(Path::new("..").join("x")),
        );
    }

    #[test]
    fn resolve_relative_clamps_parents_at_an_absolute_root() {
        // You cannot ascend above `/`: extra `..` at the root are no-ops.
        assert_eq!(
            resolve_relative(Path::new("/a/b"), "../../../x"),
            Ok(Path::new("/x").to_path_buf()),
        );
    }

    #[test]
    fn resolve_relative_rejects_segments_that_smuggle_separators() {
        // A backslash is rejected on every platform: manifest paths are
        // `/`-only, and on Windows `\` would re-split the segment past the
        // lexical model. The first offending segment is reported verbatim.
        assert_eq!(
            resolve_relative(Path::new("proj"), "a\\b/c.png"),
            Err(PathError::UnsafeSegment("a\\b".to_owned())),
        );
        // A bare absolute is not a segment, so it still passes through.
        assert_eq!(
            resolve_relative(Path::new("proj"), "/abs/x"),
            Ok(Path::new("/abs/x").to_path_buf()),
        );
    }

    #[test]
    fn resolve_source_path_preserves_parent_past_an_empty_base() {
        // Bare filename -> empty base; `../x` must keep its leading `..`.
        assert_eq!(
            resolve_source_path("../x", Path::new("main.mos")),
            Ok(Path::new("..").join("x")),
        );
    }

    #[test]
    fn resolve_source_path_normalizes_against_source_dir_and_bare_filename() {
        assert_eq!(
            resolve_source_path("img/../logo.png", Path::new("proj/main.mos")),
            Ok(Path::new("proj").join("logo.png")),
        );
        // Bare filename: parent is `Some("")`, so the base is the current dir.
        assert_eq!(
            resolve_source_path("a/./b.png", Path::new("main.mos")),
            Ok(Path::new("a").join("b.png")),
        );
    }

    #[test]
    fn resolve_source_path_rejects_unsafe_segment() {
        assert_eq!(
            resolve_source_path("img\\scan.png", Path::new("proj/main.mos")),
            Err(PathError::UnsafeSegment("img\\scan.png".to_owned())),
        );
    }
}
