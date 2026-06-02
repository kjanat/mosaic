//! The local BibTeX parse error type, [`BibParseError`], and its
//! [`BibParseErrorKind`] classification.
//!
//! It is intentionally a local error rather than a `mos-core`
//! `CoreError`/`Diagnostic`: this crate owns no `mos-core` diagnostic code,
//! and a small self-contained error keeps the public API independent of the
//! diagnostics surface. The byte offset and
//! [`line_col`](BibParseError::line_col) bridge make it cheap to convert into
//! a compiler diagnostic when citation resolution lands.

use std::fmt;

/// What went wrong while parsing BibTeX. Paired with a byte offset inside a
/// [`BibParseError`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum BibParseErrorKind {
    /// An entry must begin with `@`.
    ExpectedAt,
    /// `@` must be followed by an entry type (e.g. `article`).
    ExpectedEntryType,
    /// The entry type must be followed by `{`.
    ExpectedOpenBrace,
    /// `{` must be followed by a non-empty citation key.
    ExpectedKey,
    /// A field must begin with a field name.
    ExpectedFieldName,
    /// A field name must be followed by `=`.
    ExpectedEquals,
    /// `=` must be followed by a `{...}`, `"..."`, or bare value.
    ExpectedValue,
    /// A field value must be followed by `,` or the closing `}`.
    ExpectedCommaOrCloseBrace,
    /// The entry ended (a `}` was expected) before the input did.
    UnterminatedEntry,
    /// A `{...}` or `"..."` value had no closing delimiter.
    UnterminatedValue,
}

impl BibParseErrorKind {
    /// A short, human-readable description of this error kind.
    #[must_use]
    pub const fn message(self) -> &'static str {
        match self {
            Self::ExpectedAt => "expected '@' to start an entry",
            Self::ExpectedEntryType => "expected an entry type after '@'",
            Self::ExpectedOpenBrace => "expected '{' after the entry type",
            Self::ExpectedKey => "expected a citation key",
            Self::ExpectedFieldName => "expected a field name",
            Self::ExpectedEquals => "expected '=' after the field name",
            Self::ExpectedValue => "expected a field value",
            Self::ExpectedCommaOrCloseBrace => "expected ',' or '}'",
            Self::UnterminatedEntry => "unterminated entry: expected '}' before end of input",
            Self::UnterminatedValue => "unterminated value: missing closing '}' or '\"'",
        }
    }
}

/// A recoverable BibTeX parse error: a [`BibParseErrorKind`] plus the byte
/// offset into the original input where the problem was detected.
///
/// The offset is a byte index, matching the convention `mos-core`
/// `SourceSpan`s use, so a future citation slice can turn one of these into a
/// compiler `Diagnostic` without re-deriving positions. Use
/// [`line_col`](Self::line_col) for a 1-based line/column pair.
///
/// # Examples
///
/// ```
/// use mos_bib::{BibParseErrorKind, parse_bibtex};
///
/// let err = parse_bibtex("article{x}").unwrap_err();
/// assert_eq!(err.kind(), BibParseErrorKind::ExpectedAt);
/// assert_eq!(err.offset(), 0);
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BibParseError {
    kind: BibParseErrorKind,
    offset: usize,
}

impl BibParseError {
    /// Construct an error of `kind` at byte `offset`. Crate-internal: the
    /// parser is the only place that mints these.
    pub(crate) const fn new(kind: BibParseErrorKind, offset: usize) -> Self {
        Self { kind, offset }
    }

    /// The kind of parse failure.
    #[must_use]
    pub const fn kind(&self) -> BibParseErrorKind {
        self.kind
    }

    /// The byte offset into the parsed input where the error was detected.
    #[must_use]
    pub const fn offset(&self) -> usize {
        self.offset
    }

    /// The 1-based `(line, column)` of this error within `src`.
    ///
    /// `src` must be the input passed to [`parse_bibtex`](crate::parse_bibtex);
    /// columns count Unicode scalar values. This is the bridge to a future
    /// `mos-core` `Diagnostic`.
    ///
    /// # Examples
    ///
    /// ```
    /// use mos_bib::parse_bibtex;
    ///
    /// let src = "@article{ok}\n@bad";
    /// let err = parse_bibtex(src).unwrap_err();
    /// assert_eq!(err.line_col(src), (2, 5));
    /// ```
    #[must_use]
    pub fn line_col(&self, src: &str) -> (usize, usize) {
        mos_core::linecol(src, self.offset)
    }
}

impl fmt::Display for BibParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "BibTeX parse error at byte {}: {}",
            self.offset,
            self.kind.message()
        )
    }
}

impl std::error::Error for BibParseError {}
