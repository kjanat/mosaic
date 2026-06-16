//! The local BibTeX parse error type, [`BibParseError`], and its
//! [`BibParseErrorKind`] classification.
//!
//! [`parse_bibtex`](crate::parse_bibtex) returns this small local error (per
//! issue #66) so the parser stays self-contained, but it is **not** a parallel
//! bad-document pipeline: it bridges into the standard `mos-core` diagnostics
//! surface. [`BibParseError::to_diagnostic`] and `From<BibParseError> for
//! CoreError` map it onto the `MOS0043` code: carrying the byte offset as a
//! span, so a malformed `.bib` flows through the same `Diagnostic` path as
//! every other compiler error, without callers special-casing `mos-bib`.

use std::fmt;
use std::path::PathBuf;

use mos_core::{CoreError, Diagnostic, SourceSpan, codes};

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
    /// A citation key was declared more than once in the same BibTeX input.
    DuplicateKey,
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
            Self::DuplicateKey => "duplicate citation key",
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
    /// columns count Unicode scalar values. Use [`to_diagnostic`](Self::to_diagnostic)
    /// or `From<BibParseError> for CoreError` to bridge into `mos-core`
    /// diagnostics.
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

    /// Convert this error into a `mos-core` [`Diagnostic`] anchored in `file`.
    ///
    /// The diagnostic carries the `MOS0043` code and a zero-width
    /// [`SourceSpan`] at [`offset`](Self::offset), so a malformed `.bib`
    /// reported here renders through the standard compiler pipeline. The
    /// infallible `From<BibParseError> for CoreError` conversion is the
    /// span-less equivalent for boundaries without a source path.
    ///
    /// # Examples
    ///
    /// ```
    /// use mos_bib::parse_bibtex;
    ///
    /// let err = parse_bibtex("oops").unwrap_err();
    /// let diagnostic = err.to_diagnostic("refs.bib");
    /// assert_eq!(diagnostic.def().code().to_string(), "MOS0043");
    /// ```
    #[must_use]
    pub fn to_diagnostic(&self, file: impl Into<PathBuf>) -> Diagnostic {
        let span = SourceSpan::new(file.into(), self.offset, self.offset);
        Diagnostic::simple(&codes::MOS0043, Some(span), self.kind.message())
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

impl From<BibParseError> for CoreError {
    fn from(err: BibParseError) -> Self {
        // No source path at this boundary, so the diagnostic keeps the message
        // (which includes the byte offset) but carries no span.
        Self::Diagnostic(Box::new(Diagnostic::simple(
            &codes::MOS0043,
            None,
            err.to_string(),
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse_bibtex;

    #[test]
    fn from_bib_parse_error_yields_core_diagnostic() {
        let err = parse_bibtex("nope").expect_err("malformed input should be rejected");
        // Exhaustive match (no catch-all panic): the wrong variant yields an
        // empty code so the assertion fails with a clear diff.
        let code = match CoreError::from(err) {
            CoreError::Diagnostic(diagnostic) => diagnostic.def().code().to_string(),
            CoreError::Unimplemented(_) => String::new(),
        };
        assert_eq!(code, "MOS0043");
    }
}
