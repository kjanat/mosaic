//! Project / package manifest for Mosaic (`mosaic.toml`, manifest §14).
//!
//! Real dependency resolution and lockfile generation land later.
//! For now this crate only defines the manifest schema and parses it.

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectManifest {
    pub project: ProjectSection,

    #[serde(default)]
    pub document: DocumentSection,

    #[serde(default)]
    pub dependencies: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectSection {
    pub name: String,
    pub version: String,
    pub entry: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DocumentSection {
    #[serde(default)]
    pub language: Option<String>,

    #[serde(default)]
    pub output: Vec<String>,
}

#[derive(thiserror::Error, Debug)]
pub enum ManifestError {
    #[error("could not read manifest: {0}")]
    Io(#[from] std::io::Error),

    #[error("could not parse manifest: {0}")]
    Parse(#[from] toml::de::Error),
}

impl ProjectManifest {
    /// Load and parse a `mosaic.toml` from disk.
    pub fn load(path: &Path) -> Result<Self, ManifestError> {
        let text = std::fs::read_to_string(path)?;
        Ok(toml::from_str(&text)?)
    }
}
