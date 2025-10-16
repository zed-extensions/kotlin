use std::path::Path;

use zed_extension_api::{self as zed, make_file_executable, Result};

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

        zed::set_language_server_installation_status(
            language_server_id,
            &zed::LanguageServerInstallationStatus::CheckingForUpdate,
        );
        let version = get_version()?;

        zed::set_language_server_installation_status(
            language_server_id,
            &zed::LanguageServerInstallationStatus::Downloading,
        );

        let binary_path = download_from_teamcity(version)?;

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
    let target_dir = format!("kotlin-lsp-{version}");
    let script_path = format!("{target_dir}/kotlin-lsp{extension}",
        extension = match os {
            zed::Os::Mac | zed::Os::Linux => ".sh",
            zed::Os::Windows => ".cmd",
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
