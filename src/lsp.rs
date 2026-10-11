//! Management of the Kotlin language server binary.
//!
//! Handles downloading, caching, and locating the JetBrains Kotlin LSP
//! distribution for the current platform.

use std::fs;

use zed_extension_api::{
    self as zed, GithubReleaseOptions, Result, latest_github_release, make_file_executable,
};

use crate::utils;

/// Manages the Kotlin LSP installation.
pub struct KotlinLSP {
    /// Path to a previously resolved server binary, if any.
    cached_binary_path: Option<String>,
}

impl KotlinLSP {
    /// The language server ID, as registered in `extension.toml`.
    pub const LANGUAGE_SERVER_ID: &'static str = "kotlin-dev-lsp";

    /// Creates a new manager with no cached binary path.
    pub fn new() -> Self {
        KotlinLSP {
            cached_binary_path: None,
        }
    }

    /// Returns the path to the language server binary, downloading and
    /// extracting the server archive first if needed.
    ///
    /// The resolved path is cached for the lifetime of this manager, so later
    /// calls return immediately.
    ///
    /// # Errors
    ///
    /// Returns an error if the server archive cannot be downloaded or
    /// extracted, or the platform is unsupported.
    pub fn language_server_binary_path(
        &mut self,
        language_server_id: &zed::LanguageServerId,
    ) -> Result<String> {
        if let Some(path) = self.cached_binary_path.as_ref() {
            return Ok(path.clone());
        }

        zed::set_language_server_installation_status(
            language_server_id,
            &zed::LanguageServerInstallationStatus::CheckingForUpdate,
        );
        let version = get_version()?;

        zed::set_language_server_installation_status(
            language_server_id,
            &zed::LanguageServerInstallationStatus::Downloading,
        );

        let binary_path = download_server(version)?;

        self.cached_binary_path = Some(binary_path.clone());
        Ok(binary_path)
    }
}

/// Returns the Kotlin LSP build version to download.
///
/// Fetches the latest release tag from the `Kotlin/kotlin-lsp` GitHub
/// repository.
// JetBrains' RELEASES.md is stale (still points to expired 262.x builds),
// so we use the GitHub releases API as the source of truth.
// See: https://github.com/Kotlin/kotlin-lsp/issues/271
fn get_version() -> Result<String> {
    let release = latest_github_release(
        "Kotlin/kotlin-lsp",
        GithubReleaseOptions {
            require_assets: false,
            pre_release: false,
        },
    )
    .map_err(|e| format!("failed to fetch latest kotlin-lsp release: {e}"))?;

    let version = release
        .version
        .strip_prefix("kotlin-lsp/v")
        .unwrap_or(&release.version)
        .to_string();

    Ok(version)
}

/// Downloads the server archive for the given `version` from JetBrains and
/// returns the path to the server binary.
///
/// If a complete installation (directory *and* server binary) already exists,
/// the download is skipped. A leftover directory without the binary is treated
/// as a failed install and removed before downloading again. After a
/// successful download, older versions are removed from the working directory.
///
/// # Errors
///
/// Returns an error on 32-bit x86 platforms, if the download fails, or if the
/// extracted archive does not contain the expected server binary.
fn download_server(version: String) -> Result<String> {
    let (os, arch) = zed_extension_api::current_platform();

    let arch_suffix = match arch {
        zed::Architecture::X8664 => "",
        zed::Architecture::Aarch64 => "-aarch64",
        _ => {
            return Err("Platform X86 is not supported by the Kotlin language server.".to_string());
        }
    };

    let asset_name = match os {
        zed::Os::Windows => format!("kotlin-server-{version}{arch_suffix}.win.zip"),
        zed::Os::Mac => format!("kotlin-server-{version}{arch_suffix}.sit"),
        zed::Os::Linux => format!("kotlin-server-{version}{arch_suffix}.tar.gz"),
    };

    let url = format!(
        "https://download.jetbrains.com/language-server/kotlin-server/{version}/{asset_name}"
    );

    let extension_dir = format!(
        "{server_id}-{version}",
        server_id = KotlinLSP::LANGUAGE_SERVER_ID
    );

    let target_dir = match os {
        zed::Os::Windows => extension_dir.clone(),
        _ => format!("{extension_dir}/kotlin-server-{version}"),
    };

    let binary_path = format!(
        "{target_dir}/bin/intellij-server{exe_suffix}",
        exe_suffix = match os {
            zed::Os::Windows => ".exe",
            _ => "",
        }
    );

    if !fs::metadata(&binary_path).is_ok_and(|metadata| metadata.is_file()) {
        if fs::metadata(&extension_dir).is_ok_and(|metadata| metadata.is_dir()) {
            fs::remove_dir_all(&extension_dir)
                .map_err(|e| format!("failed to remove incomplete {extension_dir}: {e}"))?;
        }

        let downloaded_file_type = match os {
            zed::Os::Windows | zed::Os::Mac => zed_extension_api::DownloadedFileType::Zip,
            zed::Os::Linux => zed_extension_api::DownloadedFileType::GzipTar,
        };

        zed::download_file(&url, &extension_dir, downloaded_file_type)?;

        if !fs::metadata(&binary_path).is_ok_and(|metadata| metadata.is_file()) {
            return Err(format!(
                "Kotlin language server binary not found at {binary_path} after downloading {asset_name}; the archive layout may have changed"
            ));
        }
        make_file_executable(&binary_path)?;
        utils::remove_outdated_versions(KotlinLSP::LANGUAGE_SERVER_ID, &extension_dir)?;
    }

    Ok(binary_path)
}
