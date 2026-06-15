//! User-facing diagnostics: severities, sub-message annotations, and
//! machine-actionable fix-it suggestions.
//!
//! A [`Diagnostic`] pairs a `'static` [`DiagnosticDef`] (identity, from
//! [`crate::codes`]) with a resolved [`Severity`], a primary message and an
//! optional [`SourceSpan`], plus [`DiagnosticAnnotation`] rows and
//! [`Suggestion`] fixes.

use crate::SourceSpan;
use crate::codes::DiagnosticDef;

/// Diagnostic severity (manifest §31).
///
/// Three runtime severities. `Error` marks a *failing* diagnostic (the CLI
/// exits non-zero at the next phase barrier) — it does **not** mean "abort
/// the phase right now". `Notice` is informational and non-failing
/// (substitutions, auto-decisions). Sub-message kinds (`note`/`help`/
/// `hint`) live on [`DiagnosticAnnotation`], never here.
///
/// # Examples
///
/// ```
/// use mos_core::Severity;
///
/// assert_ne!(Severity::Error, Severity::Notice);
/// ```
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum Severity {
    /// Failing diagnostic; non-zero exit at the next phase barrier.
    Error,
    /// Surfaced, but the build continues.
    Warning,
    /// Informational only; the build continues.
    Notice,
}

/// A sub-message attached to a [`Diagnostic`].
///
/// The diagnostic's *primary* span lives on [`Diagnostic::span`]; these are
/// only secondary spans (`Related`) and textual rows. There is intentionally
/// no `Primary` variant — that would be a second home for the primary span.
///
/// # Examples
///
/// ```
/// use mos_core::DiagnosticAnnotation;
///
/// let help = DiagnosticAnnotation::Help("try `#set text(...)`".to_owned());
/// assert!(matches!(help, DiagnosticAnnotation::Help(_)));
/// ```
#[derive(Clone, Debug)]
pub enum DiagnosticAnnotation {
    /// Another source location that helps explain the primary cause
    /// (e.g. the first declaration of a duplicated label).
    Related {
        /// Where the related span points.
        span: SourceSpan,
        /// What that location contributes.
        message: String,
    },
    /// Attached explanation, rendered as `note:`.
    Note(String),
    /// Attached suggestion, rendered as `help:`.
    Help(String),
    /// Attached hint, rendered as `hint:`.
    Hint(String),
}

/// A machine-actionable fix for a [`Diagnostic`].
///
/// A `Suggestion` says "replace the bytes at this [`SourceSpan`] with this
/// text" — it is structured data a tool can apply automatically, as opposed
/// to the prose advice carried by [`DiagnosticAnnotation::Help`]. Backends
/// consume it without re-parsing: the CLI can print a fix-it diff and an LSP
/// can surface it as a code action keyed on the same span.
///
/// Two edge cases fall out of the replace-the-span model and are intentional:
///
/// - an empty `replacement` **deletes** the bytes covered by `span`;
/// - a zero-length `span` (`start == end`) **inserts** `replacement` at that
///   offset without removing anything.
///
/// # Examples
///
/// ```
/// use std::path::PathBuf;
///
/// use mos_core::{SourceSpan, Suggestion};
///
/// let span = SourceSpan::new(PathBuf::from("main.mos"), 4, 10);
/// let fix = Suggestion::new(span, "@intro");
///
/// assert_eq!(fix.replacement, "@intro");
/// ```
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Suggestion {
    /// The source range the fix replaces. A zero-length span
    /// (`start == end`) marks a pure insertion point.
    pub span: SourceSpan,
    /// The text to substitute for the bytes covered by `span`. An empty
    /// string deletes that range.
    pub replacement: String,
}

impl Suggestion {
    /// Construct a suggestion replacing `span` with `replacement`.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::path::PathBuf;
    ///
    /// use mos_core::{SourceSpan, Suggestion};
    ///
    /// let span = SourceSpan::new(PathBuf::from("main.mos"), 0, 3);
    /// let fix = Suggestion::new(span, "set".to_owned());
    ///
    /// assert_eq!(fix.span.start(), 0);
    /// ```
    #[must_use]
    pub fn new(span: SourceSpan, replacement: impl Into<String>) -> Self {
        Self {
            span,
            replacement: replacement.into(),
        }
    }
}

