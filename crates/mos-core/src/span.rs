//! Source locations.
//!
//! A [`SourceSpan`] is a byte range in a named source file; [`linecol`]
//! converts a byte offset within source text into a 1-based `(line, column)`
//! pair for rendering.

use std::path::PathBuf;

/// A byte-range location in a source file (manifest §6 stage 1).
///
/// # Examples
///
/// ```
/// use std::path::PathBuf;
///
/// use mos_core::SourceSpan;
///
/// let span = SourceSpan::new(PathBuf::from("main.mos"), 2, 8);
///
/// assert_eq!(span.start, 2);
/// ```
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceSpan {
    pub file: PathBuf,
    pub start: usize,
    pub end: usize,
}

impl SourceSpan {
    /// Construct a span covering `start..end` in `file`.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::path::PathBuf;
    ///
    /// use mos_core::SourceSpan;
    ///
    /// let span = SourceSpan::new(PathBuf::from("main.mos"), 4, 9);
    ///
    /// assert_eq!(span.end, 9);
    /// ```
    #[must_use]
    pub fn new(file: PathBuf, start: usize, end: usize) -> Self {
        Self { file, start, end }
    }

    /// A zero-length placeholder span anchored at the start of `file`.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::path::PathBuf;
    ///
    /// use mos_core::SourceSpan;
    ///
    /// let span = SourceSpan::placeholder(PathBuf::from("main.mos"));
    ///
    /// assert_eq!((span.start, span.end), (0, 0));
    /// ```
    #[must_use]
    pub fn placeholder(file: PathBuf) -> Self {
        Self {
            file,
            start: 0,
            end: 0,
        }
    }
}

/// Convert a byte offset into a 1-based `(line, column)` pair.
///
/// `src` is treated as UTF-8; columns are counted in *Unicode scalar
/// values* (i.e. `char`s), not bytes, so a span pointing at the byte
/// after `µ` reports column 2 rather than 3. Both the returned line
/// and column are at least 1, and offsets past the end of `src` are
/// clamped to the end. Offsets that fall in the middle of a UTF-8
/// code-point round down to the start of that code-point.
///
/// # Examples
///
/// ```
/// use mos_core::linecol;
///
/// assert_eq!(linecol("a\nb", 2), (2, 1));
/// ```
#[must_use]
pub fn linecol(src: &str, byte_offset: usize) -> (usize, usize) {
    let mut clamped = byte_offset.min(src.len());
    while clamped > 0 && !src.is_char_boundary(clamped) {
        clamped -= 1;
    }
    let mut line = 1_usize;
    let mut line_start = 0_usize;
    for (i, b) in src.as_bytes().iter().enumerate().take(clamped) {
        if *b == b'\n' {
            line += 1;
            line_start = i + 1;
        }
    }
    let column = src[line_start..clamped].chars().count() + 1;
    (line, column)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linecol_handles_ascii_offsets() {
        let src = "ab\ncd\nef";
        assert_eq!(linecol(src, 0), (1, 1));
        assert_eq!(linecol(src, 1), (1, 2));
        assert_eq!(linecol(src, 2), (1, 3));
        assert_eq!(linecol(src, 3), (2, 1));
        assert_eq!(linecol(src, 6), (3, 1));
        assert_eq!(linecol(src, 7), (3, 2));
        // Past the end clamps.
        assert_eq!(linecol(src, 9999), (3, 3));
    }

    #[test]
    fn linecol_counts_chars_not_bytes() {
        // `µ` is 2 bytes in UTF-8, `字` is 3 bytes. The column for the
        // byte after them should still be 2, not 3 / 4.
        let src = "µx\n字y\n";
        assert_eq!(linecol(src, 0), (1, 1));
        assert_eq!(linecol(src, 2), (1, 2)); // after `µ`
        assert_eq!(linecol(src, 3), (1, 3)); // after `µx`
        assert_eq!(linecol(src, 4), (2, 1)); // start of line 2
        assert_eq!(linecol(src, 7), (2, 2)); // after `字`
    }

    #[test]
    fn linecol_offsets_inside_codepoints_round_down() {
        // Pointing at the second byte of `µ` should still report
        // column 1 of line 1, not panic.
        let src = "µ";
        assert_eq!(linecol(src, 1), (1, 1));
    }
}
