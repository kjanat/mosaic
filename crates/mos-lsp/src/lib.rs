//! Language server for `.mos` files (manifest §17).
//!
//! The current slice publishes the same parse / lower / resolve
//! diagnostics that `mos check` renders, but over the Language Server
//! Protocol so editors can show them inline. Future slices add
//! go-to-definition for labels, citation autocomplete, source ↔ PDF
//! sync, and live preview. MVP 6.

#![doc(
    html_logo_url = "https://mosaic.kjanat.dev/assets/A4.svg",
    html_favicon_url = "https://mosaic.kjanat.dev/assets/A4.svg"
)]

mod diagnostics;
mod server;

pub use diagnostics::{
    LspDiagnostic, LspPosition, LspRange, byte_to_position, diagnostics_for_document,
    path_from_uri, span_to_range,
};
pub use server::{LspError, Result, run, serve};
