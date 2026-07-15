use zed::serde_json::{self, Value};
use zed::LanguageServerId;
use zed_extension_api::{self as zed, settings::LspSettings, Result};

mod language_servers;

use language_servers::{
    absolutize_in_work_dir, find_or_install_proxy, merge_proxy_settings, proxy_disabled,
    KotlinLSP, KotlinLanguageServer, SourceProxySettings,
};

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
        worktree: &zed::Worktree,
    ) -> zed::Result<zed::Command> {
        let proxy_settings = read_proxy_settings(worktree);

        match language_server_id.as_ref() {
            KotlinLanguageServer::LANGUAGE_SERVER_ID => {
                let kotlin_language_server = self
                    .kotlin_language_server
                    .get_or_insert_with(KotlinLanguageServer::new);

                let binary_path =
                    kotlin_language_server.language_server_binary_path(language_server_id)?;
                Ok(wrap_with_proxy(
                    language_server_id,
                    language_server_id.as_ref(),
                    binary_path,
                    vec![],
                    &proxy_settings,
                ))
            }
            KotlinLSP::LANGUAGE_SERVER_ID => {
                let kotlin_lsp = self.kotlin_lsp.get_or_insert_with(KotlinLSP::new);
                let binary_path = kotlin_lsp.language_server_binary_path(language_server_id)?;
                Ok(wrap_with_proxy(
                    language_server_id,
                    language_server_id.as_ref(),
                    binary_path,
                    vec!["--stdio".to_string()],
                    &proxy_settings,
                ))
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
        let mut settings = LspSettings::for_worktree(language_server_id.as_ref(), worktree)
            .ok()
            .and_then(|lsp_settings| lsp_settings.settings.clone())
            .unwrap_or_else(|| Value::Object(Default::default()));

        // Do not forward extension-only keys to the language server.
        SourceProxySettings::strip_from(&mut settings);

        Ok(Some(serde_json::json!({
            "kotlin": settings
        })))
    }
}

fn read_proxy_settings(worktree: &zed::Worktree) -> SourceProxySettings {
    let mut values = Vec::new();
    for id in [
        KotlinLanguageServer::LANGUAGE_SERVER_ID,
        KotlinLSP::LANGUAGE_SERVER_ID,
    ] {
        if let Ok(lsp) = LspSettings::for_worktree(id, worktree) {
            if let Some(settings) = lsp.settings {
                values.push(settings);
            }
        }
    }
    merge_proxy_settings(&values)
}

/// Prefer `kotlin-lsp-proxy <absolute-real-ls> [ls-args...]` when available.
///
/// Fail-open: disabled settings, missing binary, or download failure → launch LS directly.
fn wrap_with_proxy(
    language_server_id: &LanguageServerId,
    server_name: &str,
    binary_path: String,
    ls_args: Vec<String>,
    proxy_settings: &SourceProxySettings,
) -> zed::Command {
    if proxy_disabled() || !proxy_settings.enabled {
        return zed::Command {
            command: binary_path,
            args: ls_args,
            env: Default::default(),
        };
    }

    if let Some(proxy) = find_or_install_proxy(language_server_id) {
        let abs_ls = absolutize_in_work_dir(&binary_path);
        let mut args = Vec::with_capacity(1 + ls_args.len());
        args.push(abs_ls);
        args.extend(ls_args);

        let mut env = Vec::new();
        if proxy_settings.debug {
            env.push(("KOTLIN_LSP_PROXY_DEBUG".into(), "1".into()));
        }

        // Quiet by default — only log when debug is on.
        if proxy_settings.debug {
            println!("kotlin-ext: wrap {server_name} via proxy");
        }

        zed::Command {
            command: proxy,
            args,
            env,
        }
    } else {
        zed::Command {
            command: binary_path,
            args: ls_args,
            env: Default::default(),
        }
    }
}

zed::register_extension!(KotlinExtension);
