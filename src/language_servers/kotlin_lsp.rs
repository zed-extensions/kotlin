use std::path::Path;

use zed_extension_api::{self as zed, make_file_executable, settings::LspSettings, Result};

pub struct KotlinLSP {
    cached_binary_path: Option<String>,
}

impl KotlinLSP {
    pub const LANGUAGE_SERVER_ID: &'static str = "kotlin-lsp";

    pub fn new() -> Self {
        KotlinLSP {
            cached_binary_path: None,
        }
    }

    pub fn language_server_binary_path(
        &mut self,
        language_server_id: &zed::LanguageServerId,
        worktree: &zed::Worktree,
    ) -> Result<String> {
        if let Some(path) = self.cached_binary_path.as_ref() {
            return Ok(path.clone());
        }

        zed::set_language_server_installation_status(
            language_server_id,
            &zed::LanguageServerInstallationStatus::CheckingForUpdate,
        );

        // Check if custom download URL is provided in settings
        let settings = LspSettings::for_worktree(Self::LANGUAGE_SERVER_ID, worktree)
            .ok()
            .and_then(|lsp_settings| lsp_settings.settings.clone());

        let binary_path = if let Some(settings_obj) = settings {
            if let Some(custom_url) = settings_obj.get("download_url").and_then(|v| v.as_str()) {
                zed::set_language_server_installation_status(
                    language_server_id,
                    &zed::LanguageServerInstallationStatus::Downloading,
                );
                download_from_url(custom_url.to_string())?
            } else {
                let version = get_version()?;
                zed::set_language_server_installation_status(
                    language_server_id,
                    &zed::LanguageServerInstallationStatus::Downloading,
                );
                download_from_teamcity(version)?
            }
        } else {
            let version = get_version()?;
            zed::set_language_server_installation_status(
                language_server_id,
                &zed::LanguageServerInstallationStatus::Downloading,
            );
            download_from_teamcity(version)?
        };

        self.cached_binary_path = Some(binary_path.clone());
        Ok(binary_path)
    }
}

fn extract_version_from_markdown(contents: &str) -> Option<String> {
    contents
        .lines()
        .find_map(|line| line.strip_prefix("### v"))
        .map(|version| version.to_string())
}

/// Return URL to the kotlin-lsp package on TeamCity servers
fn get_version() -> Result<String> {
    let url = "https://raw.githubusercontent.com/Kotlin/kotlin-lsp/refs/heads/main/RELEASES.md"
        .to_string();
    let result = zed::http_client::fetch(&zed::http_client::HttpRequest {
        method: zed::http_client::HttpMethod::Get,
        url,
        headers: vec![],
        body: None,
        redirect_policy: zed::http_client::RedirectPolicy::NoFollow,
    })?;
    let body =
        String::from_utf8(result.body).map_err(|_| "Failed to fetch RELEASES.md".to_owned())?;
    extract_version_from_markdown(&body)
        .ok_or_else(|| "Failed to extract version from RELEASES.md".into())
}

fn download_from_teamcity(version: String) -> Result<String> {
    let url =
        format!("https://download-cdn.jetbrains.com/kotlin-lsp/{version}/kotlin-{version}.zip");
    download_from_url_with_version(url, version)
}

fn download_from_url(url: String) -> Result<String> {
    // Extract a unique identifier from the URL for the target directory
    // Use a hash or simplified version to create a unique directory name
    let url_hash = format!("{:x}", url.bytes().fold(0u64, |acc, b| acc.wrapping_mul(31).wrapping_add(b as u64)));
    let target_dir = format!("kotlin-lsp-custom-{}", url_hash);

    let (os, _arch) = zed_extension_api::current_platform();
    let script_path = format!(
        "{target_dir}/kotlin-lsp.{extension}",
        extension = match os {
            zed::Os::Mac | zed::Os::Linux => "sh",
            zed::Os::Windows => "cmd",
        }
    );

    if !Path::new(&target_dir).exists() {
        zed::download_file(
            &url,
            &target_dir,
            zed_extension_api::DownloadedFileType::Zip,
        )?;
        make_file_executable(&script_path)?;
    }
    Ok(script_path)
}

fn download_from_url_with_version(url: String, version: String) -> Result<String> {
    let target_dir = format!("kotlin-lsp-{version}");
    let (os, _arch) = zed_extension_api::current_platform();
    let script_path = format!(
        "{target_dir}/kotlin-lsp.{extension}",
        extension = match os {
            zed::Os::Mac | zed::Os::Linux => "sh",
            zed::Os::Windows => "cmd",
        }
    );
    if !Path::new(&target_dir).exists() {
        zed::download_file(
            &url,
            &target_dir,
            zed_extension_api::DownloadedFileType::Zip,
        )?;
        make_file_executable(&script_path)?;
    }
    Ok(script_path)
}
