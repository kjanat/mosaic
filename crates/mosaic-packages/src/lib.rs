//! Project / package manifest for Mosaic (`mosaic.toml`, manifest §14).
//!
//! Real dependency resolution and lockfile generation land later.
//! For now this crate only defines the manifest schema and parses it.

#![doc(
    html_logo_url = "https://mosaic.kjanat.dev/assets/A4.svg",
    html_favicon_url = "https://mosaic.kjanat.dev/assets/A4.png"
)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectManifest {
    pub project: ProjectSection,

    #[serde(default)]
    pub document: DocumentSection,

    #[serde(default)]
    pub dependencies: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectSection {
    pub name: String,
    pub version: String,
    pub entry: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DocumentSection {
    #[serde(default)]
    pub language: Option<String>,

    #[serde(default)]
    pub output: Vec<String>,
}

#[derive(thiserror::Error, Debug)]
pub enum ManifestError {
    #[error("could not read manifest `{path}`: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("could not parse manifest `{path}`: {source}")]
    Parse {
        path: PathBuf,
        #[source]
        source: toml::de::Error,
    },
}

impl ProjectManifest {
    /// Load and parse a `mosaic.toml` from disk.
    pub fn load(path: &Path) -> Result<Self, ManifestError> {
        let text = std::fs::read_to_string(path).map_err(|source| ManifestError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        toml::from_str(&text).map_err(|source| ManifestError::Parse {
            path: path.to_path_buf(),
            source,
        })
    }
}
