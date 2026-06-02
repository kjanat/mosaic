//! The local CSL style parse error type, [`CslParseError`].
//!
//! Like `mos-bib`'s `BibParseError`, [`parse_style`](crate::parse_style)
//! returns this small local error, but it bridges into the standard `mos-core`
//! diagnostics surface rather than forming a parallel pipeline:
//! [`CslParseError::to_diagnostic`] and `From<CslParseError> for CoreError` map
//! it onto the `MOS0044` code, carrying the byte offset as a span.

use std::fmt;
use std::path::PathBuf;

use mos_core::{CoreError, Diagnostic, SourceSpan, codes};

/// What went wrong while parsing a CSL style. Paired with a byte offset inside
/// a [`CslParseError`].
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum CslParseErrorKind {
    /// The input is not well-formed XML.
    MalformedXml(String),
    /// The root element is not `<style>`.
    UnexpectedRoot(String),
    /// The `<style>` root is in a namespace other than the CSL namespace.
    ForeignNamespace(String),
    /// `<style>` lacks the required `version` attribute.
    MissingVersion,
    /// `<style>` lacks the required `class` attribute.
    MissingClass,
    /// `class` is neither `in-text` nor `note`.
    UnknownClass(String),
    /// `version` is not a supported CSL style version.
    UnsupportedVersion(String),
    /// A `<macro>` lacks the required `name` attribute.
    MissingMacroName,
    /// A `<citation>` or `<bibliography>` lacks its required `<layout>`.
    MissingLayout,
    /// A `<text>` element selects none of `variable`/`macro`/`term`/`value`.
    TextWithoutSource,
    /// A `<text>` element selects more than one source attribute.
    TextWithMultipleSources,
    /// A `<choose>` has no leading `<if>` or branches in the wrong order.
    InvalidChooseOrder,
    /// A rendering element name is not part of the supported CSL subset.
    UnsupportedElement(String),
}

impl CslParseErrorKind {
    fn describe(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MalformedXml(message) => write!(f, "malformed XML: {message}"),
            Self::UnexpectedRoot(name) => {
                write!(f, "expected a <style> root element, found <{name}>")
            }
            Self::ForeignNamespace(namespace) => {
                write!(
                    f,
                    "<style> is in an unsupported namespace `{namespace}` (expected the CSL namespace or none)"
                )
            }
            Self::MissingVersion => {
                f.write_str("<style> is missing the required `version` attribute")
            }
            Self::MissingClass => f.write_str("<style> is missing the required `class` attribute"),
            Self::UnknownClass(class) => {
                write!(
                    f,
                    "unknown style class `{class}` (expected `in-text` or `note`)"
                )
            }
            Self::UnsupportedVersion(version) => {
                write!(
                    f,
                    "unsupported CSL version `{version}` (expected `1.0` or a `1.0.x` release)"
                )
            }
            Self::MissingMacroName => {
                f.write_str("<macro> is missing the required `name` attribute")
            }
            Self::MissingLayout => {
                f.write_str("<citation>/<bibliography> is missing the required <layout>")
            }
            Self::TextWithoutSource => {
                f.write_str("<text> must select a variable, macro, term, or value")
            }
            Self::TextWithMultipleSources => {
                f.write_str("<text> must select exactly one variable, macro, term, or value")
            }
            Self::InvalidChooseOrder => {
                f.write_str("<choose> must contain <if>, then <else-if>, then optional <else>")
            }
            Self::UnsupportedElement(name) => write!(f, "unsupported CSL element <{name}>"),
        }
    }
}

/// A recoverable CSL parse error: a [`CslParseErrorKind`] plus the byte offset
/// into the original input where the problem was detected.
///
/// The offset is a byte index (the same convention `mos-core` `SourceSpan`s
/// use). Use [`line_col`](Self::line_col) for a 1-based line/column pair, or
/// [`to_diagnostic`](Self::to_diagnostic) to bridge into the compiler
/// diagnostics pipeline.
///
/// # Examples
///
/// ```
/// use mos_csl::{CslParseErrorKind, parse_style};
///
/// let err = parse_style("<not-a-style/>").unwrap_err();
/// assert!(matches!(err.kind(), CslParseErrorKind::UnexpectedRoot(_)));
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CslParseError {
    kind: CslParseErrorKind,
    offset: usize,
}

impl CslParseError {
    pub(crate) const fn new(kind: CslParseErrorKind, offset: usize) -> Self {
        Self { kind, offset }
    }

    /// The kind of parse failure.
    #[must_use]
    pub const fn kind(&self) -> &CslParseErrorKind {
        &self.kind
    }

    /// The byte offset into the parsed input where the error was detected.
    #[must_use]
    pub const fn offset(&self) -> usize {
        self.offset
    }

