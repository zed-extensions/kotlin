use std::fs;

use url::Url;
use zed_extension_api::{self as zed, make_file_executable, Result};

use crate::language_servers::util;

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
        let release = get_release()?;

        zed::set_language_server_installation_status(
            language_server_id,
            &zed::LanguageServerInstallationStatus::Downloading,
        );

        let binary_path = download_from_teamcity(release)?;

        self.cached_binary_path = Some(binary_path.clone());
        Ok(binary_path)
    }
}

#[derive(Debug, PartialEq)]
struct Release {
    version: String,
    /// Download URL up to and including the `/{version}` path segment.
    base_url: Option<String>,
}

impl Release {
    fn download_url(&self, asset_name: &str) -> String {
        match &self.base_url {
            Some(base_url) => format!("{base_url}/{asset_name}"),
            // Fallback in case no link can be found in `RELEASES.md`. Builds from 262.8190.0
            // onwards live under `language-server/kotlin-server/`.
            None => format!(
                "https://download.jetbrains.com/language-server/kotlin-server/{version}/{asset_name}",
                version = self.version
            ),
        }
    }
}

/// Finds the latest release and, within its section, the base URL its download links point to.
fn extract_release_from_markdown(contents: &str) -> Option<Release> {
    let mut lines = contents
        .lines()
        .skip_while(|line| !line.starts_with("### v"));
    let version = lines.next()?.strip_prefix("### v")?.trim().to_string();

    let version_segment = format!("/{version}/");
    let base_url = lines
        .take_while(|line| !line.starts_with("### "))
        .flat_map(|line| line.split("](").skip(1))
        .find_map(|link| {
            let (url, _) = link.split_once(')')?;
            if !is_jetbrains_url(url) {
                return None;
            }
            // Keep everything up to the version, dropping its trailing slash.
            url.find(&version_segment)
                .map(|index| url[..index + version_segment.len() - 1].to_owned())
        });

    Some(Release { version, base_url })
}

fn is_jetbrains_url(url: &str) -> bool {
    Url::parse(url).is_ok_and(|url| {
        url.scheme() == "https"
            && url.domain().is_some_and(|domain| {
                domain == "jetbrains.com" || domain.ends_with(".jetbrains.com")
            })
    })
}

fn get_release() -> Result<Release> {
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
    extract_release_from_markdown(&body)
        .ok_or_else(|| "Failed to extract version from RELEASES.md".into())
}

fn download_from_teamcity(release: Release) -> Result<String> {
    let (os, arch) = zed_extension_api::current_platform();
    let version = &release.version;

    // WIN https://download.jetbrains.com/language-server/kotlin-server/262.9593.0/kotlin-server-262.9593.0.win.zip
    // WIN ARM https://download.jetbrains.com/language-server/kotlin-server/262.9593.0/kotlin-server-262.9593.0-aarch64.win.zip
    // LINUX https://download.jetbrains.com/language-server/kotlin-server/262.9593.0/kotlin-server-262.9593.0.tar.gz
    // LINUX ARM https://download.jetbrains.com/language-server/kotlin-server/262.9593.0/kotlin-server-262.9593.0-aarch64.tar.gz
    // MAC https://download.jetbrains.com/language-server/kotlin-server/262.9593.0/kotlin-server-262.9593.0.sit
    // MAC ARM https://download.jetbrains.com/language-server/kotlin-server/262.9593.0/kotlin-server-262.9593.0-aarch64.sit

    let arch_suffix = match arch {
        zed::Architecture::X8664 => "",
        zed::Architecture::Aarch64 => "-aarch64",
        _ => {
            return Err("Platform X86 is not supported by the Kotlin language server.".to_string())
        }
    };

    let asset_name = match os {
        zed::Os::Windows => format!("kotlin-server-{version}{arch_suffix}.win.zip"),
        zed::Os::Mac => format!("kotlin-server-{version}{arch_suffix}.sit"),
        zed::Os::Linux => format!("kotlin-server-{version}{arch_suffix}.tar.gz"),
    };

    let url = release.download_url(&asset_name);

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

    if !fs::metadata(&extension_dir).is_ok_and(|metadata| metadata.is_dir()) {
        let downloaded_file_type = match os {
            // We don't ask questions as to why `sit` == `zip`. Let JetBrains keep their secrets there
            zed::Os::Windows | zed::Os::Mac => zed_extension_api::DownloadedFileType::Zip,
            zed::Os::Linux => zed_extension_api::DownloadedFileType::GzipTar,
        };

        zed::download_file(&url, &extension_dir, downloaded_file_type)?;
        make_file_executable(&binary_path)?;
        util::remove_outdated_versions(KotlinLSP::LANGUAGE_SERVER_ID, &extension_dir)?;
    }

    Ok(binary_path)
}

#[cfg(test)]
mod tests {
    use indoc::indoc;

    use super::*;

    #[test]
    fn extracts_release_with_base_url() {
        let markdown = indoc! {r#"
            ### Kotlin LSP and VSC extension releases

            ### v263.6379.0
            - :test_tube: "Kotlin by JetBrains" extension v0.0.13 for VS Code

              The extension is also available on the [VS Code Marketplace](https://marketplace.visualstudio.com/items?itemName=JetBrains.kotlin-server) and [OpenVSX Registry](https://open-vsx.org/extension/JetBrains/kotlin-server).
                * [Download for macOS-x64](https://download.jetbrains.com/language-server/kotlin-server/263.6379.0/kotlin-server-0.0.13-mac-amd64.vsix)&nbsp;&nbsp;|&nbsp;&nbsp;[SHA-256 checksum](https://download.jetbrains.com/language-server/kotlin-server/263.6379.0/kotlin-server-0.0.13-mac-amd64.vsix.sha256)

            - :card_index_dividers: **Standalone Kotlin LSP Archive**

                * [Download for macOS-x64](https://download.jetbrains.com/language-server/kotlin-server/263.6379.0/kotlin-server-263.6379.0.sit)&nbsp;&nbsp;|&nbsp;&nbsp;[SHA-256 checksum](https://download.jetbrains.com/language-server/kotlin-server/263.6379.0/kotlin-server-263.6379.0.sit.sha256)

            ### v263.4702.0
        "#};

        assert_eq!(
            extract_release_from_markdown(markdown),
            Some(Release {
                version: "263.6379.0".into(),
                base_url: Some(
                    "https://download.jetbrains.com/language-server/kotlin-server/263.6379.0"
                        .into()
                ),
            })
        );

        let foreign = indoc! {r#"
                ### v1.0
                * [x](https://jetbrains.com.evil.com/1.0/kotlin-server-1.0.sit)
            "#};
        assert_eq!(
            extract_release_from_markdown(foreign).unwrap().base_url,
            None
        );
    }
}
