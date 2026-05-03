use std::fs;

use zed_extension_api::{self as zed, make_file_executable, Result};

// this version is known to be working,
// so we use it as a fallback if installation of the latest version fails
const FALLBACK_VERSION: &str = "262.2310.0";

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
    ) -> Result<String> {
        if let Some(path) = self.cached_binary_path.as_ref() {
            return Ok(path.clone());
        }

        let binary_path = match try_download_latest(language_server_id) {
            Ok(path) => path,
            Err(original_err) => {
                zed::set_language_server_installation_status(
                    language_server_id,
                    &zed::LanguageServerInstallationStatus::Downloading,
                );
                match download_from_teamcity(FALLBACK_VERSION.to_string()) {
                    Ok(path) => path,
                    Err(_) => return Err(original_err),
                }
            }
        };

        self.cached_binary_path = Some(binary_path.clone());
        Ok(binary_path)
    }
}

fn try_download_latest(language_server_id: &zed::LanguageServerId) -> Result<String> {
    zed::set_language_server_installation_status(
        language_server_id,
        &zed::LanguageServerInstallationStatus::CheckingForUpdate,
    );
    let version = get_version()?;
    zed::set_language_server_installation_status(
        language_server_id,
        &zed::LanguageServerInstallationStatus::Downloading,
    );
    download_from_teamcity(version)
}

fn extract_version_from_markdown(contents: &str) -> Option<String> {
    contents
        .lines()
        .find_map(|line| line.strip_prefix("### v"))
        .map(|version| version.trim().to_string())
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
    let (os, arch) = zed_extension_api::current_platform();
    let platform = match os {
        zed::Os::Mac => "mac",
        zed::Os::Linux => "linux",
        zed::Os::Windows => "win",
    };
    let arch = match arch {
        zed::Architecture::Aarch64 => "aarch64",
        zed::Architecture::X8664 => "x64",
        _ => {
            return Err("Platform X86 is not supported by the Kotlin language server.".to_string())
        }
    };

    let url =
        format!("https://download-cdn.jetbrains.com/kotlin-lsp/{version}/kotlin-lsp-{version}-{platform}-{arch}.zip");
    let target_dir = format!("kotlin-lsp-{version}");
    let script_path = format!(
        "{target_dir}/kotlin-lsp.{extension}",
        extension = match os {
            zed::Os::Mac | zed::Os::Linux => "sh",
            zed::Os::Windows => "cmd",
        }
    );
    if !fs::metadata(&script_path).is_ok_and(|stat| stat.is_file()) {
        zed::download_file(
            &url,
            &target_dir,
            zed_extension_api::DownloadedFileType::Zip,
        )
        .map_err(|e| format!("failed to download kotlin-lsp: {e}"))?;

        if !fs::metadata(&script_path).is_ok_and(|stat| stat.is_file()) {
            return Err(format!(
                "failed to locate kotlin-lsp launcher after extraction: {script_path}"
            ));
        }

        make_file_executable(&script_path)
            .map_err(|e| format!("failed to make kotlin-lsp script executable: {e}"))?;
    }

    Ok(script_path)
}
