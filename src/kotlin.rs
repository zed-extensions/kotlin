use std::fs;
use zed::serde_json;
use zed::LanguageServerId;
use zed_extension_api::{self as zed, settings::LspSettings, Result};

mod language_servers;

use language_servers::{kotlin_language_server, kotlin_lsp};

struct KotlinExtension {
    cached_binary_path: Option<String>,
}

impl KotlinExtension {
    fn language_server_binary_path(
        &mut self,
        language_server_id: &LanguageServerId,
    ) -> Result<String> {
        if let Some(path) = &self.cached_binary_path {
            if fs::metadata(path).map_or(false, |stat| stat.is_file()) {
                return Ok(path.clone());
            }
        }

        let binary_path = kotlin_language_server::language_server_binary_path(language_server_id)?;

        self.cached_binary_path = Some(binary_path.clone());
        Ok(binary_path)
    }
}

impl zed::Extension for KotlinExtension {
    fn new() -> Self {
        Self {
            cached_binary_path: None,
        }
    }

    fn language_server_command(
        &mut self,
        language_server_id: &LanguageServerId,
        _: &zed::Worktree,
    ) -> zed::Result<zed::Command> {
        match language_server_id.as_ref() {
            kotlin_language_server::LANGUAGE_SERVER_ID => Ok(zed::Command {
                command: self.language_server_binary_path(language_server_id)?,
                args: vec![],
                env: Default::default(),
            }),
            _ => Err(format!(
                "Unsupported language server ID: {}",
                language_server_id
            )),
        }
    }

    fn language_server_workspace_configuration(
        &mut self,
        language_server_id: &LanguageServerId,
        worktree: &zed_extension_api::Worktree,
    ) -> Result<Option<serde_json::Value>> {
        if language_server_id.as_ref() != kotlin_language_server::LANGUAGE_SERVER_ID {
            return Err(format!(
                "Unsupported language server ID: {}",
                language_server_id
            ));
        }

        let settings = LspSettings::for_worktree(language_server_id.as_ref(), worktree)
            .ok()
            .and_then(|lsp_settings| lsp_settings.settings.clone())
            .unwrap_or_default();

        Ok(Some(serde_json::json!({
            "kotlin": settings
        })))
    }
}

zed::register_extension!(KotlinExtension);
