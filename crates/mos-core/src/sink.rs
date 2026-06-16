//! Diagnostic emission plumbing.
//!
//! Compiler phases push diagnostics into a [`DiagnosticSink`] rather than
//! returning a `Vec`. This crate ships exactly one sink, [`CollectingSink`],
//! which gathers everything and tracks whether any error-severity
//! diagnostic was seen. Rendering sinks (and any future
//! suppression/severity-resolver wrappers) live in the consumer; the CLI
//! binary owns presentation, `mos-core` owns data.
//!
//! ## `Err` means *structural abort*, not *error diagnostic*
//!
//! [`DiagnosticResult::Err`] signals that a phase cannot structurally
//! continue producing meaningful output (a fatal IO failure, a violated
//! internal invariant). It is **not** returned merely because an
//! `Error`-severity diagnostic was emitted. Ordinary error diagnostics are
//! collected and remembered via [`CollectingSink::had_error`]; the caller
//! (the CLI) enforces phase barriers by checking that between phases and
//! exiting before the next one starts. Conflating the two would unwind the
//! parser on the first error and hide every later one.

use crate::Diagnostic;
use crate::Severity;

/// A phase aborted for structural reasons (see the module docs). Carries
/// no payload: the diagnostics explaining *why* have already been emitted
/// to the sink.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct DiagnosticAbort;

/// Result of a compiler phase. `Ok(T)` on (possibly diagnostic-bearing)
/// completion; `Err(DiagnosticAbort)` only on a structural abort.
pub type DiagnosticResult<T> = Result<T, DiagnosticAbort>;

/// Receiver for diagnostics emitted during a compiler phase.
///
/// Implementors decide what to do with each [`Diagnostic`] (collect,
/// render, count). Returning `Err(DiagnosticAbort)` asks the phase to stop
/// for structural reasons; the in-tree sinks never do, so `?` on an
/// `emit` is a no-op in practice and the phase runs to completion.
pub trait DiagnosticSink {
    /// Accept one fully-built diagnostic.
    ///
    /// # Errors
    ///
    /// Returns [`DiagnosticAbort`] only if the sink wants the current
    /// phase to stop for structural reasons (not implemented by the
    /// in-tree sinks).
    fn emit(&mut self, diagnostic: Diagnostic) -> DiagnosticResult<()>;
}

/// Collects every diagnostic and remembers whether any was an error.
///
/// Used by tests and by library callers that want the full list. Always
/// returns `Ok(())` from [`emit`](DiagnosticSink::emit).
///
/// # Examples
///
/// ```
/// use mos_core::{CollectingSink, Diagnostic, DiagnosticSink, Severity, codes};
///
/// let mut sink = CollectingSink::new();
/// assert!(
///     sink.emit(Diagnostic::simple(&codes::MOS0028, None, "unterminated"))
///         .is_ok(),
///     "collecting sink must not abort"
/// );
/// assert!(
///     sink.emit(Diagnostic::simple(&codes::MOS0010, None, "missing ident"))
///         .is_ok(),
///     "collecting sink must not abort"
/// );
///
/// assert!(sink.had_error()); // MOS0010 is an error
/// assert_eq!(sink.into_diagnostics().len(), 2);
/// ```
#[derive(Default, Debug)]
pub struct CollectingSink {
    diagnostics: Vec<Diagnostic>,
    had_error: bool,
}

impl CollectingSink {
    /// A fresh, empty sink.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether any `Error`-severity diagnostic has been emitted so far.
    #[must_use]
    pub fn had_error(&self) -> bool {
        self.had_error
    }

    /// Borrow the collected diagnostics in emission order.
    #[must_use]
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    /// Consume the sink, yielding the collected diagnostics.
    #[must_use]
    pub fn into_diagnostics(self) -> Vec<Diagnostic> {
        self.diagnostics
    }
}

impl DiagnosticSink for CollectingSink {
    fn emit(&mut self, diagnostic: Diagnostic) -> DiagnosticResult<()> {
        if diagnostic.severity() == Severity::Error {
            self.had_error = true;
        }
        self.diagnostics.push(diagnostic);
        Ok(())
    }
}

/// Forwarding impl so phases can take `&mut dyn DiagnosticSink` and callers
/// can pass `&mut sink` of a concrete type through several layers.
impl<S: DiagnosticSink + ?Sized> DiagnosticSink for &mut S {
    fn emit(&mut self, diagnostic: Diagnostic) -> DiagnosticResult<()> {
        (**self).emit(diagnostic)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codes;

    #[test]
    fn collecting_sink_tracks_error_flag_and_order() {
        let mut sink = CollectingSink::new();
        assert!(!sink.had_error());

        assert!(
            sink.emit(Diagnostic::simple(&codes::MOS0018, None, "notice"))
                .is_ok(),
            "collecting sink must not abort"
        );
        assert!(!sink.had_error(), "a notice must not flip had_error");

        assert!(
            sink.emit(Diagnostic::simple(&codes::MOS0028, None, "warning"))
                .is_ok(),
            "collecting sink must not abort"
        );
        assert!(!sink.had_error(), "a warning must not flip had_error");

        assert!(
            sink.emit(Diagnostic::simple(&codes::MOS0010, None, "error"))
                .is_ok(),
            "collecting sink must not abort"
        );
        assert!(sink.had_error(), "an error must flip had_error");

        let diags = sink.into_diagnostics();
        assert_eq!(diags.len(), 3);
        assert_eq!(diags[0].def().code(), codes::MOS0018.code());
        assert_eq!(diags[2].severity(), Severity::Error);
    }
}
