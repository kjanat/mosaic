//! Language server for `.mos` files (manifest §17).
//!
//! Eventually provides diagnostics, go-to-definition for labels,
//! citation autocomplete, source ↔ PDF sync, and live preview. MVP 6.

#![doc(
    html_logo_url = "https://mosaic.kjanat.dev/assets/A4.svg",
    html_favicon_url = "https://mosaic.kjanat.dev/assets/A4.png"
)]

use mosaic_core::{CoreError, Result};

/// Run the language server on stdio. Stub.
pub fn run() -> Result<()> {
    Err(CoreError::Unimplemented("mosaic-lsp::run"))
}
