//! Semantic HTML backend (manifest §21.2).
//!
//! Output preserves document semantics (`<section>`, `<figure>`,
//! `<figcaption>`, …) rather than absolute-positioned rectangles.

#![doc(
    html_logo_url = "https://mosaiclang.dev/assets/A4.svg",
    html_favicon_url = "https://mosaiclang.dev/assets/A4.svg"
)]

use std::path::Path;

use mos_core::{CoreError, Result};
use mos_layout::PageGraph;

/// Emit a `PageGraph` as an HTML file. Stub.
///
/// # Errors
///
/// Always returns [`CoreError::Unimplemented`] until the HTML backend ships.
///
/// # Examples
///
/// ```no_run
/// use std::path::Path;
///
/// use mos_layout::PageGraph;
///
/// let graph = PageGraph::default();
/// let result = mos_html::emit(&graph, Path::new("build/main.html"));
///
/// assert!(result.is_err());
/// ```
pub const fn emit(_graph: &PageGraph, _out: &Path) -> Result<()> {
    Err(CoreError::Unimplemented("mos-html::emit"))
}