    /// The 1-based `(line, column)` of this error within `src`.
    ///
    /// `src` must be the input passed to [`parse_style`](crate::parse_style);
    /// columns count Unicode scalar values.
    #[must_use]
    pub fn line_col(&self, src: &str) -> (usize, usize) {
        mos_core::linecol(src, self.offset)
    }

    /// Convert this error into a `mos-core` [`Diagnostic`] anchored in `file`,
    /// carrying the `MOS0044` code and a zero-width span at
    /// [`offset`](Self::offset).
    #[must_use]
    pub fn to_diagnostic(&self, file: impl Into<PathBuf>) -> Diagnostic {
        let span = SourceSpan::new(file.into(), self.offset, self.offset);
        Diagnostic::simple(&codes::MOS0044, Some(span), self.to_string())
    }
}

impl fmt::Display for CslParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "CSL parse error at byte {}: ", self.offset)?;
        self.kind.describe(f)
    }
}

impl std::error::Error for CslParseError {}

impl From<CslParseError> for CoreError {
    fn from(err: CslParseError) -> Self {
        // No source path at this boundary, so the diagnostic keeps the message
        // (which includes the byte offset) but carries no span.
        Self::Diagnostic(Box::new(Diagnostic::simple(
            &codes::MOS0044,
            None,
            err.to_string(),
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_covers_error_kinds() {
        let cases = [
            (
                CslParseErrorKind::MalformedXml("bad token".to_owned()),
                "CSL parse error at byte 7: malformed XML: bad token",
            ),
            (
                CslParseErrorKind::UnexpectedRoot("not-style".to_owned()),
                "CSL parse error at byte 7: expected a <style> root element, found <not-style>",
            ),
            (
                CslParseErrorKind::ForeignNamespace("urn:not-csl".to_owned()),
                "CSL parse error at byte 7: <style> is in an unsupported namespace `urn:not-csl` (expected the CSL namespace or none)",
            ),
            (
                CslParseErrorKind::MissingVersion,
                "CSL parse error at byte 7: <style> is missing the required `version` attribute",
            ),
            (
                CslParseErrorKind::MissingClass,
                "CSL parse error at byte 7: <style> is missing the required `class` attribute",
            ),
            (
                CslParseErrorKind::UnknownClass("weird".to_owned()),
                "CSL parse error at byte 7: unknown style class `weird` (expected `in-text` or `note`)",
            ),
            (
                CslParseErrorKind::UnsupportedVersion("1.1".to_owned()),
                "CSL parse error at byte 7: unsupported CSL version `1.1` (expected `1.0` or a `1.0.x` release)",
            ),
            (
                CslParseErrorKind::MissingMacroName,
                "CSL parse error at byte 7: <macro> is missing the required `name` attribute",
            ),
            (
                CslParseErrorKind::MissingLayout,
                "CSL parse error at byte 7: <citation>/<bibliography> is missing the required <layout>",
            ),
            (
                CslParseErrorKind::TextWithoutSource,
                "CSL parse error at byte 7: <text> must select a variable, macro, term, or value",
            ),
            (
                CslParseErrorKind::TextWithMultipleSources,
                "CSL parse error at byte 7: <text> must select exactly one variable, macro, term, or value",
            ),
            (
                CslParseErrorKind::InvalidChooseOrder,
                "CSL parse error at byte 7: <choose> must contain <if>, then <else-if>, then optional <else>",
            ),
            (
                CslParseErrorKind::UnsupportedElement("magic".to_owned()),
                "CSL parse error at byte 7: unsupported CSL element <magic>",
            ),
        ];

        for (kind, expected) in cases {
            assert_eq!(CslParseError::new(kind, 7).to_string(), expected);
        }
    }

    #[test]
    fn error_carries_offset_line_col_and_diagnostic() {
        let src = "<style version=\"1.0\" class=\"in-text\">\n  <citation/>\n</style>";
        let err = crate::parse_style(src).expect_err("citation without layout");
        assert_eq!(err.kind(), &CslParseErrorKind::MissingLayout);
        assert_eq!(err.line_col(src), (2, 3));

        let diagnostic = err.to_diagnostic("style.csl");
        assert_eq!(diagnostic.def().code().to_string(), "MOS0044");
        let span = diagnostic.span().expect("diagnostic should carry a span");
        assert_eq!(span.start, err.offset());
    }

    #[test]
    fn from_error_yields_core_diagnostic() {
        let err = CslParseError::new(CslParseErrorKind::MissingClass, 3);
        let code = match CoreError::from(err) {
            CoreError::Diagnostic(diagnostic) => diagnostic.def().code().to_string(),
            CoreError::Unimplemented(_) => String::new(),
        };
        assert_eq!(code, "MOS0044");
    }
}
