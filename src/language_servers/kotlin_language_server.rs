use std::fs;

use zed_extension_api::{self as zed, Result};

pub const LANGUAGE_SERVER_ID: &'static str = "kotlin-language-server";

pub fn language_server_binary_path(language_server_id: &zed::LanguageServerId) -> Result<String> {
    zed::set_language_server_installation_status(
        language_server_id,
        &zed::LanguageServerInstallationStatus::CheckingForUpdate,
    );
    
    let release = match zed::latest_github_release(
        "fwcd/kotlin-language-server",
        zed::GithubReleaseOptions {
            require_assets: true,
            pre_release: false,
        },
    ) {
        Ok(release) => release,
        Err(e) => {
            eprintln!("Failed to fetch latest release information: {}", e);
            return Err(format!("Failed to get release information: {}. This might be due to network issues or API rate limits.", e));
        }
    };

    let asset_name = "server.zip";
    let asset = release
        .assets
        .iter()
        .find(|asset| asset.name == asset_name)
        .ok_or_else(|| format!("Required asset '{}' not found in release {}", asset_name, release.version))?;

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
            language_server_id,
            &zed::LanguageServerInstallationStatus::Downloading,
        );

        if let Err(e) = zed::download_file(
            &asset.download_url,
            &version_dir,
            zed::DownloadedFileType::Zip,
        ) {
            eprintln!("Failed to download kotlin-language-server: {}", e);
            return Err(format!("Failed to download kotlin-language-server: {}. This might be due to network issues.", e));
        }

        if let Err(e) = zed::make_file_executable(&binary_path) {
            eprintln!("Failed to make binary executable: {}", e);
            return Err(format!("Failed to make kotlin-language-server executable: {}", e));
        }
    }

    // Final verification that the binary exists
    if !fs::metadata(&binary_path).map_or(false, |stat| stat.is_file()) {
        return Err(format!("kotlin-language-server binary not found at expected location: {}", binary_path));
    }

    zed::set_language_server_installation_status(
        language_server_id,
        &zed::LanguageServerInstallationStatus::None,
    );

    Ok(binary_path)
}
