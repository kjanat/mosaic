//! Bibliography and citation engine (manifest §12).
//!
//! Built-in CSL / BibLaTeX / BibTeX support — no separate Biber run.
//! MVP 4 lands the real implementation.

/// A single bibliography entry, keyed by citation key.
#[derive(Clone, Debug, Default)]
pub struct Bibliography {
    _private: (),
}

/// A citation reference within the document body.
#[derive(Clone, Debug)]
pub struct Citation {
    pub key: String,
}
