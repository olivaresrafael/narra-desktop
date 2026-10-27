//! Versioned application settings (M10 S4).
//!
//! Only non-secret preferences live here. API keys are written to the OS
//! keychain (`narra-core::provider::secrets`) and are deliberately absent
//! from this file, its `Debug` output and the dump.
//!
//! The store is a single JSON object with `schema_version`; unknown versions
//! fail closed (defaults + an explicit error) instead of being guessed.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use narra_core::agent::LOCAL_DEFAULT_BASE_URL;
use serde_json::{Value, json};

pub const SETTINGS_VERSION: u64 = 1;
pub const SETTINGS_FILE: &str = "settings.json";

/// Model provider kinds the app can configure. OpenCode Zen and OpenCode Go
/// are separate gateways with separate keys and endpoints; both expose an
/// OpenAI chat-completions route for the models Narra uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AiProvider {
    #[default]
    Mock,
    OpenAiCompat,
    OpenCode,
    OpenCodeGo,
}

impl AiProvider {
    pub const ALL: [AiProvider; 4] = [
        AiProvider::Mock,
        AiProvider::OpenAiCompat,
        AiProvider::OpenCode,
        AiProvider::OpenCodeGo,
    ];

    /// Stable key in JSON, argv and the dump.
    pub fn key(self) -> &'static str {
        match self {
            AiProvider::Mock => "mock",
            AiProvider::OpenAiCompat => "openai-compat",
            AiProvider::OpenCode => "opencode",
            AiProvider::OpenCodeGo => "opencode-go",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|provider| provider.key() == value)
    }

    /// i18n key for the settings chooser.
    pub fn i18n_key(self) -> &'static str {
        match self {
            AiProvider::Mock => "settings.provider.mock",
            AiProvider::OpenAiCompat => "settings.provider.openai_compat",
            AiProvider::OpenCode => "settings.provider.opencode",
            AiProvider::OpenCodeGo => "settings.provider.opencode_go",
        }
    }

    /// Fixed chat-completions base URL for a gateway. A custom provider uses
    /// the user's base URL; the caller decides which one to apply.
    pub fn fixed_base_url(self) -> Option<&'static str> {
        match self {
            AiProvider::OpenCode => Some("https://opencode.ai/inference/openai/v1"),
            AiProvider::OpenCodeGo => Some("https://opencode.ai/inference/go/openai/v1"),
            AiProvider::Mock | AiProvider::OpenAiCompat => None,
        }
    }

    /// Catalog endpoint. OpenCode exposes the model list at a different path
    /// from its chat route; this is the URL the settings "fetch models"
    /// action probes with the configured key.
    pub fn models_url(self, custom_base_url: &str) -> Option<String> {
        match self {
            AiProvider::OpenCode => Some("https://opencode.ai/inference/v1/models".to_string()),
            AiProvider::OpenCodeGo => {
                Some("https://opencode.ai/inference/go/v1/models".to_string())
            }
            AiProvider::OpenAiCompat => {
                Some(format!("{}/models", custom_base_url.trim_end_matches('/')))
            }
            AiProvider::Mock => None,
        }
    }

    /// Keychain account for the provider key; `None` for Mock.
    pub fn secret_account(self) -> Option<&'static str> {
        match self {
            AiProvider::Mock => None,
            AiProvider::OpenAiCompat => Some(narra_core::provider::OPENAI_COMPAT_API_KEY_ACCOUNT),
            AiProvider::OpenCode => Some(narra_core::provider::OPENCODE_API_KEY_ACCOUNT),
            AiProvider::OpenCodeGo => Some(narra_core::provider::OPENCODE_GO_API_KEY_ACCOUNT),
        }
    }

    /// Environment fallback for headless/CI runs.
    pub fn env_var(self) -> Option<&'static str> {
        match self {
            AiProvider::Mock => None,
            AiProvider::OpenAiCompat => Some("OPENAI_API_KEY"),
            AiProvider::OpenCode => Some("OPENCODE_API_KEY"),
            AiProvider::OpenCodeGo => Some("OPENCODE_GO_API_KEY"),
        }
    }

    pub fn requires_key(self) -> bool {
        matches!(self, AiProvider::OpenCode | AiProvider::OpenCodeGo)
    }

    pub fn default_model(self) -> &'static str {
        match self {
            AiProvider::OpenCode | AiProvider::OpenCodeGo => "deepseek-v4.1-flash",
            AiProvider::Mock | AiProvider::OpenAiCompat => "",
        }
    }

    /// Curated chat-completions models from the public catalogs. The model
    /// field stays editable so a new model can be used without a release;
    /// "fetch models" replaces this list with the live catalog.
    pub fn known_models(self) -> &'static [&'static str] {
        match self {
            AiProvider::OpenCode => &[
                "deepseek-v4.1-flash",
                "glm-5.3-flash",
                "glm-5.3",
                "kimi-k3",
                "qwen3.8-max",
                "minimax-m3",
            ],
            AiProvider::OpenCodeGo => &[
                "deepseek-v4.1-flash",
                "glm-5.3-flash",
                "glm-5.3",
                "kimi-k3",
                "minimax-m3.1-flash",
            ],
            AiProvider::Mock | AiProvider::OpenAiCompat => &[],
        }
    }
}

/// Persisted, non-secret preferences. `error` is load-time diagnostics only
/// and is never written back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppSettings {
    pub provider: AiProvider,
    pub base_url: String,
    pub model: String,
    pub error: Option<String>,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            provider: AiProvider::Mock,
            base_url: LOCAL_DEFAULT_BASE_URL.to_string(),
            model: String::new(),
            error: None,
        }
    }
}

