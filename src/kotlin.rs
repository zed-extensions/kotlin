use zed::serde_json;
use zed::LanguageServerId;
use zed_extension_api::{self as zed, settings::LspSettings, Result};

mod language_servers;

use language_servers::{
    absolutize_in_work_dir, find_or_install_proxy, proxy_debug_enabled, proxy_disabled, KotlinLSP,
    KotlinLanguageServer,
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
        _: &zed::Worktree,
    ) -> zed::Result<zed::Command> {
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
        let settings = LspSettings::for_worktree(language_server_id.as_ref(), worktree)
            .ok()
            .and_then(|lsp_settings| lsp_settings.settings.clone())
            .unwrap_or_default();

        Ok(Some(serde_json::json!({
            "kotlin": settings
        })))
    }
}

/// Prefer:
///   kotlin-lsp-proxy <absolute-real-ls> [ls-args...]
/// so jar!/zip! definition URIs can be rewritten to real files for Zed.
///
/// Fail-open: if the proxy is disabled or missing, launch the real LS directly.
fn wrap_with_proxy(
    language_server_id: &LanguageServerId,
    server_name: &str,
    binary_path: String,
    ls_args: Vec<String>,
) -> zed::Command {
    if proxy_disabled() {
        println!("kotlin-ext: proxy disabled — direct {server_name}");
        return zed::Command {
            command: binary_path,
            args: ls_args,
            env: Default::default(),
        };
    }

    if let Some(proxy) = find_or_install_proxy(language_server_id) {
        let abs_ls = absolutize_in_work_dir(&binary_path);
        println!("kotlin-ext: wrap {server_name} via {proxy} -> {abs_ls}");
        let mut args = Vec::with_capacity(1 + ls_args.len());
        args.push(abs_ls);
        args.extend(ls_args);

        let mut env = Vec::new();
        if proxy_debug_enabled() {
            env.push(("KOTLIN_LSP_PROXY_DEBUG".into(), "1".into()));
        }

        zed::Command {
            command: proxy,
            args,
            env,
        }
    } else {
        println!("kotlin-ext: no proxy — direct {server_name} path={binary_path}");
        zed::Command {
            command: binary_path,
            args: ls_args,
            env: Default::default(),
        }
    }
}

zed::register_extension!(KotlinExtension);
