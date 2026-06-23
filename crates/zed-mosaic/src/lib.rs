//! Zed extension entrypoint for Mosaic language support.

#![doc(
    html_logo_url = "https://mosaiclang.dev/assets/A4.svg",
    html_favicon_url = "https://mosaiclang.dev/assets/A4.svg"
)]

use std::fs;

use zed_extension_api::settings::LspSettings;
use zed_extension_api::{
    self as zed, Architecture, DownloadedFileType, LanguageServerId,
    LanguageServerInstallationStatus, Os, Result,
};

/// Settings key and language-server id (`lsp."mos-lsp"` in Zed settings).
const SERVER_ID: &str = "mos-lsp";
/// Executable name looked up on `PATH` and shipped in release archives.
const SERVER_BINARY: &str = "mos-lsp";
/// GitHub repository (`<owner>/<repo>`) whose releases carry `mos-lsp` assets.
const RELEASE_REPO: &str = "kjanat/mosaic";

#[derive(Debug, Default)]
struct MosaicExtension {
    /// Path to a previously downloaded `mos-lsp`, reused across requests so the
    /// extension does not re-hit the GitHub API once a release binary is on disk.
    cached_binary_path: Option<String>,
}

/// The release archive that matches the current platform, plus how to unpack it.
///
/// Asset names follow the `taiki-e/upload-rust-binary-action` layout produced by
/// `.github/workflows/release-binaries.yml`: `mos-lsp-<target-triple>.tar.gz` on
/// Unix and `mos-lsp-<target-triple>.zip` on Windows, with the binary at the
/// archive root.
struct ReleaseAsset {
    /// File name of the asset on the GitHub release (e.g.
    /// `mos-lsp-x86_64-unknown-linux-gnu.tar.gz`).
    archive_name: String,
    /// Binary file name inside the extracted archive (`mos-lsp` / `mos-lsp.exe`).
    binary_name: String,
    /// Archive format Zed should extract.
    file_type: DownloadedFileType,
}

impl ReleaseAsset {
    /// Resolve the release asset for the host Zed reports via `current_platform`.
    fn for_current_platform() -> Result<Self> {
        let (os, arch) = zed::current_platform();
        let arch = match arch {
            Architecture::Aarch64 => "aarch64",
            Architecture::X8664 => "x86_64",
            Architecture::X86 => {
                return Err(format!("`{SERVER_BINARY}` has no 32-bit x86 release build"));
            }
        };
        let (vendor_os, file_type, extension, binary_name) = match os {
            Os::Mac => (
                "apple-darwin",
                DownloadedFileType::GzipTar,
                "tar.gz",
                SERVER_BINARY.to_owned(),
            ),
            Os::Linux => (
                "unknown-linux-gnu",
                DownloadedFileType::GzipTar,
                "tar.gz",
                SERVER_BINARY.to_owned(),
            ),
            Os::Windows => (
                "pc-windows-msvc",
                DownloadedFileType::Zip,
                "zip",
                format!("{SERVER_BINARY}.exe"),
            ),
        };
        let target = format!("{arch}-{vendor_os}");
        Ok(Self {
            archive_name: format!("{SERVER_BINARY}-{target}.{extension}"),
            binary_name,
            file_type,
        })
    }
}

