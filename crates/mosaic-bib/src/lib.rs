//! Bibliography and citation engine (manifest §12).
//!
//! Built-in CSL / BibLaTeX / BibTeX support — no separate Biber run.
//! MVP 4 lands the real implementation.

#![doc(
    html_logo_url = "https://mosaic.kjanat.dev/assets/A4.svg",
    html_favicon_url = "https://mosaic.kjanat.dev/assets/A4.svg"
)]

/// A bibliography: the collection of entries keyed by citation key,
/// loaded from a CSL/BibLaTeX/BibTeX source. Placeholder until MVP 4.
#[derive(Clone, Debug, Default)]
pub struct Bibliography {
    _private: (),
}

/// A citation reference within the document body — a single key that
/// resolves into a `Bibliography` entry at render time.
#[derive(Clone, Debug)]
pub struct Citation {
    pub key: String,
}
