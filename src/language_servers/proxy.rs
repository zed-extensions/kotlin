use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use zed_extension_api::{
    self as zed, Architecture, DownloadedFileType, LanguageServerId,
    LanguageServerInstallationStatus, Os, Result,
};

/// Relative path under the extension work directory for a manually installed proxy.
pub const PROXY_RELATIVE: &str = "bin/kotlin-lsp-proxy";

const PROXY_BINARY: &str = "kotlin-lsp-proxy";
/// GitHub repo that publishes platform proxy binaries on release tags (`vX.Y.Z`).
/// Override with `KOTLIN_LSP_PROXY_GITHUB_REPO` (e.g. `you/kotlin`) while testing a fork.
const DEFAULT_GITHUB_REPO: &str = "zed-extensions/kotlin";

/// Locate or download the jar-URI materializing proxy.
///
/// Search order:
/// 1. `$KOTLIN_LSP_PROXY` (absolute path)
/// 2. `bin/kotlin-lsp-proxy` in the extension work dir (local / script install)
/// 3. Managed download from GitHub Releases for this extension version
///
/// Returns `None` if unavailable — callers should start the real LS directly (fail-open).
pub fn find_or_install_proxy(language_server_id: &LanguageServerId) -> Option<String> {
    if proxy_disabled() {
        return None;
    }

    if let Ok(path) = env::var("KOTLIN_LSP_PROXY") {
        if path_is_file(Path::new(&path)) {
            return Some(path);
        }
    }

    if let Ok(cwd) = env::current_dir() {
        let local = cwd.join(PROXY_RELATIVE);
        if path_is_file(&local) {
            // Relative so Zed resolves against the extension work dir.
            return Some(PROXY_RELATIVE.to_string());
        }
        // Also accept versioned layout: bin/v0.4.0/kotlin-lsp-proxy
        let versioned = cwd
            .join("bin")
            .join(format!("v{}", env!("CARGO_PKG_VERSION")))
            .join(proxy_exec_name());
        if path_is_file(&versioned) {
            return Some(versioned.to_string_lossy().into_owned());
        }
    }

    match download_proxy(language_server_id) {
        Ok(path) => Some(path),
        Err(err) => {
            println!("kotlin-ext: proxy download skipped/failed: {err}");
            None
        }
    }
}

/// Make a language-server path absolute using the extension work directory.
///
/// Zed absolutizes `Command.command` but not argv. The proxy is spawned with the
/// project as cwd, so the real LS path must be absolute.
pub fn absolutize_in_work_dir(path: &str) -> String {
    let p = Path::new(path);
    if p.is_absolute() {
        return path.to_string();
    }
    match env::current_dir() {
        Ok(cwd) => cwd.join(p).to_string_lossy().into_owned(),
        Err(_) => path.to_string(),
    }
}

pub fn proxy_disabled() -> bool {
    env::var("KOTLIN_LSP_PROXY_DISABLE")
        .map(|v| matches!(v.as_str(), "1" | "true" | "TRUE" | "yes"))
        .unwrap_or(false)
}

fn path_is_file(path: &Path) -> bool {
    fs::metadata(path).is_ok_and(|m| m.is_file())
}

fn proxy_exec_name() -> String {
    let (os, _) = zed::current_platform();
    match os {
        Os::Windows => format!("{PROXY_BINARY}.exe"),
        _ => PROXY_BINARY.to_string(),
    }
}

fn github_repo() -> String {
    env::var("KOTLIN_LSP_PROXY_GITHUB_REPO").unwrap_or_else(|_| DEFAULT_GITHUB_REPO.to_string())
}

fn platform_asset() -> Result<(String, DownloadedFileType)> {
    let (os, arch) = zed::current_platform();
    let (os_str, file_type) = match os {
        Os::Mac => ("darwin", DownloadedFileType::GzipTar),
        Os::Linux => ("linux", DownloadedFileType::GzipTar),
        Os::Windows => ("windows", DownloadedFileType::Zip),
    };
    let arch_str = match arch {
        Architecture::Aarch64 => "aarch64",
        Architecture::X8664 => "x86_64",
        _ => return Err("Unsupported architecture for kotlin-lsp-proxy".into()),
    };
    let ext = match file_type {
        DownloadedFileType::Zip => "zip",
        _ => "tar.gz",
    };
    Ok((
        format!("{PROXY_BINARY}-{os_str}-{arch_str}.{ext}"),
        file_type,
    ))
}

fn download_proxy(language_server_id: &LanguageServerId) -> Result<String> {
    let version = env!("CARGO_PKG_VERSION");
    let tag = format!("v{version}");
    let exec_name = proxy_exec_name();
    let version_dir = format!("bin/v{version}");
    let binary_path = format!("{version_dir}/{exec_name}");

    if path_is_file(Path::new(&binary_path)) {
        return Ok(binary_path);
    }

    zed::set_language_server_installation_status(
        language_server_id,
        &LanguageServerInstallationStatus::CheckingForUpdate,
    );

    let repo = github_repo();
    let release = zed::github_release_by_tag_name(&repo, &tag)
        .map_err(|e| format!("proxy release {tag} from {repo}: {e}"))?;

    let (asset_name, file_type) = platform_asset()?;
    let asset = release
        .assets
        .iter()
        .find(|a| a.name == asset_name)
        .ok_or_else(|| format!("no proxy asset {asset_name} in {repo}@{tag}"))?;

    zed::set_language_server_installation_status(
        language_server_id,
        &LanguageServerInstallationStatus::Downloading,
    );

    fs::create_dir_all(&version_dir).map_err(|e| e.to_string())?;
    zed::download_file(&asset.download_url, &version_dir, file_type)
        .map_err(|e| format!("download proxy: {e}"))?;

    // tarball may extract to version_dir/kotlin-lsp-proxy or nested
    let candidates = [
        PathBuf::from(&binary_path),
        PathBuf::from(&version_dir).join(PROXY_BINARY),
        PathBuf::from(&version_dir).join(format!("{PROXY_BINARY}.exe")),
    ];
    let found = candidates.into_iter().find(|p| path_is_file(p));
    let Some(found) = found else {
        return Err(format!(
            "proxy binary not found after extract in {version_dir}"
        ));
    };

    if found.to_string_lossy() != binary_path {
        let _ = fs::rename(&found, &binary_path);
    }

    let _ = zed::make_file_executable(&binary_path);

    zed::set_language_server_installation_status(
        language_server_id,
        &LanguageServerInstallationStatus::None,
    );

    Ok(binary_path)
}
