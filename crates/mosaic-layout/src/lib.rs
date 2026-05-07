//! Layout engine for Mosaic.
//!
//! Covers inline shaping (§6 stage 5, §22.1), block layout (§6 stage 6),
//! and pagination / float solving (§6 stage 7, §22.2, §33). Boundary-state
//! reuse (manifest §22.3) is the linchpin for incremental builds.

use mosaic_core::{CoreError, NodeId, Result};

/// Absolute typographic length. The real type will support `mm`, `pt`,
/// `em`, and conversions; for now it's just a wrapper over points.
#[derive(Copy, Clone, Debug, Default, PartialEq, PartialOrd)]
pub struct Abs(pub f64);

/// A laid-out block (manifest §6 stage 6).
#[derive(Debug)]
pub enum Block {
    Paragraph,
    DisplayMath,
    Figure,
    Table,
    Heading,
    Footnote,
}

/// Cached page-boundary signature used by the incremental pagination
/// algorithm in manifest §22.3 / §33.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BoundaryState {
    pub next_block: Option<NodeId>,
    pub pending_floats: Vec<NodeId>,
    // Real counter and footnote state land in MVP 1.
}

/// A paginated output graph (manifest §6 stage 7).
#[derive(Debug, Default)]
pub struct PageGraph {
    pub pages: Vec<Page>,
}

#[derive(Debug, Default)]
pub struct Page {
    pub number: u32,
}

#[derive(Debug, Default)]
pub struct LayoutEngine;

impl LayoutEngine {
    pub fn new() -> Self {
        Self
    }

    /// Lay out a resolved document into a `PageGraph`. Stub.
    pub fn layout(&mut self) -> Result<PageGraph> {
        Err(CoreError::Unimplemented(
            "mosaic-layout::LayoutEngine::layout",
        ))
    }
}
