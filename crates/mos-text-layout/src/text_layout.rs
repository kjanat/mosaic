//! Source-preserving lookup over measured text segments.
//!
//! Supply segments in both source and left-to-right spatial order. A segment
//! can be a shaped cluster, a run, or an inline element. Lengths are original
//! source byte lengths; no text is normalized or shaped here. Callers own UTF-8
//! and cluster boundary validation, glyph ink bounds, and any bidirectional
//! reordering within segments. Arbitrarily reordered source ranges are not
//! supported by this index.
//!
//! Construction is O(n), storage is O(n), and lookups are O(log n). This does
//! not bound shaping work, implement eviction, or allocate lazily.

use std::{fmt, ops::Range};

/// Invalid input to [`SegmentIndex::new`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IndexError {
    /// An individual advance is negative or non-finite.
    InvalidAdvance,
    /// The accumulated byte length or advance overflowed.
    Overflow,
}

impl fmt::Display for IndexError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidAdvance => "segment advances must be finite and nonnegative",
            Self::Overflow => "segment index length or advance overflowed",
        })
    }
}

impl std::error::Error for IndexError {}

#[derive(Clone, Copy, Debug, Default)]
struct Boundary {
    offset: usize,
    position: f32,
}

/// Source and horizontal extent of one measured segment.
#[derive(Clone, Debug, PartialEq)]
pub struct Segment {
    /// Half-open original source byte range.
    pub source: Range<usize>,
    /// Half-open horizontal advance range, in caller-selected units.
    pub horizontal: Range<f32>,
}

/// Immutable prefix index of contiguous measured segments.
///
/// Zero-length and zero-advance segments are allowed. Source lookups skip
/// zero-length segments, while position lookups skip zero-advance segments at
/// that position. At shared boundaries, the following segment wins.
/// Advances accumulate as `f32`, matching typical layout coordinates; at very
/// large positions small advances may round away. This is an advance index,
/// not an ink-bounds index: callers must account for glyph overhang when culling.
#[derive(Clone, Debug)]
pub struct SegmentIndex {
    boundaries: Vec<Boundary>,
}

impl SegmentIndex {
    /// Index `(original_byte_length, measured_advance)` pairs.
    ///
    /// # Errors
    /// Returns [`IndexError`] for negative/non-finite advances or total overflow.
    ///
    /// # Examples
    /// ```
    /// use mos_text_layout::SegmentIndex;
    /// let index = SegmentIndex::new([(3, 12.0), (2, 8.0)])?;
    /// assert_eq!(index.segment_for_offset(3), Some(1));
    /// assert_eq!(index.segment_for_position(12.0), Some(1));
    /// assert_eq!(index.intersecting(11.0..13.0), 0..2);
    /// # Ok::<(), mos_text_layout::IndexError>(())
    /// ```
    pub fn new(segments: impl IntoIterator<Item = (usize, f32)>) -> Result<Self, IndexError> {
        let mut boundaries = vec![Boundary::default()];
        let mut boundary = Boundary::default();
        for (length, advance) in segments {
            if !advance.is_finite() || advance < 0.0 {
                return Err(IndexError::InvalidAdvance);
            }
            boundary.offset = boundary
                .offset
                .checked_add(length)
                .ok_or(IndexError::Overflow)?;
            boundary.position += advance;
            if !boundary.position.is_finite() {
                return Err(IndexError::Overflow);
            }
            boundaries.push(boundary);
        }
        Ok(Self { boundaries })
    }

    /// Number of indexed segments.
    #[must_use]
    pub fn len(&self) -> usize {
        self.boundaries.len() - 1
    }

    /// Whether there are no segments.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Retrieve a segment's source and horizontal extents.
    #[must_use]
    pub fn segment(&self, index: usize) -> Option<Segment> {
        let start = self.boundaries.get(index)?;
        let end = self.boundaries.get(index.checked_add(1)?)?;
        Some(Segment {
            source: start.offset..end.offset,
            horizontal: start.position..end.position,
        })
    }

    /// Find the segment containing a source byte offset; end-of-source is `None`.
    #[must_use]
    pub fn segment_for_offset(&self, offset: usize) -> Option<usize> {
        let index = self.boundaries[1..].partition_point(|end| end.offset <= offset);
        (index < self.len()).then_some(index)
    }

    /// Find the segment containing a horizontal position.
    ///
    /// Finite negative positions select the first segment, for hit-testing to
    /// the left of a line. Non-finite positions and the line end return `None`.
    #[must_use]
    pub fn segment_for_position(&self, position: f32) -> Option<usize> {
        if !position.is_finite() {
            return None;
        }
        let index = self.boundaries[1..].partition_point(|end| end.position <= position);
        (index < self.len()).then_some(index)
    }

