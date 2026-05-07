//! Expression and scripting evaluator (manifest §4, §25).
//!
//! Lowers a parsed `SyntaxTree` into the typed semantic document graph
//! defined by `mosaic-core`. The evaluator is deliberately *not* a full
//! programming language — see manifest §4 ("no arbitrary macro hell").

use mosaic_core::{CoreError, Result};
use mosaic_parse::SyntaxTree;

/// The semantic document graph produced after lowering and resolution
/// (manifest §6 stages 2–3). Real fields land in MVP 0.
#[derive(Default, Debug)]
pub struct DocumentGraph {
    _private: (),
}

#[derive(Default, Debug)]
pub struct Evaluator;

impl Evaluator {
    pub fn new() -> Self {
        Self
    }

    pub fn evaluate(&self, _tree: &SyntaxTree) -> Result<DocumentGraph> {
        Err(CoreError::Unimplemented("mosaic-eval::Evaluator::evaluate"))
    }
}