impl MosaicExtension {
    /// Resolve the `mos-lsp` command and its arguments for `worktree`.
    ///
    /// Discovery order:
    /// 1. `lsp."mos-lsp".binary.path` from Zed settings (explicit override).
    /// 2. `mos-lsp` on `PATH` (e.g. installed via `cargo mosils`).
    /// 3. A `mos-lsp` already downloaded from a GitHub release and still on disk.
    /// 4. Download the matching release asset from `kjanat/mosaic` and cache it.
    ///
    /// `binary.arguments` from settings override the default (none); `mos-lsp`
    /// itself takes no arguments and speaks LSP over stdio.
    fn language_server_binary(
        &mut self,
        language_server_id: &LanguageServerId,
        worktree: &zed::Worktree,
    ) -> Result<(String, Vec<String>)> {
        let settings_binary = LspSettings::for_worktree(SERVER_ID, worktree)
            .ok()
            .and_then(|settings| settings.binary);
        let args = settings_binary
            .as_ref()
            .and_then(|binary| binary.arguments.clone())
            .unwrap_or_default();

        if let Some(path) = settings_binary.and_then(|binary| binary.path) {
            return Ok((path, args));
        }

        if let Some(path) = worktree.which(SERVER_BINARY) {
            return Ok((path, args));
        }

        if let Some(path) = self.cached_binary_path.clone()
            && fs::metadata(&path).is_ok_and(|stat| stat.is_file())
        {
            return Ok((path, args));
        }

        let path = self.download_release_binary(language_server_id)?;
        Ok((path, args))
    }

    /// Download the latest `mos-lsp` release asset for this platform, extract it
    /// into a versioned directory, and return the binary path.
    ///
    /// Re-downloads only when the expected binary is missing, so an extension
    /// update that keeps the same release version reuses the on-disk copy.
    fn download_release_binary(&mut self, language_server_id: &LanguageServerId) -> Result<String> {
        zed::set_language_server_installation_status(
            language_server_id,
            &LanguageServerInstallationStatus::CheckingForUpdate,
        );

        let release = zed::latest_github_release(
            RELEASE_REPO,
            zed::GithubReleaseOptions {
                require_assets: true,
                pre_release: false,
            },
        )?;

        let asset = ReleaseAsset::for_current_platform()?;
        let release_asset = release
            .assets
            .iter()
            .find(|candidate| candidate.name == asset.archive_name)
            .ok_or_else(|| {
                format!(
                    "the latest `{RELEASE_REPO}` release ({}) has no `{}` asset for this platform",
                    release.version, asset.archive_name
                )
            })?;

        let install_dir = format!("{SERVER_BINARY}-{}", release.version);
        let binary_path = format!("{install_dir}/{}", asset.binary_name);

        if !fs::metadata(&binary_path).is_ok_and(|stat| stat.is_file()) {
            zed::set_language_server_installation_status(
                language_server_id,
                &LanguageServerInstallationStatus::Downloading,
            );
            zed::download_file(&release_asset.download_url, &install_dir, asset.file_type)
                .map_err(|err| format!("failed to download `{}`: {err}", asset.archive_name))?;
            zed::make_file_executable(&binary_path)?;
        }

        self.cached_binary_path = Some(binary_path.clone());
        Ok(binary_path)
    }
}

impl zed::Extension for MosaicExtension {
    fn new() -> Self {
        Self::default()
    }

    fn language_server_command(
        &mut self,
        language_server_id: &LanguageServerId,
        worktree: &zed::Worktree,
    ) -> Result<zed::Command> {
        let (command, args) = self.language_server_binary(language_server_id, worktree)?;
        Ok(zed::Command {
            command,
            args,
            env: worktree.shell_env(),
        })
    }

    fn language_server_initialization_options(
        &mut self,
        language_server_id: &LanguageServerId,
        worktree: &zed::Worktree,
    ) -> Result<Option<zed::serde_json::Value>> {
        Ok(
            LspSettings::for_worktree(language_server_id.as_ref(), worktree)
                .ok()
                .and_then(|settings| settings.initialization_options),
        )
    }

    fn language_server_workspace_configuration(
        &mut self,
        language_server_id: &LanguageServerId,
        worktree: &zed::Worktree,
    ) -> Result<Option<zed::serde_json::Value>> {
        Ok(
            LspSettings::for_worktree(language_server_id.as_ref(), worktree)
                .ok()
                .and_then(|settings| settings.settings),
        )
    }
}

mod registration {
    use super::MosaicExtension;

    zed_extension_api::register_extension!(MosaicExtension);
}
