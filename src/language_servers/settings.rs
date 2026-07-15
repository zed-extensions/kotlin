//! Extension-side settings for the jar/zip source proxy.
//!
//! Users may place these under either language server's `settings` object.
//! They are **stripped** before settings are forwarded to the language server.
//!
//! ```json
//! {
//!   "lsp": {
//!     "kotlin-language-server": {
//!       "settings": {
//!         "sourceProxy": {
//!           "enabled": true,
//!           "debug": false
//!         }
//!       }
//!     }
//!   }
//! }
//! ```

use zed_extension_api::serde_json::Value;

const KEY: &str = "sourceProxy";

#[derive(Debug, Clone)]
pub struct SourceProxySettings {
    /// When false, the real language server is launched without the proxy.
    pub enabled: bool,
    /// When true, the proxy logs URI rewrites verbosely.
    pub debug: bool,
}

impl Default for SourceProxySettings {
    fn default() -> Self {
        Self {
            enabled: true,
            debug: false,
        }
    }
}

impl SourceProxySettings {
    /// Remove `sourceProxy` so it is not sent to the Kotlin language server.
    pub fn strip_from(settings: &mut Value) {
        if let Some(obj) = settings.as_object_mut() {
            obj.remove(KEY);
        }
    }
}

/// Merge proxy settings from multiple setting objects (first wins for each field if present).
pub fn merge_proxy_settings(values: &[Value]) -> SourceProxySettings {
    let mut out = SourceProxySettings::default();
    let mut seen_enabled = false;
    let mut seen_debug = false;
    for v in values {
        if let Some(obj) = v.get(KEY) {
            if !seen_enabled {
                if let Some(e) = obj.get("enabled").and_then(|x| x.as_bool()) {
                    out.enabled = e;
                    seen_enabled = true;
                }
            }
            if !seen_debug {
                if let Some(d) = obj.get("debug").and_then(|x| x.as_bool()) {
                    out.debug = d;
                    seen_debug = true;
                }
            }
        }
    }
    out
}
