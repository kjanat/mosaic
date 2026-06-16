//! Portable path resolution and display helpers.
//!
//! Source documents and manifests spell relative paths with `/` (manifest
//! convention). These helpers resolve such paths to the platform separator
//! ([`resolve_relative`], [`resolve_source_path`]) and render any path back
//! with forward slashes for user-facing output ([`display_path`]). Nothing
//! here touches the filesystem; these are pure path-string operations.

use std::path::{Path, PathBuf};

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
/// `.` and empty (`a//b`) segments are dropped and `..` segments are resolved
/// lexically, so the result is a clean path for display (`proj/file.txt`, not
/// `proj/sub/../file.txt`). This stays filesystem-free: `..` is popped
/// textually and does not consult symlinks, so callers that need link-aware
/// resolution rely on the OS at access time, and identity/canonicalization for
/// dependency tracking lives in `mos_cache::ProjectPath`.
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
///     Path::new("proj").join("sub").join("file.txt"),
/// );
/// // `.` and `..` segments are normalized away:
/// assert_eq!(
///     resolve_relative(Path::new("proj"), "sub/../assets/./logo.png"),
///     Path::new("proj").join("assets").join("logo.png"),
/// );
/// ```
#[must_use]
pub fn resolve_relative(base: &Path, relative: &str) -> PathBuf {
    let candidate = Path::new(relative);
    if candidate.is_absolute() || candidate.has_root() {
        return candidate.to_path_buf();
    }
    let mut resolved = base.to_path_buf();
    for component in relative.split('/') {
        match component {
            "" | "." => {}
            ".." => {
                resolved.pop();
            }
            other => resolved.push(other),
        }
    }
    resolved
}

/// Resolve a portable, `/`-separated `src_path` (as written in a source file)
/// relative to the directory containing `source_file`.
///
/// A convenience wrapper over [`resolve_relative`]: the base is `source_file`'s
/// parent, or the current directory when `source_file` is a bare filename.
/// Absolute or rooted `src_path` values pass through unchanged.
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
///     Path::new("proj").join("img").join("a.png"),
/// );
/// ```
#[must_use]
pub fn resolve_source_path(src_path: &str, source_file: &Path) -> PathBuf {
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

    use super::{resolve_relative, resolve_source_path};

    #[test]
    fn resolve_relative_normalizes_dot_dotdot_and_empty_segments() {
        // `.`/empty are dropped and `..` is resolved lexically, so displayed
        // paths stay clean (no `proj/sub/../file`).
        assert_eq!(
            resolve_relative(Path::new("proj"), "sub/../assets/./logo.png"),
            Path::new("proj").join("assets").join("logo.png"),
        );
        assert_eq!(
            resolve_relative(Path::new("proj"), "a//b"),
            Path::new("proj").join("a").join("b"),
        );
        // `..` may ascend past the base lexically (sibling directory).
        assert_eq!(
            resolve_relative(Path::new("proj"), "../shared/x"),
            Path::new("shared").join("x"),
        );
    }

    #[test]
    fn resolve_relative_passes_absolute_and_rooted_through() {
        assert_eq!(
            resolve_relative(Path::new("proj"), "/etc/refs.bib"),
            Path::new("/etc/refs.bib"),
        );
    }

    #[test]
    fn resolve_source_path_normalizes_against_source_dir_and_bare_filename() {
        assert_eq!(
            resolve_source_path("img/../logo.png", Path::new("proj/main.mos")),
            Path::new("proj").join("logo.png"),
        );
        // Bare filename: parent is `Some("")`, so the base is the current dir.
        assert_eq!(
            resolve_source_path("a/./b.png", Path::new("main.mos")),
            Path::new("a").join("b.png"),
        );
    }
}
