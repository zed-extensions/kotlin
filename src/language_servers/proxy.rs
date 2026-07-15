use std::fs;

use zed_extension_api::{self as zed, make_file_executable, serde_json::Value, Result, Worktree};

use crate::language_servers::util;

const PROXY_BINARY: &str = "kotlin-lsp-proxy";
/// Install dirs are named `kotlin-proxy-<version>`. The distinct `kotlin-proxy`
/// prefix (vs. `kotlin-lsp`) keeps its cleanup from clobbering the LSP install.
const INSTALL_PREFIX: &str = "kotlin-proxy";
const GITHUB_REPO: &str = "zed-extensions/kotlin";

/// Downloads and caches the `kotlin-lsp-proxy` binary that resolves archive-internal
/// (`jar!/`, `zip!/`) source URIs returned by kotlin-lsp. See zed-extensions/kotlin#106.
pub struct Proxy {
    cached_binary_path: Option<String>,
}

impl Proxy {
    pub fn new() -> Self {
        Self {
            cached_binary_path: None,
        }
    }

    pub fn language_server_binary_path(
        &mut self,
        language_server_id: &zed::LanguageServerId,
        configuration: &Option<Value>,
        worktree: &Worktree,
    ) -> Result<String> {
        if let Some(path) = self.cached_binary_path.as_ref() {
            return Ok(path.clone());
        }

        // Escape hatch: point directly at a locally-built or manually-placed binary,
        // bypassing the GitHub Releases download entirely (`lsp.kotlin-lsp.settings.proxy_path`
        // in settings.json). Useful when testing an unreleased build, or if release assets
        // aren't available for a given platform.
        if let Some(path) = user_configured_proxy_path(configuration, worktree) {
            self.cached_binary_path = Some(path.clone());
            return Ok(path);
        }

        zed::set_language_server_installation_status(
            language_server_id,
            &zed::LanguageServerInstallationStatus::CheckingForUpdate,
        );

        let release = zed::latest_github_release(
            GITHUB_REPO,
            zed::GithubReleaseOptions {
                require_assets: true,
                pre_release: false,
            },
        )
        .map_err(|err| format!("Failed to fetch kotlin-lsp-proxy release: {err}"))?;

        let (asset_name, file_type) = asset(&release.version)?;
        let version_dir = format!("{INSTALL_PREFIX}-{}", release.version);
        let binary_path = format!("{version_dir}/{}", proxy_exec());

        if !fs::metadata(&binary_path).is_ok_and(|m| m.is_file()) {
            let asset = release
                .assets
                .iter()
                .find(|a| a.name == asset_name)
                .ok_or_else(|| {
                    format!(
                        "No kotlin-lsp-proxy asset matching {asset_name:?} in release {}",
                        release.version
                    )
                })?;

            zed::set_language_server_installation_status(
                language_server_id,
                &zed::LanguageServerInstallationStatus::Downloading,
            );

            zed::download_file(&asset.download_url, &version_dir, file_type)
                .map_err(|err| format!("Failed to download kotlin-lsp-proxy: {err}"))?;
            make_file_executable(&binary_path)?;
            util::remove_outdated_versions(INSTALL_PREFIX, &version_dir)?;
        }

        self.cached_binary_path = Some(binary_path.clone());
        Ok(binary_path)
    }
}

fn asset(version: &str) -> Result<(String, zed::DownloadedFileType)> {
    let (os, arch) = zed::current_platform();
    let (os_str, file_type) = match os {
        zed::Os::Mac => ("darwin", zed::DownloadedFileType::GzipTar),
        zed::Os::Linux => ("linux", zed::DownloadedFileType::GzipTar),
        zed::Os::Windows => ("windows", zed::DownloadedFileType::Zip),
    };
    let arch_str = match arch {
        zed::Architecture::Aarch64 => "aarch64",
        zed::Architecture::X8664 => "x86_64",
        _ => return Err("Unsupported architecture for kotlin-lsp-proxy".into()),
    };
    let ext = match file_type {
        zed::DownloadedFileType::Zip => "zip",
        _ => "tar.gz",
    };
    let _ = version;
    Ok((
        format!("{PROXY_BINARY}-{os_str}-{arch_str}.{ext}"),
        file_type,
    ))
}

fn proxy_exec() -> String {
    match zed::current_platform().0 {
        zed::Os::Windows => format!("{PROXY_BINARY}.exe"),
        _ => PROXY_BINARY.to_string(),
    }
}

fn user_configured_proxy_path(configuration: &Option<Value>, worktree: &Worktree) -> Option<String> {
    let path = configuration
        .as_ref()?
        .pointer("/proxy_path")
        .and_then(|v| v.as_str())?;
    expand_home_path(worktree, path.to_string()).ok()
}

/// Expand a leading `~` to the worktree's `$HOME`. On Windows the path is returned
/// unchanged (mirrors the equivalent helper in the Zed Java extension).
fn expand_home_path(worktree: &Worktree, path: String) -> Result<String> {
    match zed::current_platform().0 {
        zed::Os::Windows => Ok(path),
        _ => worktree
            .shell_env()
            .into_iter()
            .find(|(key, _)| key == "HOME")
            .map(|(_, home)| path.replace('~', &home))
            .ok_or_else(|| "Failed to expand '~': $HOME not found in shell environment".to_string()),
    }
}
