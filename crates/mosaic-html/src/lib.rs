//! Semantic HTML backend (manifest §21.2).
//!
//! Output preserves document semantics (`<section>`, `<figure>`,
//! `<figcaption>`, …) rather than absolute-positioned rectangles.

use std::path::Path;

use mosaic_core::{CoreError, Result};
use mosaic_layout::PageGraph;

/// Emit a `PageGraph` as an HTML file. Stub.
pub fn emit(_graph: &PageGraph, _out: &Path) -> Result<()> {
    Err(CoreError::Unimplemented("mosaic-html::emit"))
}
