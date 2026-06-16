//! Project / package manifest for Mosaic (`mosaic.toml`, manifest §14).
//!
//! Real dependency resolution and lockfile generation land later.
//! For now this crate only defines the manifest schema and parses it.

#![doc(
    html_logo_url = "https://mosaic.kjanat.dev/assets/A4.svg",
    html_favicon_url = "https://mosaic.kjanat.dev/assets/A4.svg"
)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Parsed `mosaic.toml` project manifest.
///
/// # Examples
///
/// ```
/// use mos_packages::ProjectManifest;
///
/// let manifest: ProjectManifest = toml::from_str(
///     r#"
///     [project]
///     name = "demo"
///     version = "0.1.0"
///     entry = "main.mos"
///     "#,
/// )?;
///
/// assert_eq!(manifest.project.name, "demo");
/// # Ok::<(), toml::de::Error>(())
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectManifest {
    pub project: ProjectSection,

    #[serde(default)]
    pub document: DocumentSection,

    #[serde(default)]
    pub output: OutputSection,

    #[serde(default)]
    pub dependencies: BTreeMap<String, String>,
}

/// Required `[project]` section of `mosaic.toml`.
///
/// # Examples
///
/// ```
/// use mos_packages::ProjectSection;
///
/// let project = ProjectSection {
///     name: "demo".to_owned(),
///     version: "0.1.0".to_owned(),
///     entry: "main.mos".to_owned(),
/// };
///
/// assert_eq!(project.entry, "main.mos");
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectSection {
    pub name: String,
    pub version: String,
    pub entry: String,
}

/// Optional `[document]` defaults from `mosaic.toml`.
///
/// # Examples
///
/// ```
/// use mos_packages::DocumentSection;
///
/// let section = DocumentSection::default();
///
/// assert!(section.output.is_empty());
/// ```
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DocumentSection {
    #[serde(default)]
    pub language: Option<String>,

    #[serde(default)]
    pub output: Vec<String>,
}

/// Declared output paths from `mosaic.toml`.
///
/// Paths are interpreted by the CLI relative to the project directory.
///
/// # Examples
///
/// ```
/// use mos_packages::OutputSection;
///
/// let section = OutputSection {
///     pdf: Some("paper.pdf".to_owned()),
/// };
///
/// assert_eq!(section.pdf.as_deref(), Some("paper.pdf"));
/// ```
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OutputSection {
    #[serde(default)]
    pub pdf: Option<String>,
}

/// Error returned when loading a manifest from disk.
///
/// # Examples
///
/// ```
/// use std::path::Path;
///
/// use mos_packages::{ManifestError, ProjectManifest};
///
/// let err = ProjectManifest::load(Path::new("definitely-missing-mosaic.toml")).err();
///
/// assert!(matches!(err, Some(ManifestError::Io { .. })));
/// ```
#[derive(thiserror::Error, Debug)]
pub enum ManifestError {
    #[error("could not read manifest `{}`: {}", mos_core::display_path(.path), .source)]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("could not parse manifest `{}`: {}", mos_core::display_path(.path), .source)]
    Parse {
        path: PathBuf,
        #[source]
        source: toml::de::Error,
    },
}

impl ProjectManifest {
    /// Load and parse a `mosaic.toml` from disk.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use std::path::Path;
    ///
    /// use mos_packages::ProjectManifest;
    ///
    /// let manifest = ProjectManifest::load(Path::new("mosaic.toml"))?;
    ///
    /// assert!(!manifest.project.entry.is_empty());
    /// # Ok::<(), mos_packages::ManifestError>(())
    /// ```
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_optional_pdf_output_path() {
        let manifest: ProjectManifest = toml::from_str(
            r#"
            [project]
            name = "demo"
            version = "0.1.0"
            entry = "main.mos"

            [output]
            pdf = "demo.pdf"
            "#,
        )
        .expect("manifest parses");

        assert_eq!(manifest.output.pdf.as_deref(), Some("demo.pdf"));
    }
}
