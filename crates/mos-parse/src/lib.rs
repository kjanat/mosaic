//! Parser for the Mosaic source language (`.mos`).
//!
//! See manifest §3 (language design) and §6 stages 1–2 (parse + lower).
//! Currently covers:
//!
//! - `= Heading` / `== Subheading` / `=== Subsubheading`,
//! - paragraphs (newline-joined non-empty line groups),
//! - inline `*emphasis*`, `**strong**`, and `` `inline code` ``,
//! - `#set name(...)` blocks, recorded with span and name but interpreted
//!   later by the evaluator,
//! - `#image(...)` and `#figure(...)` directives, sharing the same
//!   `key: value` body grammar as `#set` plus an optional leading
//!   positional string literal (`#image("path.png")`),
//! - raw `#pre[[...]]` and `#code[[...]]` long-bracket blocks,
//! - `<label>` attached to the preceding block (trailing on a heading or
//!   leading on a paragraph), and `@label` cross-references as inline
//!   [`InlineKind::Reference`] runs (manifest §3.3 and the MVP 1
//!   resolver),
//! - `[@key]` citations as inline [`InlineKind::Citation`] runs. Only
//!   the single-key form is recognised in this slice; bibliography
//!   loading and rendering are deferred to MVP 4.
//!
//! Anything outside that subset is preserved as text and a recoverable
//! diagnostic is emitted; the parser never panics on user input
//! (manifest §31).

#![doc(
    html_logo_url = "https://mosaic.kjanat.dev/assets/A4.svg",
    html_favicon_url = "https://mosaic.kjanat.dev/assets/A4.svg"
)]

use std::path::Path;

use mos_core::{DiagnosticResult, DiagnosticSink};

pub use syntax::{
    DirectiveKind, Inline, InlineKind, Item, LengthUnit, ListItem, ParseResult, RawBlockKind,
    RawBlockView, SetArg, SetValue, SyntaxTree,
};

mod block;
mod directive;
mod inline;
mod list;
mod parser;
mod support;
mod syntax;

use parser::Parser;

/// Parse a Mosaic source string, emitting recoverable diagnostics to
/// `sink` (manifest §6 stage 1). Returns the [`SyntaxTree`]; the parser
/// never structurally aborts, so the `Err` arm only fires if the sink
/// itself asks to stop.
///
/// # Examples
///
/// ```
/// use std::path::Path;
///
/// use mos_core::CollectingSink;
/// use mos_parse::{InlineKind, Item, parse};
///
/// let mut sink = CollectingSink::new();
/// let result = parse("= Hello\n", Path::new("main.mos"), &mut sink);
/// assert!(result.is_ok(), "parse structurally aborted: {result:?}");
/// if let Ok(tree) = result {
///
///     assert!(!sink.had_error());
///     assert!(matches!(tree.items[0], Item::Heading { .. }));
///     if let Item::Heading { inlines, .. } = &tree.items[0] {
///         assert_eq!(inlines[0].kind, InlineKind::Text);
///     }
/// }
/// ```
///
/// # Errors
///
/// Returns [`DiagnosticAbort`](mos_core::DiagnosticAbort) only if `sink`
/// asks the parse to stop; the in-tree sinks never do.
pub fn parse(
    src: &str,
    file: &Path,
    sink: &mut dyn DiagnosticSink,
) -> DiagnosticResult<SyntaxTree> {
    let ParseResult { tree, diagnostics } = Parser::new(src, file).run();
    for diagnostic in diagnostics {
        sink.emit(diagnostic)?;
    }
    Ok(tree)
}