impl AppSettings {
    pub fn load() -> Self {
        Self::load_from(&store_path())
    }

    pub fn load_from(file: &Path) -> Self {
        let text = match fs::read_to_string(file) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Self::default();
            }
            Err(error) => {
                return Self {
                    error: Some(format!("{}: {error}", file.display())),
                    ..Self::default()
                };
            }
        };
        match Self::from_json(&text) {
            Ok(settings) => settings,
            Err(error) => Self {
                error: Some(format!("{}: {error}", file.display())),
                ..Self::default()
            },
        }
    }

    fn from_json(text: &str) -> Result<Self, String> {
        let value: Value =
            serde_json::from_str(text).map_err(|error| format!("invalid JSON: {error}"))?;
        let object = value
            .as_object()
            .ok_or_else(|| "settings must be a JSON object".to_string())?;
        let version = object
            .get("schema_version")
            .and_then(Value::as_u64)
            .ok_or_else(|| "missing schema_version".to_string())?;
        if version != SETTINGS_VERSION {
            return Err(format!(
                "unsupported settings schema_version {version} (expected {SETTINGS_VERSION})"
            ));
        }
        let provider_key = object
            .get("provider")
            .and_then(Value::as_str)
            .ok_or_else(|| "missing provider".to_string())?;
        let provider = AiProvider::parse(provider_key)
            .ok_or_else(|| format!("unknown provider `{provider_key}`"))?;
        let base_url = object
            .get("base_url")
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| {
                provider
                    .fixed_base_url()
                    .unwrap_or(LOCAL_DEFAULT_BASE_URL)
                    .to_string()
            });
        let model = object
            .get("model")
            .and_then(Value::as_str)
            .unwrap_or(provider.default_model())
            .to_string();
        Ok(Self {
            provider,
            base_url,
            model,
            error: None,
        })
    }

    /// Save only the non-secret fields. Atomic temp+rename in the settings
    /// directory; a failure leaves the previous file intact.
    pub fn save(&self) -> Result<(), String> {
        self.save_to(&store_path())
    }

    pub fn save_to(&self, file: &Path) -> Result<(), String> {
        let Some(parent) = file.parent() else {
            return Err(format!("{} has no parent directory", file.display()));
        };
        fs::create_dir_all(parent).map_err(|error| format!("{}: {error}", parent.display()))?;
        let text = serde_json::to_string_pretty(&json!({
            "schema_version": SETTINGS_VERSION,
            "provider": self.provider.key(),
            "base_url": self.base_url,
            "model": self.model,
        }))
        .map_err(|error| format!("could not encode settings: {error}"))?;
        let temp = file.with_extension("json.tmp");
        fs::write(&temp, text).map_err(|error| format!("{}: {error}", temp.display()))?;
        fs::rename(&temp, file).map_err(|error| format!("{}: {error}", file.display()))?;
        Ok(())
    }

    /// The selected provider's final chat base URL. Fixed gateways ignore a
    /// stale custom base URL from a previous provider.
    pub fn resolved_base_url(&self) -> String {
        self.provider
            .fixed_base_url()
            .map(str::to_string)
            .unwrap_or_else(|| self.base_url.clone())
    }
}

pub fn store_path() -> PathBuf {
    env_store_path().unwrap_or_else(|| crate::recent::config_dir().join(SETTINGS_FILE))
}

fn env_store_path() -> Option<PathBuf> {
    env::var_os("NARRA_SETTINGS_FILE")
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = env::temp_dir().join(format!(
            "narra-settings-{tag}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("temp dir");
        dir
    }

    #[test]
    fn provider_keys_round_trip_and_fixed_gateways_ignore_custom_urls() {
        for provider in AiProvider::ALL {
            assert_eq!(AiProvider::parse(provider.key()), Some(provider));
        }
        let settings = AppSettings {
            provider: AiProvider::OpenCodeGo,
            base_url: "http://stale".to_string(),
            model: "kimi-k3".to_string(),
            error: None,
        };
        assert_eq!(
            settings.resolved_base_url(),
            "https://opencode.ai/inference/go/openai/v1"
        );
        assert_eq!(AiProvider::OpenCode.default_model(), "deepseek-v4.1-flash");
        assert!(AiProvider::OpenCodeGo.requires_key());
        assert!(!AiProvider::Mock.requires_key());
    }

    #[test]
    fn settings_round_trip_without_secrets() {
        let dir = temp_dir("roundtrip");
        let file = dir.join("nested/settings.json");
        let settings = AppSettings {
            provider: AiProvider::OpenCode,
            base_url: "https://ignored".to_string(),
            model: "deepseek-v4.1-flash".to_string(),
            error: None,
        };
        settings.save_to(&file).expect("save");
        let text = fs::read_to_string(&file).expect("read");
        assert!(!text.contains("api_key"));
        assert!(!text.contains("sk-"));
        let loaded = AppSettings::load_from(&file);
        assert_eq!(loaded, settings);
        assert!(loaded.error.is_none());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn unknown_versions_and_providers_fail_closed() {
        let dir = temp_dir("invalid");
        for body in [
            r#"{"schema_version":99,"provider":"mock","base_url":"x","model":""}"#,
            r#"{"schema_version":1,"provider":"not-a-provider","base_url":"x","model":""}"#,
            r#"{"provider":"mock"}"#,
        ] {
            let file = dir.join("settings.json");
            fs::write(&file, body).expect("write");
            let loaded = AppSettings::load_from(&file);
            assert_eq!(loaded.provider, AiProvider::Mock);
            assert!(loaded.error.is_some(), "body {body}");
        }
        let _ = fs::remove_dir_all(&dir);
    }
}
