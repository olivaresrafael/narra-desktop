//! Versioned application settings (M10 S4, expanded in S4c).
//!
//! Only non-secret preferences live here. API keys are written to the OS
//! keychain (`narra-core::provider::secrets`) and are deliberately absent
//! from this file, its `Debug` output and the dump.
//!
//! The store is a single JSON object with `schema_version`; unknown versions
//! fail closed (defaults + an explicit error) instead of being guessed.
//! Version 1 (AI provider only) migrates in place to version 2 by adding
//! defaults; the API key never existed in either version.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use narra_core::agent::LOCAL_DEFAULT_BASE_URL;
use serde_json::{Value, json};

use crate::i18n::Lang;
use crate::theme::ThemeMode;

pub const SETTINGS_VERSION: u64 = 2;
pub const SETTINGS_FILE: &str = "settings.json";

/// Persisted UI-language preference. `System` resolves from the macOS locale
/// on startup; explicit choices survive restarts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LanguageSetting {
    #[default]
    System,
    En,
    Es,
}

impl LanguageSetting {
    pub const ALL: [LanguageSetting; 3] = [
        LanguageSetting::System,
        LanguageSetting::En,
        LanguageSetting::Es,
    ];

    pub fn key(self) -> &'static str {
        match self {
            LanguageSetting::System => "system",
            LanguageSetting::En => "en",
            LanguageSetting::Es => "es",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|language| language.key() == value)
    }

    pub fn i18n_key(self) -> &'static str {
        match self {
            LanguageSetting::System => "settings.language.system",
            LanguageSetting::En => "settings.language.english",
            LanguageSetting::Es => "settings.language.spanish",
        }
    }

    /// Concrete UI language for the next render.
    pub fn resolved(self) -> Lang {
        match self {
            LanguageSetting::System => Lang::detect(),
            LanguageSetting::En => Lang::En,
            LanguageSetting::Es => Lang::Es,
        }
    }
}

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
#[derive(Debug, Clone, PartialEq)]
pub struct AppSettings {
    // AI (v1)
    pub provider: AiProvider,
    pub base_url: String,
    pub model: String,
    /// Optional manual prices per million tokens for the usage report. Zero
    /// means "not configured"; the app never guesses provider pricing.
    pub input_price_per_million: f64,
    pub output_price_per_million: f64,
    // General (v2)
    pub language: LanguageSetting,
    pub theme: ThemeMode,
    // Editor (v2)
    pub font_size: u16,
    pub word_wrap: bool,
    pub line_numbers: bool,
    pub show_config_files: bool,
    // Git (v2)
    pub git_name: String,
    pub git_email: String,
    // Load-time diagnostics; never persisted.
    pub error: Option<String>,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            provider: AiProvider::Mock,
            base_url: LOCAL_DEFAULT_BASE_URL.to_string(),
            model: String::new(),
            input_price_per_million: 0.0,
            output_price_per_million: 0.0,
            language: LanguageSetting::System,
            theme: ThemeMode::System,
            font_size: 14,
            word_wrap: true,
            line_numbers: true,
            show_config_files: false,
            git_name: String::new(),
            git_email: String::new(),
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
        if version != 1 && version != SETTINGS_VERSION {
            return Err(format!(
                "unsupported settings schema_version {version} (expected 1 or {SETTINGS_VERSION})"
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

        // v1 has only the AI fields; every added preference takes its default.
        let mut settings = Self {
            provider,
            base_url,
            model,
            ..Self::default()
        };
        settings.input_price_per_million = optional_f64(object, "input_price_per_million", 0.0)?;
        settings.output_price_per_million = optional_f64(object, "output_price_per_million", 0.0)?;
        if version >= 2 {
            settings.language = optional_enum(
                object,
                "language",
                LanguageSetting::parse,
                settings.language,
            )?;
            settings.theme = optional_enum(object, "theme", ThemeMode::parse, settings.theme)?;
            settings.font_size = optional_u16(object, "font_size", settings.font_size);
            settings.word_wrap = optional_bool(object, "word_wrap", settings.word_wrap)?;
            settings.line_numbers = optional_bool(object, "line_numbers", settings.line_numbers)?;
            settings.show_config_files =
                optional_bool(object, "show_config_files", settings.show_config_files)?;
            settings.git_name = optional_string(object, "git_name", &settings.git_name);
            settings.git_email = optional_string(object, "git_email", &settings.git_email);
        }
        settings.font_size = settings.font_size.clamp(12, 24);
        // The keychain is the only home for a key; refuse to load one even if
        // an older/hand-edited file accidentally carries it (never echo it).
        settings.error = None;
        Ok(settings)
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
            "input_price_per_million": self.input_price_per_million,
            "output_price_per_million": self.output_price_per_million,
            "language": self.language.key(),
            "theme": self.theme.key(),
            "font_size": self.font_size.clamp(12, 24),
            "word_wrap": self.word_wrap,
            "line_numbers": self.line_numbers,
            "show_config_files": self.show_config_files,
            "git_name": self.git_name,
            "git_email": self.git_email,
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

fn optional_f64(
    object: &serde_json::Map<String, Value>,
    key: &str,
    default: f64,
) -> Result<f64, String> {
    match object.get(key) {
        None => Ok(default),
        Some(value) => value
            .as_f64()
            .filter(|value| value.is_finite() && *value >= 0.0)
            .ok_or_else(|| format!("`{key}` must be a non-negative number")),
    }
}

fn optional_string(object: &serde_json::Map<String, Value>, key: &str, default: &str) -> String {
    object
        .get(key)
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| default.to_string())
}

fn optional_bool(
    object: &serde_json::Map<String, Value>,
    key: &str,
    default: bool,
) -> Result<bool, String> {
    match object.get(key) {
        None => Ok(default),
        Some(value) => value
            .as_bool()
            .ok_or_else(|| format!("`{key}` must be a boolean")),
    }
}

fn optional_u16(object: &serde_json::Map<String, Value>, key: &str, default: u16) -> u16 {
    object
        .get(key)
        .and_then(Value::as_u64)
        .and_then(|value| u16::try_from(value).ok())
        .unwrap_or(default)
}

fn optional_enum<T: Copy>(
    object: &serde_json::Map<String, Value>,
    key: &str,
    parse: impl Fn(&str) -> Option<T>,
    default: T,
) -> Result<T, String> {
    match object.get(key).and_then(Value::as_str) {
        None => Ok(default),
        Some(value) => parse(value).ok_or_else(|| format!("unknown `{key}` value `{value}`")),
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
            ..AppSettings::default()
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
            input_price_per_million: 0.15,
            output_price_per_million: 0.60,
            language: LanguageSetting::Es,
            theme: ThemeMode::Light,
            font_size: 17,
            word_wrap: false,
            line_numbers: false,
            show_config_files: true,
            git_name: "Rafael".to_string(),
            git_email: "rafael@example.com".to_string(),
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
    fn v1_settings_migrate_to_v2_defaults() {
        let dir = temp_dir("migration");
        let file = dir.join("settings.json");
        fs::write(
            &file,
            r#"{"schema_version":1,"provider":"opencode","base_url":"https://opencode.ai/inference/openai/v1","model":"deepseek-v4.1-flash"}"#,
        )
        .expect("write");
        let loaded = AppSettings::load_from(&file);
        assert!(loaded.error.is_none(), "{:?}", loaded.error);
        assert_eq!(loaded.provider, AiProvider::OpenCode);
        assert_eq!(loaded.language, LanguageSetting::System);
        assert_eq!(loaded.theme, ThemeMode::System);
        assert_eq!(loaded.font_size, 14);
        assert!(loaded.word_wrap);
        assert!(loaded.line_numbers);
        assert!(!loaded.show_config_files);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn unknown_versions_and_providers_fail_closed() {
        let dir = temp_dir("invalid");
        for body in [
            r#"{"schema_version":99,"provider":"mock","base_url":"x","model":""}"#,
            r#"{"schema_version":1,"provider":"not-a-provider","base_url":"x","model":""}"#,
            r#"{"provider":"mock"}"#,
            r#"{"schema_version":2,"provider":"mock","base_url":"x","model":"","theme":"neon"}"#,
        ] {
            let file = dir.join("settings.json");
            fs::write(&file, body).expect("write");
            let loaded = AppSettings::load_from(&file);
            assert_eq!(loaded.provider, AiProvider::Mock);
            assert!(loaded.error.is_some(), "body {body}");
        }
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn language_resolution_and_keys_are_stable() {
        assert_eq!(LanguageSetting::parse("es"), Some(LanguageSetting::Es));
        assert_eq!(LanguageSetting::Es.resolved(), Lang::Es);
        assert_eq!(LanguageSetting::En.resolved(), Lang::En);
        assert_eq!(ThemeMode::parse("light"), Some(ThemeMode::Light));
        assert_eq!(ThemeMode::Dark.i18n_key(), "settings.theme.dark");
    }
}
