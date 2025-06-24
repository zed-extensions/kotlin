use std::path::Path;

use zed_extension_api::{self as zed, make_file_executable, Result};

pub const LANGUAGE_SERVER_ID: &'static str = "kotlin-lsp";

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
    });
    
    let result = match result {
        Ok(result) => result,
        Err(e) => {
            eprintln!("Failed to fetch RELEASES.md: {}", e);
            return Err(format!("Failed to fetch version information: {}", e));
        }
    };
    
    let body = String::from_utf8(result.body)
        .map_err(|e| format!("Failed to decode RELEASES.md content: {}", e))?;
    
    extract_version_from_markdown(&body)
        .ok_or_else(|| "Failed to extract version from RELEASES.md. The file format may have changed.".into())
}

fn download_from_teamcity(version: String) -> Result<String> {
    let url = format!("https://download-cdn.jetbrains.com/kotlin-lsp/{version}/kotlin-{version}.zip");
    let target_dir = format!("kotlin-lsp-{version}");
    let script_path = format!("{target_dir}/kotlin-lsp.sh");
    
    if !Path::new(&target_dir).exists() {
        match zed::download_file(
            &url,
            &target_dir,
            zed_extension_api::DownloadedFileType::Zip,
        ) {
            Ok(_) => {},
            Err(e) => {
                eprintln!("Failed to download kotlin-lsp from {}: {}", url, e);
                return Err(format!("Failed to download kotlin-lsp: {}. This might be due to network issues or the version {} not being available.", e, version));
            }
        }
        
        if let Err(e) = make_file_executable(&script_path) {
            eprintln!("Failed to make {} executable: {}", script_path, e);
            return Err(format!("Failed to make kotlin-lsp script executable: {}", e));
        }
    }
    
    // Verify the script exists and is executable
    if !Path::new(&script_path).exists() {
        return Err(format!("kotlin-lsp script not found at expected location: {}", script_path));
    }
    
    Ok(script_path)
}

pub fn language_server_binary_path() -> Result<String> {
    let version = get_version()?;
    download_from_teamcity(version)
}
