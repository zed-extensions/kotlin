use zed::serde_json;
use zed::LanguageServerId;
use zed_extension_api::{self as zed, settings::LspSettings, Result};

mod language_servers;

use language_servers::{KotlinLSP, KotlinLanguageServer};

struct KotlinExtension {
    kotlin_language_server: Option<KotlinLanguageServer>,
    kotlin_lsp: Option<KotlinLSP>,
}

impl zed::Extension for KotlinExtension {
    fn new() -> Self {
        Self {
            kotlin_language_server: None,
            kotlin_lsp: None,
        }
    }

    fn language_server_command(
        &mut self,
        language_server_id: &LanguageServerId,
        _: &zed::Worktree,
    ) -> zed::Result<zed::Command> {
        match language_server_id.as_ref() {
            KotlinLanguageServer::LANGUAGE_SERVER_ID => {
                let kotlin_language_server = self
                    .kotlin_language_server
                    .get_or_insert_with(KotlinLanguageServer::new);

                let binary_path =
                    kotlin_language_server.language_server_binary_path(language_server_id)?;
                Ok(zed::Command {
                    command: binary_path,
                    args: vec![],
                    env: Default::default(),
                })
            }
            KotlinLSP::LANGUAGE_SERVER_ID => {
                let kotlin_lsp = self.kotlin_lsp.get_or_insert_with(KotlinLSP::new);
                let binary_path = kotlin_lsp.language_server_binary_path(language_server_id)?;
                Ok(zed::Command {
                    command: binary_path,
                    args: vec!["--stdio".to_string()],
                    env: Default::default(),
                })
            }
            _ => Err(format!(
                "Unrecognized language server for Kotlin: {language_server_id}"
            )),
        }
    }

    fn language_server_workspace_configuration(
        &mut self,
        language_server_id: &LanguageServerId,
        worktree: &zed_extension_api::Worktree,
    ) -> Result<Option<serde_json::Value>> {
        let settings = LspSettings::for_worktree(language_server_id.as_ref(), worktree)
            .ok()
            .and_then(|lsp_settings| lsp_settings.settings.clone())
            .unwrap_or_default();

        // todo! test with kotlin-lsp, is "kotlin" key required?
        Ok(Some(serde_json::json!({
            "kotlin": settings
        })))
    }
}

zed::register_extension!(KotlinExtension);