    /// Smallest contiguous segment range overlapping a half-open viewport.
    ///
    /// May include zero-advance segments between overlapping segments. Empty,
    /// reversed, non-finite, or non-overlapping viewports return `0..0`.
    /// This tests advances, not glyph ink; expand the viewport for overhang.
    #[must_use]
    pub fn intersecting(&self, viewport: Range<f32>) -> Range<usize> {
        if !viewport.start.is_finite()
            || !viewport.end.is_finite()
            || viewport.start >= viewport.end
        {
            return 0..0;
        }
        let start = self.boundaries[1..].partition_point(|end| end.position <= viewport.start);
        let end =
            self.boundaries[..self.len()].partition_point(|start| start.position < viewport.end);
        if start >= end { 0..0 } else { start..end }
    }
}

#[cfg(test)]
#[allow(
    clippy::panic_in_result_fn,
    reason = "tests propagate setup errors and assert behavior"
)]
mod tests {
    use super::{IndexError, SegmentIndex};

    #[test]
    fn preserves_source_lengths_and_shared_boundaries() -> Result<(), IndexError> {
        // e + combining acute retains three original UTF-8 bytes, not NFC's two.
        let index = SegmentIndex::new([("e\u{301}".len(), 7.0), ("🦀".len(), 14.0), (1, 5.0)])?;
        assert_eq!(index.segment_for_offset(2), Some(0));
        assert_eq!(index.segment_for_offset(3), Some(1));
        assert_eq!(index.segment_for_offset(8), None);
        assert_eq!(index.segment_for_offset(usize::MAX), None);
        assert_eq!(index.segment_for_position(-1.0), Some(0));
        assert_eq!(index.segment_for_position(7.0), Some(1));
        assert_eq!(index.segment_for_position(26.0), None);
        assert_eq!(index.segment(1).map(|segment| segment.source), Some(3..7));
        assert_eq!(index.segment(usize::MAX), None);
        assert_eq!(index.intersecting(7.0..21.0), 1..2);
        assert_eq!(index.intersecting(-10.0..1.0), 0..1);
        Ok(())
    }

    #[test]
    fn empty_and_degenerate_segments() -> Result<(), IndexError> {
        let empty = SegmentIndex::new([])?;
        assert!(empty.is_empty());
        assert_eq!(empty.segment_for_offset(0), None);
        assert_eq!(empty.intersecting(0.0..10.0), 0..0);
        let index = SegmentIndex::new([(0, 0.0), (3, 0.0), (0, 2.0), (1, 3.0), (0, 0.0)])?;
        assert_eq!(index.segment_for_offset(0), Some(1));
        assert_eq!(index.segment_for_offset(3), Some(3));
        assert_eq!(index.segment_for_position(0.0), Some(2));
        assert_eq!(index.segment_for_position(2.0), Some(3));
        assert_eq!(index.intersecting(0.0..5.0), 2..4);
        Ok(())
    }

    #[test]
    fn rejects_invalid_geometry_and_overflow() {
        for advance in [-1.0, f32::NAN, f32::INFINITY] {
            assert_eq!(
                SegmentIndex::new([(1, advance)]).err(),
                Some(IndexError::InvalidAdvance)
            );
        }
        assert_eq!(
            SegmentIndex::new([(usize::MAX, 0.0), (1, 0.0)]).err(),
            Some(IndexError::Overflow)
        );
        assert_eq!(
            SegmentIndex::new([(1, f32::MAX), (1, f32::MAX)]).err(),
            Some(IndexError::Overflow)
        );
    }

    #[test]
    fn matches_linear_lookup_for_many_fragments() -> Result<(), IndexError> {
        let segments: Vec<_> = (0..100_000)
            .map(|index| (index % 5, if index % 3 == 0 { 0.0 } else { 0.5 }))
            .collect();
        let index = SegmentIndex::new(segments.iter().copied())?;
        for offset in [0, 1, 1024, 65_535, 199_999, 200_000] {
            let mut end = 0;
            let expected = segments.iter().position(|(length, _)| {
                end += length;
                offset < end
            });
            assert_eq!(index.segment_for_offset(offset), expected);
        }
        for position in [-1.0, 0.0, 0.5, 1024.0, 32_000.0, 40_000.0] {
            let mut end = 0.0;
            let expected = segments.iter().position(|(_, width)| {
                end += width;
                position < end
            });
            assert_eq!(index.segment_for_position(position), expected);
        }
        for viewport in [
            0.0..0.0,
            10.0..9.0,
            -5.0..-1.0,
            0.5..10.0,
            20_000.0..20_050.0,
            40_000.0..50_000.0,
        ] {
            let mut position = 0.0;
            let mut matches = Vec::new();
            for (segment, (_, width)) in segments.iter().enumerate() {
                let end = position + width;
                if viewport.start < viewport.end && end > viewport.start && position < viewport.end
                {
                    matches.push(segment);
                }
                position = end;
            }
            let expected = matches
                .first()
                .zip(matches.last())
                .map_or(0..0, |(first, last)| *first..last + 1);
            assert_eq!(index.intersecting(viewport), expected);
        }
        assert_eq!(index.segment_for_position(f32::NAN), None);
        assert_eq!(index.intersecting(0.0..f32::INFINITY), 0..0);
        Ok(())
    }
}
