use std::{error::Error, fmt};

/// Errors returned by [`crate::parse`]. Line numbers are 1-based.
///
/// # Examples
///
/// ```
/// use adobe_font_metrics::{ParseError, parse};
///
/// let err = parse("").err();
///
/// assert!(matches!(err, Some(ParseError::MissingHeader { line: 1 })));
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    /// A byte is outside the supported AFM ASCII text repertoire.
    InvalidByte {
        /// Zero-based byte offset in the input.
        offset: usize,
        /// One-based source line number.
        line: usize,
        /// The rejected byte.
        value: u8,
    },
    /// First non-blank, non-comment line was not `StartFontMetrics`.
    MissingHeader {
        /// 1-based source line number where the missing header was expected.
        line: usize,
    },
    /// `StartFontMetrics` declared a version outside the 4.x family.
    UnsupportedVersion {
        /// 1-based source line number of the offending `StartFontMetrics`.
        line: usize,
        /// Version literal that was rejected (e.g. `"5.0"`).
        version: String,
    },
    /// A required field is missing: `FontName`, `FontBBox`, or `EscChar`
    /// when `MappingScheme` is 3.
    MissingRequiredField {
        /// Name of the missing field.
        field: &'static str,
    },
    /// A token that should have parsed as a number didn't.
    InvalidNumber {
        /// 1-based source line number where parsing failed.
        line: usize,
        /// Logical field whose value couldn't be parsed (e.g. `"FontBBox"`).
        field: &'static str,
        /// The raw token that failed to parse.
        value: String,
    },
    /// A record was structurally malformed (wrong arity, unrecognised
    /// boolean, etc.).
    MalformedRecord {
        /// 1-based source line number where the record appeared.
        line: usize,
        /// AFM keyword that introduced the record.
        keyword: &'static str,
        /// Human-readable description of how the record was malformed.
        reason: &'static str,
    },
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidByte {
                offset,
                line,
                value,
            } => {
                write!(
                    f,
                    "line {line}: invalid AFM byte {value:#04x} at offset {offset}"
                )
            }
            Self::MissingHeader { line } => {
                write!(f, "line {line}: expected StartFontMetrics header")
            }
            Self::UnsupportedVersion { line, version } => {
                write!(
                    f,
                    "line {line}: unsupported AFM version {version:?} (need 4.x)"
                )
            }
            Self::MissingRequiredField { field } => {
                write!(f, "missing required field {field}")
            }
            Self::InvalidNumber { line, field, value } => {
                write!(f, "line {line}: invalid number {value:?} for {field}")
            }
            Self::MalformedRecord {
                line,
                keyword,
                reason,
            } => {
                write!(f, "line {line}: malformed {keyword} record: {reason}")
            }
        }
    }
}

impl Error for ParseError {}
