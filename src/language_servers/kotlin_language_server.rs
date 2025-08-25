use std::fs;

use zed_extension_api::{self as zed, Result};

pub struct KotlinLanguageServer {
    cached_binary_path: Option<String>,
}

impl KotlinLanguageServer {
    pub const LANGUAGE_SERVER_ID: &'static str = "kotlin-language-server";

    pub fn new() -> Self {
        Self {
            cached_binary_path: None,
        }
    }
}

impl KotlinLanguageServer {
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
        let release = zed::latest_github_release(
            "fwcd/kotlin-language-server",
            zed::GithubReleaseOptions {
                require_assets: true,
                pre_release: false,
            },
        )?;

        let asset_name = "server.zip";
        let asset = release
            .assets
            .iter()
            .find(|asset| asset.name == asset_name)
            .ok_or_else(|| "no asset found")?;

        let (os, _arch) = zed::current_platform();
        let version_dir = format!("kotlin-language-server-{}", release.version);
        let binary_path = format!(
            "{version_dir}/server/bin/kotlin-language-server{extension}",
            extension = match os {
                zed::Os::Mac | zed::Os::Linux => "",
                zed::Os::Windows => ".bat",
            }
        );

        if !fs::metadata(&binary_path).map_or(false, |stat| stat.is_file()) {
            zed::set_language_server_installation_status(
                &language_server_id,
                &zed::LanguageServerInstallationStatus::Downloading,
            );

            zed::download_file(
                &asset.download_url,
                &version_dir,
                zed::DownloadedFileType::Zip,
            )
            .map_err(|e| format!("failed to download file error: {e}"))?;

            zed::make_file_executable(&binary_path)
                .map_err(|e| format!("failed to make binary executable: {e}"))?;
        }

        self.cached_binary_path = Some(binary_path.clone());
        Ok(binary_path)
    }
}
