//! Font discovery, shaping, and metrics (manifest §22.1).
//!
//! Real shaping will go through HarfBuzz or `rustybuzz` per the manifest:
//! "Do not invent font shaping unless the goal is lifelong suffering."

/// In-process font database. The MVP-2 version will hold loaded font
/// files and cached metrics keyed by `font_set_hash` (manifest §32).
#[derive(Default, Debug)]
pub struct FontDb {
    _private: (),
}

impl FontDb {
    pub fn new() -> Self {
        Self::default()
    }
}
