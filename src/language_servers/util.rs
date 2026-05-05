use std::fs;

use zed_extension_api::Result;

pub(super) fn remove_outdated_versions(
    language_server_id: &'static str,
    version_dir: &str,
) -> Result<()> {
    let entries = fs::read_dir(".").map_err(|e| format!("failed to list working directory {e}"))?;
    for entry in entries {
        let entry = entry.map_err(|e| format!("failed to load directory entry {e}"))?;
        if entry.file_name().to_str().is_none_or(|file_name| {
            file_name.starts_with(language_server_id) && file_name != version_dir
        }) {
            fs::remove_dir_all(entry.path()).ok();
        }
    }
    Ok(())
}

pub(super) fn find_existing_binary(
    language_server_id: &'static str,
    binary_name: &str,
) -> Option<String> {
    fs::read_dir(".").ok()?.flatten().find_map(|entry| {
        let binary_path = entry.path().join(binary_name);

        (binary_path.is_file()
            && entry
                .file_name()
                .to_str()
                .is_some_and(|binary_dir| binary_dir.starts_with(language_server_id)))
        .then(|| binary_path.to_string_lossy().to_string())
    })
}
