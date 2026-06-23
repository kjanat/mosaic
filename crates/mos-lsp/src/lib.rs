//! Language server for `.mos` files (manifest §17).
//!
//! The current slice publishes the same parse / lower / resolve
//! diagnostics that `mos check` renders, but over the Language Server
//! Protocol so editors can show them inline, answers
//! `textDocument/definition` to jump from an `@label` reference to its
//! declaration or from a resolved `[@key]` citation to the BibTeX key,
//! and answers `textDocument/rename` to rewrite a label across its
//! declaration and references. It also exposes compiler suggestions
//! through `textDocument/codeAction`. Future slices add citation
//! autocomplete, source ↔ PDF sync, and live preview. MVP 6.

#![doc(
    html_logo_url = "https://mosaiclang.dev/assets/A4.svg",
    html_favicon_url = "https://mosaiclang.dev/assets/A4.svg"
)]

#[doc(hidden)]
pub mod cache;
#[doc(hidden)]
pub mod code_action;
#[doc(hidden)]
pub mod definition;
#[doc(hidden)]
pub mod diagnostics;
#[doc(hidden)]
pub mod document_symbol;
#[doc(hidden)]
pub mod rename;
#[doc(hidden)]
pub mod server;

pub use definition::{
    position_to_byte, range as definition_range, range_in as definition_range_in,
};
pub use diagnostics::{
    LspDiagnostic, LspPosition, LspRange, byte_to_position,
    for_document as diagnostics_for_document, path_from_uri, span_to_range,
};
pub use rename::ranges as rename_ranges;
pub use server::{LspError, Result, run, serve};
