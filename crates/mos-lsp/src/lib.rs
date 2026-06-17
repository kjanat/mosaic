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
    html_logo_url = "https://mosaic.kjanat.dev/assets/A4.svg",
    html_favicon_url = "https://mosaic.kjanat.dev/assets/A4.svg"
)]

mod cache;
mod code_action;
mod definition;
mod diagnostics;
mod document_symbol;
mod rename;
mod server;

pub use definition::{definition_range, definition_range_in, position_to_byte};
pub use diagnostics::{
    LspDiagnostic, LspPosition, LspRange, byte_to_position, diagnostics_for_document,
    path_from_uri, span_to_range,
};
pub use rename::rename_ranges;
pub use server::{LspError, Result, run, serve};