/// A user-facing diagnostic (manifest §16, §31).
///
/// Identity and default severity come from a `'static` [`DiagnosticDef`] in
/// [`crate::codes`]; the instance carries the *resolved* severity (today always
/// the def's default, later a config override) so rendering never has to
/// consult the def. Fields are private — construct via [`Diagnostic::simple`]
/// or [`Diagnostic::new`].
///
/// # Examples
///
/// ```
/// use mos_core::{Diagnostic, Severity, codes};
///
/// let diagnostic = Diagnostic::simple(&codes::MOS0010, None, "boom");
///
/// assert_eq!(diagnostic.severity(), Severity::Error);
/// assert_eq!(diagnostic.def().code().to_string(), "MOS0010");
/// ```
#[derive(Clone, Debug)]
pub struct Diagnostic {
    def: &'static DiagnosticDef,
    severity: Severity,
    span: Option<SourceSpan>,
    message: String,
    annotations: Vec<DiagnosticAnnotation>,
    suggestions: Vec<Suggestion>,
}

impl Diagnostic {
    /// Full constructor: the caller supplies the resolved severity. The
    /// future config resolver uses this; nothing has to crack open the
    /// struct.
    ///
    /// # Examples
    ///
    /// ```
    /// use mos_core::{Diagnostic, Severity, codes};
    ///
    /// // Promote a warning-by-default code to an error.
    /// let d = Diagnostic::new(&codes::MOS0028, Severity::Error, None, "promoted");
    /// assert_eq!(d.severity(), Severity::Error);
    /// ```
    pub fn new(
        def: &'static DiagnosticDef,
        severity: Severity,
        span: Option<SourceSpan>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            def,
            severity,
            span,
            message: message.into(),
            annotations: Vec::new(),
            suggestions: Vec::new(),
        }
    }

    /// Convenience: severity defaults to `def.default_severity()`.
    ///
    /// # Examples
    ///
    /// ```
    /// use mos_core::{Diagnostic, Severity, codes};
    ///
    /// let d = Diagnostic::simple(&codes::MOS0018, None, "substituted Noto Sans");
    /// assert_eq!(d.severity(), Severity::Notice);
    /// ```
    pub fn simple(
        def: &'static DiagnosticDef,
        span: Option<SourceSpan>,
        message: impl Into<String>,
    ) -> Self {
        Self::new(def, def.default_severity(), span, message)
    }

    /// Attach a sub-message annotation, builder-style.
    ///
    /// # Examples
    ///
    /// ```
    /// use mos_core::{Diagnostic, DiagnosticAnnotation, codes};
    ///
    /// let d = Diagnostic::simple(&codes::MOS0033, None, "unknown label")
    ///     .with_annotation(DiagnosticAnnotation::Help("did you mean `@intro`?".to_owned()));
    /// assert_eq!(d.annotations().len(), 1);
    /// ```
    #[must_use]
    pub fn with_annotation(mut self, annotation: DiagnosticAnnotation) -> Self {
        self.annotations.push(annotation);
        self
    }

    /// Attach a machine-actionable [`Suggestion`], builder-style.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::path::PathBuf;
    ///
    /// use mos_core::{Diagnostic, SourceSpan, Suggestion, codes};
    ///
    /// let span = SourceSpan::new(PathBuf::from("main.mos"), 4, 10);
    /// let d = Diagnostic::simple(&codes::MOS0033, None, "unknown label")
    ///     .with_suggestion(Suggestion::new(span, "@intro"));
    /// assert_eq!(d.suggestions().len(), 1);
    /// ```
    #[must_use]
    pub fn with_suggestion(mut self, suggestion: Suggestion) -> Self {
        self.suggestions.push(suggestion);
        self
    }

    /// Attach a span, builder-style.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::path::PathBuf;
    ///
    /// use mos_core::{Diagnostic, SourceSpan, codes};
    ///
    /// let span = SourceSpan::new(PathBuf::from("main.mos"), 4, 10);
    /// let diagnostic = Diagnostic::simple(&codes::MOS0033, None, "unknown label")
    ///     .with_span(span.clone());
    ///
    /// assert_eq!(diagnostic.span(), Some(&span));
    /// ```
    #[must_use]
    pub fn with_span(mut self, span: SourceSpan) -> Self {
        self.span = Some(span);
        self
    }

    /// The registry definition behind this diagnostic.
    ///
    /// # Examples
    ///
    /// ```
    /// use mos_core::{Diagnostic, codes};
    ///
    /// let diagnostic = Diagnostic::simple(&codes::MOS0033, None, "unknown label");
    ///
    /// assert_eq!(diagnostic.def().code(), codes::MOS0033.code());
    /// ```
    #[must_use]
    pub fn def(&self) -> &'static DiagnosticDef {
        self.def
    }

    /// The resolved severity carried by this instance.
    ///
    /// # Examples
    ///
    /// ```
    /// use mos_core::{Diagnostic, Severity, codes};
    ///
    /// let diagnostic = Diagnostic::simple(&codes::MOS0033, None, "unknown label");
    ///
    /// assert_eq!(diagnostic.severity(), Severity::Error);
    /// ```
    #[must_use]
    pub fn severity(&self) -> Severity {
        self.severity
    }

    /// The primary span, if any.
    ///
    /// # Examples
    ///
    /// ```
    /// use mos_core::{Diagnostic, codes};
    ///
    /// let diagnostic = Diagnostic::simple(&codes::MOS0033, None, "unknown label");
    ///
    /// assert!(diagnostic.span().is_none());
    /// ```
    #[must_use]
    pub fn span(&self) -> Option<&SourceSpan> {
        self.span.as_ref()
    }

    /// The primary message.
    ///
    /// # Examples
    ///
    /// ```
    /// use mos_core::{Diagnostic, codes};
    ///
    /// let diagnostic = Diagnostic::simple(&codes::MOS0033, None, "unknown label");
    ///
    /// assert_eq!(diagnostic.message(), "unknown label");
    /// ```
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }

    /// The attached sub-message annotations, in attach order.
    ///
    /// # Examples
    ///
    /// ```
    /// use mos_core::{Diagnostic, DiagnosticAnnotation, codes};
    ///
    /// let diagnostic = Diagnostic::simple(&codes::MOS0033, None, "unknown label")
    ///     .with_annotation(DiagnosticAnnotation::Help("declare `<intro>` first".to_owned()));
    ///
    /// assert_eq!(diagnostic.annotations().len(), 1);
    /// ```
    #[must_use]
    pub fn annotations(&self) -> &[DiagnosticAnnotation] {
        &self.annotations
    }

    /// The attached machine-actionable suggestions, in attach order.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::path::PathBuf;
    ///
    /// use mos_core::{Diagnostic, SourceSpan, Suggestion, codes};
    ///
    /// let span = SourceSpan::new(PathBuf::from("main.mos"), 4, 10);
    /// let diagnostic = Diagnostic::simple(&codes::MOS0033, None, "unknown label")
    ///     .with_suggestion(Suggestion::new(span, "@intro"));
    ///
    /// assert_eq!(diagnostic.suggestions().len(), 1);
    /// ```
    #[must_use]
    pub fn suggestions(&self) -> &[Suggestion] {
        &self.suggestions
    }
}

