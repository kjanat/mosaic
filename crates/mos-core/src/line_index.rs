//! Immutable text snapshots with indexed UTF-16 source positions.

use std::sync::Arc;

const CHECKPOINT_BYTES: usize = 256;

#[derive(Clone, Copy, Debug)]
struct Checkpoint {
    byte: usize,
    column: usize,
}

#[derive(Clone, Copy, Debug)]
struct Line {
    start: usize,
    checkpoint: usize,
}

/// An immutable UTF-8 snapshot and its reusable line/UTF-16 index.
///
/// Construction takes linear time. Lookups binary-search line starts and sparse
/// checkpoints, then scan at most 256 bytes (plus one code point). The index
/// owns its text so it cannot accidentally be used with a different revision.
/// Cloning shares both text and index. No filesystem or protocol dependency.
///
/// LF starts a new line; CR is a normal character, preserving CRLF byte offsets.
/// Columns and lines are zero-based. Tabs count as one UTF-16 code unit.
///
/// ```
/// use mos_core::LineIndex;
/// let source = LineIndex::new("😀\nhello");
/// assert_eq!(source.utf16_position(4), (0, 2));
/// assert_eq!(source.byte_offset(1, 2), 7);
/// ```
#[derive(Clone, Debug)]
pub struct LineIndex {
    text: Arc<str>,
    lines: Arc<[Line]>,
    checkpoints: Arc<[Checkpoint]>,
}

impl LineIndex {
    /// Index a snapshot. Pass an `Arc<str>` to share existing text storage.
    #[must_use]
    pub fn new(text: impl Into<Arc<str>>) -> Self {
        let text = text.into();
        let mut lines = vec![Line {
            start: 0,
            checkpoint: 0,
        }];
        let mut checkpoints = Vec::new();
        let mut checkpoint_byte = 0;
        let mut column = 0;
        for (byte, character) in text.char_indices() {
            if byte - checkpoint_byte >= CHECKPOINT_BYTES {
                checkpoints.push(Checkpoint { byte, column });
                checkpoint_byte = byte;
            }
            if character == '\n' {
                let start = byte + 1;
                lines.push(Line {
                    start,
                    checkpoint: checkpoints.len(),
                });
                checkpoint_byte = start;
                column = 0;
            } else {
                column += character.len_utf16();
            }
        }
        Self {
            text,
            lines: lines.into(),
            checkpoints: checkpoints.into(),
        }
    }

    /// The exact text used to build this index.
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Convert a byte offset to a zero-based `(line, UTF-16 column)`.
    /// Offsets past EOF clamp to EOF; offsets inside UTF-8 code points round down.
    #[must_use]
    pub fn utf16_position(&self, byte: usize) -> (usize, usize) {
        let byte = self.text.floor_char_boundary(byte.min(self.text.len()));
        let line = self.lines.partition_point(|line| line.start <= byte) - 1;
        let checkpoints = self.line_checkpoints(line);
        let checkpoint = checkpoints
            .partition_point(|point| point.byte <= byte)
            .checked_sub(1)
            .map_or(
                Checkpoint {
                    byte: self.lines[line].start,
                    column: 0,
                },
                |index| checkpoints[index],
            );
        let column = checkpoint.column + self.text[checkpoint.byte..byte].encode_utf16().count();
        (line, column)
    }

    /// Convert a zero-based line and UTF-16 column to a byte offset.
    /// Missing lines clamp to EOF; columns beyond a line clamp to its LF (or EOF).
    /// A column inside a surrogate pair rounds up to the next code-point boundary.
    #[must_use]
    pub fn byte_offset(&self, line: usize, column: usize) -> usize {
        let Some(_) = self.lines.get(line) else {
            return self.text.len();
        };
        let checkpoints = self.line_checkpoints(line);
        let checkpoint = checkpoints
            .partition_point(|point| point.column <= column)
            .checked_sub(1)
            .map_or(
                Checkpoint {
                    byte: self.lines[line].start,
                    column: 0,
                },
                |index| checkpoints[index],
            );
        let mut current_column = checkpoint.column;
        for (offset, character) in self.text[checkpoint.byte..].char_indices() {
            if character == '\n' || current_column >= column {
                return checkpoint.byte + offset;
            }
            current_column += character.len_utf16();
        }
        self.text.len()
    }

    fn line_checkpoints(&self, line: usize) -> &[Checkpoint] {
        let start = self.lines[line].checkpoint;
        let end = self
            .lines
            .get(line + 1)
            .map_or(self.checkpoints.len(), |next| next.checkpoint);
        &self.checkpoints[start..end]
    }
}

impl std::ops::Deref for LineIndex {
    type Target = str;
    fn deref(&self) -> &str {
        self.text()
    }
}

impl From<String> for LineIndex {
    fn from(text: String) -> Self {
        Self::new(text)
    }
}

impl From<&str> for LineIndex {
    fn from(text: &str) -> Self {
        Self::new(text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn positions_preserve_unicode_and_newline_boundaries() {
        let source = LineIndex::new("a😀\r\n字\n");
        assert_eq!(source.utf16_position(3), (0, 1));
        assert_eq!(source.utf16_position(5), (0, 3));
        assert_eq!(source.utf16_position(6), (0, 4));
        assert_eq!(source.utf16_position(7), (1, 0));
        assert_eq!(source.utf16_position(usize::MAX), (2, 0));
        assert_eq!(source.byte_offset(0, 2), 5);
        assert_eq!(source.byte_offset(0, usize::MAX), 6);
        assert_eq!(source.byte_offset(usize::MAX, 0), source.len());
        assert_eq!(LineIndex::new("").utf16_position(1), (0, 0));
        assert_eq!(LineIndex::new("").byte_offset(0, 1), 0);
    }

    #[test]
    fn checkpoints_round_trip_long_lines_and_clones_share_storage() {
        let text = format!("{}\n{}\n", "a😀é字\t".repeat(300), "x".repeat(1025));
        let source = LineIndex::new(text);
        for (byte, _) in source.char_indices() {
            let (line, column) = source.utf16_position(byte);
            assert_eq!(source.byte_offset(line, column), byte);
        }
        let clone = source.clone();
        assert!(Arc::ptr_eq(&source.text, &clone.text));
        assert!(Arc::ptr_eq(&source.lines, &clone.lines));
        assert!(Arc::ptr_eq(&source.checkpoints, &clone.checkpoints));
    }
}
