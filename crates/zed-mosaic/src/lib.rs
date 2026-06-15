//! Zed extension entrypoint for Mosaic language support.

#![doc(
    html_logo_url = "https://mosaic.kjanat.dev/assets/A4.svg",
    html_favicon_url = "https://mosaic.kjanat.dev/assets/A4.svg"
)]

use zed_extension_api::settings::LspSettings;
use zed_extension_api::{self as zed, LanguageServerId, Result};

/// Settings key and language-server id (`lsp."mos-lsp"` in Zed settings).
const SERVER_ID: &str = "mos-lsp";
/// Executable name looked up on `PATH`.
const SERVER_BINARY: &str = "mos-lsp";

#[derive(Debug)]
struct MosaicExtension;

impl MosaicExtension {
    /// Resolve the `mos-lsp` command and its arguments for `worktree`.
    ///
    /// Discovery order:
    /// 1. `lsp."mos-lsp".binary.path` from Zed settings (explicit override).
    /// 2. `mos-lsp` on `PATH` (e.g. installed via `cargo mosils`).
    ///
    /// `binary.arguments` from settings override the default (none); `mos-lsp`
    /// itself takes no arguments and speaks LSP over stdio.
    fn language_server_binary(worktree: &zed::Worktree) -> Result<(String, Vec<String>)> {
        let binary = LspSettings::for_worktree(SERVER_ID, worktree)
            .ok()
            .and_then(|settings| settings.binary);
        let args = binary
            .as_ref()
            .and_then(|binary| binary.arguments.clone())
            .unwrap_or_default();

        if let Some(path) = binary.and_then(|binary| binary.path) {
            return Ok((path, args));
        }

        if let Some(path) = worktree.which(SERVER_BINARY) {
            return Ok((path, args));
        }

        Err(format!(
            "`{SERVER_BINARY}` not found. Install it with `cargo install --path crates/mos-lsp` \
             (or `cargo mosils` from the Mosaic repo), make sure it is on `PATH`, or set \
             `lsp.\"{SERVER_ID}\".binary.path` in your Zed settings."
        ))
    }
}

impl zed::Extension for MosaicExtension {
    fn new() -> Self {
        Self
    }

    fn language_server_command(
        &mut self,
        _language_server_id: &LanguageServerId,
        worktree: &zed::Worktree,
    ) -> Result<zed::Command> {
        let (command, args) = Self::language_server_binary(worktree)?;
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
