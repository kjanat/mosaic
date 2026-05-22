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
//!   resolver).
//!
//! Anything outside that subset is preserved as text and a recoverable
//! diagnostic is emitted; the parser never panics on user input
//! (manifest §31).

#![doc(
    html_logo_url = "https://mosaic.kjanat.dev/assets/A4.svg",
    html_favicon_url = "https://mosaic.kjanat.dev/assets/A4.svg"
)]

use std::path::Path;

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

/// Parse a Mosaic source string. Always returns a [`ParseResult`]; the
/// parser is recoverable per manifest §6 stage 1.
///
/// # Examples
///
/// ```
/// use std::path::Path;
///
/// use mos_parse::{InlineKind, Item, parse};
///
/// let result = parse("= Hello\n", Path::new("main.mos"));
///
/// assert!(!result.has_errors());
/// assert!(matches!(result.tree.items[0], Item::Heading { .. }));
/// let Item::Heading { inlines, .. } = &result.tree.items[0] else {
///     unreachable!();
/// };
/// assert_eq!(inlines[0].kind, InlineKind::Text);
/// ```
#[must_use]
pub fn parse(src: &str, file: &Path) -> ParseResult {
    Parser::new(src, file).run()
}
