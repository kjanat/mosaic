//! PDF backend (manifest §21.1).
//!
//! The eventual implementation handles font subsetting, hyperlinks,
//! bookmarks, tagged PDF, and PDF/A. For now it is a stub.

use std::path::Path;

use mosaic_core::{CoreError, Result};
use mosaic_layout::PageGraph;

/// Emit a `PageGraph` as a PDF file. Stub.
pub fn emit(_graph: &PageGraph, _out: &Path) -> Result<()> {
    Err(CoreError::Unimplemented("mosaic-pdf::emit"))
}