impl std::fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] {}", self.def.code(), self.message)
    }
}

impl std::error::Error for Diagnostic {}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::codes;

    #[test]
    fn suggestion_new_sets_span_and_replacement() {
        let span = SourceSpan::new(PathBuf::from("main.mos"), 4, 10);
        let suggestion = Suggestion::new(span.clone(), "@intro");
        assert_eq!(suggestion.span, span);
        assert_eq!(suggestion.replacement, "@intro");
    }

    #[test]
    fn diagnostic_has_no_suggestions_by_default() {
        let diagnostic = Diagnostic::simple(&codes::MOS0033, None, "unknown label");
        assert!(diagnostic.suggestions().is_empty());
    }

    #[test]
    fn with_suggestion_accumulates_in_order() {
        let first = Suggestion::new(SourceSpan::new(PathBuf::from("main.mos"), 4, 10), "@intro");
        let second = Suggestion::new(
            SourceSpan::new(PathBuf::from("other.mos"), 12, 15),
            "@summary",
        );
        let diagnostic = Diagnostic::simple(&codes::MOS0033, None, "unknown label")
            .with_suggestion(first)
            .with_suggestion(second);

        let suggestions = diagnostic.suggestions();
        assert_eq!(suggestions.len(), 2);

        assert_eq!(suggestions[0].span.file, PathBuf::from("main.mos"));
        assert_eq!(suggestions[0].span.start(), 4);
        assert_eq!(suggestions[0].span.end(), 10);
        assert_eq!(suggestions[0].replacement, "@intro");

        assert_eq!(suggestions[1].span.file, PathBuf::from("other.mos"));
        assert_eq!(suggestions[1].span.start(), 12);
        assert_eq!(suggestions[1].span.end(), 15);
        assert_eq!(suggestions[1].replacement, "@summary");
    }

    #[test]
    fn suggestion_new_accepts_str_and_owned_string() {
        let span = SourceSpan::new(PathBuf::from("main.mos"), 4, 10);
        let from_str = Suggestion::new(span.clone(), "@intro");
        let from_string = Suggestion::new(span, String::from("@intro"));
        assert_eq!(from_str, from_string);
    }

    #[test]
    fn suggestion_clone_and_equality() {
        let span = SourceSpan::new(PathBuf::from("main.mos"), 4, 10);
        let suggestion = Suggestion::new(span.clone(), "@intro");

        // A clone equals its original.
        assert_eq!(suggestion.clone(), suggestion);
        // Built independently from the same parts => equal.
        assert_eq!(Suggestion::new(span.clone(), "@intro"), suggestion);
        // Differing replacement text => unequal.
        assert_ne!(Suggestion::new(span, "@outro"), suggestion);
        // Differing span => unequal.
        let wider = SourceSpan::new(PathBuf::from("main.mos"), 4, 11);
        assert_ne!(Suggestion::new(wider, "@intro"), suggestion);
    }

    #[test]
    fn suggestion_empty_replacement_encodes_deletion() {
        let span = SourceSpan::new(PathBuf::from("main.mos"), 4, 10);
        let deletion = Suggestion::new(span, "");
        assert!(deletion.replacement.is_empty());
        // A deletion still covers a real, non-empty range.
        assert!(deletion.span.start() < deletion.span.end());
    }

    #[test]
    fn suggestion_zero_length_span_encodes_insertion() {
        let point = SourceSpan::new(PathBuf::from("main.mos"), 7, 7);
        let insertion = Suggestion::new(point, "@intro");
        assert_eq!(insertion.span.start(), insertion.span.end());
        assert_eq!(insertion.replacement, "@intro");
    }

    #[test]
    fn suggestions_and_annotations_are_independent_channels() {
        let span = SourceSpan::new(PathBuf::from("main.mos"), 4, 10);

        // A suggestion does not leak into the annotation channel.
        let with_fix = Diagnostic::simple(&codes::MOS0033, None, "unknown label")
            .with_suggestion(Suggestion::new(span.clone(), "@intro"));
        assert_eq!(with_fix.suggestions().len(), 1);
        assert!(with_fix.annotations().is_empty());

        // Prose help does not leak into the suggestion channel.
        let with_help = Diagnostic::simple(&codes::MOS0033, None, "unknown label").with_annotation(
            DiagnosticAnnotation::Help("did you mean `@intro`?".to_owned()),
        );
        assert_eq!(with_help.annotations().len(), 1);
        assert!(with_help.suggestions().is_empty());

        // Both channels populate independently and keep their own payloads.
        let with_both = Diagnostic::simple(&codes::MOS0033, None, "unknown label")
            .with_annotation(DiagnosticAnnotation::Help(
                "did you mean `@intro`?".to_owned(),
            ))
            .with_suggestion(Suggestion::new(span, "@intro"));
        assert_eq!(with_both.suggestions().len(), 1);
        assert_eq!(with_both.annotations().len(), 1);
        assert_eq!(with_both.suggestions()[0].replacement, "@intro");

        // The existing Help annotation is carried through unchanged.
        let help_text = match &with_both.annotations()[0] {
            DiagnosticAnnotation::Help(text) => Some(text.as_str()),
            _ => None,
        };
        assert_eq!(help_text, Some("did you mean `@intro`?"));
    }
}
