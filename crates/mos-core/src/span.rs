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
/// assert_eq!(span.start(), 2);
/// ```
///
/// `start` and `end` are private so the `start <= end` invariant cannot be
/// violated after construction; read them through [`SourceSpan::start`],
/// [`SourceSpan::end`], or [`SourceSpan::range`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceSpan {
    /// The source file this range points into.
    pub file: PathBuf,
    /// Byte offset of the first covered byte (inclusive).
    start: usize,
    /// Byte offset one past the last covered byte (exclusive); always
    /// `>= start`.
    end: usize,
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
    /// assert_eq!(span.end(), 9);
    /// ```
    ///
    /// # Panics
    ///
    /// Panics in debug builds if `start > end`; a backwards span is a
    /// programmer error, never user input.
    #[must_use]
    pub fn new(file: PathBuf, start: usize, end: usize) -> Self {
        debug_assert!(
            start <= end,
            "SourceSpan start ({start}) must not exceed end ({end})"
        );
        Self { file, start, end }
    }

    /// Byte offset of the first covered byte (inclusive).
    #[must_use]
    pub const fn start(&self) -> usize {
        self.start
    }

    /// Byte offset one past the last covered byte (exclusive); always
    /// `>= start`.
    #[must_use]
    pub const fn end(&self) -> usize {
        self.end
    }

    /// The covered byte range, ready to slice the source text it points into.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::path::PathBuf;
    ///
    /// use mos_core::SourceSpan;
    ///
    /// let src = "let x = 1;";
    /// let span = SourceSpan::new(PathBuf::from("main.mos"), 4, 5);
    ///
    /// assert_eq!(&src[span.range()], "x");
    /// ```
    #[must_use]
    pub const fn range(&self) -> std::ops::Range<usize> {
        self.start..self.end
    }

    /// Move the start of the span to `start`, preserving `start <= end`.
    ///
    /// # Panics
    ///
    /// Panics if `start` would exceed the current `end`. Enforced in all
    /// builds so an inverted span can never escape into release.
    pub fn set_start(&mut self, start: usize) {
        assert!(
            start <= self.end,
            "SourceSpan start ({start}) must not exceed end ({})",
            self.end
        );
        self.start = start;
    }

    /// Move the end of the span to `end`, preserving `start <= end`.
    ///
    /// # Panics
    ///
    /// Panics if `end` would fall below the current `start`. Enforced in all
    /// builds so an inverted span can never escape into release.
    pub fn set_end(&mut self, end: usize) {
        assert!(
            self.start <= end,
            "SourceSpan end ({end}) must not fall below start ({})",
            self.start
        );
        self.end = end;
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
    /// assert_eq!((span.start(), span.end()), (0, 0));
    /// ```
    #[must_use]
    pub const fn placeholder(file: PathBuf) -> Self {
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
