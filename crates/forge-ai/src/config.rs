//! Provider configuration, persisted as TOML in `<config dir>/forge/ai.toml`.
//! API keys are never written here; see [`crate::secrets`].

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{AiError, AiResult};

pub const ANTHROPIC_DEFAULT_MODEL: &str = "claude-sonnet-5-5";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProviderKind {
    Anthropic,
    OpenAi,
    OpenRouter,
    LmStudio,
    Ollama,
    LlamaCpp,
    /// Any other OpenAI-compatible server.
    Custom,
}

impl ProviderKind {
    pub const ALL: [ProviderKind; 7] = [
        ProviderKind::Anthropic,
        ProviderKind::OpenAi,
        ProviderKind::OpenRouter,
        ProviderKind::Ollama,
        ProviderKind::LmStudio,
        ProviderKind::LlamaCpp,
        ProviderKind::Custom,
    ];

    pub fn display_name(self) -> &'static str {
        match self {
            ProviderKind::Anthropic => "Anthropic",
            ProviderKind::OpenAi => "OpenAI",
            ProviderKind::OpenRouter => "OpenRouter",
            ProviderKind::LmStudio => "LM Studio",
            ProviderKind::Ollama => "Ollama",
            ProviderKind::LlamaCpp => "llama.cpp",
            ProviderKind::Custom => "OpenAI-compatible",
        }
    }

    pub fn default_base_url(self) -> &'static str {
        match self {
            ProviderKind::Anthropic => "https://api.anthropic.com/v1",
            ProviderKind::OpenAi => "https://api.openai.com/v1",
            ProviderKind::OpenRouter => "https://openrouter.ai/api/v1",
            ProviderKind::LmStudio => "http://localhost:1234/v1",
            ProviderKind::Ollama => "http://localhost:11434/v1",
            ProviderKind::LlamaCpp => "http://localhost:8080/v1",
            ProviderKind::Custom => "http://localhost:8000/v1",
        }
    }

    pub fn default_model(self) -> Option<&'static str> {
        match self {
            ProviderKind::Anthropic => Some(ANTHROPIC_DEFAULT_MODEL),
            ProviderKind::OpenAi => Some("gpt-4.1"),
            ProviderKind::OpenRouter => Some("anthropic/claude-sonnet-5.5"),
            _ => None,
        }
    }

    /// Hosted APIs need a key; local servers usually don't.
    pub fn requires_api_key(self) -> bool {
        matches!(
            self,
            ProviderKind::Anthropic | ProviderKind::OpenAi | ProviderKind::OpenRouter
        )
    }

    /// Environment variable consulted when the keychain has no key.
    pub fn env_var(self) -> Option<&'static str> {
        match self {
            ProviderKind::Anthropic => Some("ANTHROPIC_API_KEY"),
            ProviderKind::OpenAi => Some("OPENAI_API_KEY"),
            ProviderKind::OpenRouter => Some("OPENROUTER_API_KEY"),
            _ => None,
        }
    }

    /// Servers whose OpenAI-compatible `completions` endpoint takes a `suffix`
    /// (fill-in-the-middle), the usual setup for local code models.
    pub fn supports_fim(self) -> bool {
        matches!(
            self,
            ProviderKind::Ollama | ProviderKind::LmStudio | ProviderKind::LlamaCpp | ProviderKind::Custom
        )
    }

    /// Runs on this machine (by default): code sent to it stays local.
    pub fn is_local(self) -> bool {
        matches!(
            self,
            ProviderKind::Ollama | ProviderKind::LmStudio | ProviderKind::LlamaCpp
        )
    }

    pub fn is_anthropic(self) -> bool {
        self == ProviderKind::Anthropic
    }

    /// Hint shown when the server cannot be reached.
    pub fn connection_hint(self) -> Option<&'static str> {
        match self {
            ProviderKind::LmStudio => {
                Some("is LM Studio running? Start the local server in LM Studio's Developer tab")
            }
            ProviderKind::Ollama => Some("is Ollama running? Start it with `ollama serve`"),
            ProviderKind::LlamaCpp => {
                Some("is llama.cpp running? Start it with `llama-server -m <model.gguf>`")
            }
            ProviderKind::Custom => Some("is the server running and the base URL correct?"),
            _ => Some("check your network connection and proxy settings"),
        }
    }

    pub fn default_headers(self) -> BTreeMap<String, String> {
        let mut h = BTreeMap::new();
        if self == ProviderKind::OpenRouter {
            h.insert(
                "HTTP-Referer".into(),
                "https://github.com/Reeperk/Forge".into(),
            );
            h.insert("X-Title".into(), "Forge IDE".into());
        }
        h
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderConfig {
    pub id: String,
    pub kind: ProviderKind,
    pub name: String,
    pub base_url: String,
    #[serde(default)]
    pub default_model: Option<String>,
    #[serde(default = "yes")]
    pub enabled: bool,
    /// Extra HTTP headers sent with every request.
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
}

fn yes() -> bool {
    true
}

impl ProviderConfig {
    /// A config pre-filled for `kind` (the "Add provider" presets).
    pub fn preset(kind: ProviderKind, id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            kind,
            name: kind.display_name().into(),
            base_url: kind.default_base_url().into(),
            default_model: kind.default_model().map(Into::into),
            headers: kind.default_headers(),
            enabled: true,
        }
    }
}

/// Features that pick their own model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AiFeature {
    Chat,
    CommitMessages,
    Completion,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelSelection {
    pub provider: String,
    pub model: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeatureModels {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chat: Option<ModelSelection>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commit_messages: Option<ModelSelection>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completion: Option<ModelSelection>,
}

impl FeatureModels {
    pub fn get(&self, f: AiFeature) -> Option<&ModelSelection> {
        match f {
            AiFeature::Chat => self.chat.as_ref(),
            AiFeature::CommitMessages => self.commit_messages.as_ref(),
            AiFeature::Completion => self.completion.as_ref(),
        }
    }

    pub fn set(&mut self, f: AiFeature, sel: Option<ModelSelection>) {
        match f {
            AiFeature::Chat => self.chat = sel,
            AiFeature::CommitMessages => self.commit_messages = sel,
            AiFeature::Completion => self.completion = sel,
        }
    }

    fn retain(&mut self, keep: impl Fn(&ModelSelection) -> bool) {
        for f in [
            AiFeature::Chat,
            AiFeature::CommitMessages,
            AiFeature::Completion,
        ] {
            if self.get(f).is_some_and(|s| !keep(s)) {
                self.set(f, None);
            }
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_provider: Option<String>,
    #[serde(default)]
    pub features: FeatureModels,
    #[serde(default)]
    pub providers: Vec<ProviderConfig>,
}

impl AiConfig {
    /// `<config dir>/forge/ai.toml`.
    pub fn default_path() -> Option<PathBuf> {
        dirs::config_dir().map(|d| d.join("forge").join("ai.toml"))
    }

    /// Load from `path`; a missing file yields the default config.
    pub fn load(path: &Path) -> AiResult<Self> {
        match std::fs::read_to_string(path) {
            Ok(s) => {
                toml::from_str(&s).map_err(|e| AiError::Config(format!("{}: {e}", path.display())))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(AiError::Config(format!("{}: {e}", path.display()))),
        }
    }

    pub fn save(&self, path: &Path) -> AiResult<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)
                .map_err(|e| AiError::Config(format!("{}: {e}", dir.display())))?;
        }
        let s = toml::to_string_pretty(self).map_err(|e| AiError::Config(e.to_string()))?;
        std::fs::write(path, s).map_err(|e| AiError::Config(format!("{}: {e}", path.display())))
    }

    pub fn provider(&self, id: &str) -> Option<&ProviderConfig> {
        self.providers.iter().find(|p| p.id == id)
    }

    /// Insert or replace (by id). The first provider becomes the default.
    pub fn upsert(&mut self, cfg: ProviderConfig) {
        match self.providers.iter_mut().find(|p| p.id == cfg.id) {
            Some(p) => *p = cfg,
            None => self.providers.push(cfg),
        }
        if self.default_provider.is_none() {
            self.default_provider = self.providers.first().map(|p| p.id.clone());
        }
    }

    pub fn remove(&mut self, id: &str) -> bool {
        let before = self.providers.len();
        self.providers.retain(|p| p.id != id);
        self.features.retain(|s| s.provider != id);
        if self.default_provider.as_deref() == Some(id) {
            self.default_provider = self
                .providers
                .iter()
                .find(|p| p.enabled)
                .map(|p| p.id.clone());
        }
        before != self.providers.len()
    }

    /// Provider used when none is given explicitly: the configured default if enabled,
    /// otherwise the first enabled provider.
    pub fn default_provider(&self) -> Option<&ProviderConfig> {
        self.default_provider
            .as_deref()
            .and_then(|id| self.provider(id))
            .filter(|p| p.enabled)
            .or_else(|| self.providers.iter().find(|p| p.enabled))
    }

    /// A fresh id like `ollama`, `ollama-2`, ...
    pub fn unique_id(&self, base: &str) -> String {
        let base: String = base
            .to_lowercase()
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
            .collect();
        let base = base.trim_matches('-');
        let base = if base.is_empty() { "provider" } else { base };
        if self.provider(base).is_none() {
            return base.to_string();
        }
        (2..)
            .map(|n| format!("{base}-{n}"))
            .find(|id| self.provider(id).is_none())
            .expect("unbounded")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_through_toml() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("forge/ai.toml");
        assert_eq!(AiConfig::load(&path).unwrap(), AiConfig::default());

        let mut cfg = AiConfig::default();
        cfg.upsert(ProviderConfig::preset(ProviderKind::Anthropic, "anthropic"));
        cfg.upsert(ProviderConfig::preset(
            ProviderKind::OpenRouter,
            "openrouter",
        ));
        cfg.features.set(
            AiFeature::CommitMessages,
            Some(ModelSelection {
                provider: "openrouter".into(),
                model: "openai/gpt-4.1-mini".into(),
            }),
        );
        cfg.save(&path).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("kind = \"openrouter\""), "{text}");
        assert!(text.contains("X-Title"), "{text}");
        assert_eq!(AiConfig::load(&path).unwrap(), cfg);
    }

    #[test]
    fn first_provider_becomes_default_and_remove_reassigns() {
        let mut cfg = AiConfig::default();
        cfg.upsert(ProviderConfig::preset(ProviderKind::Ollama, "ollama"));
        cfg.upsert(ProviderConfig::preset(ProviderKind::LmStudio, "lmstudio"));
        cfg.features.set(
            AiFeature::Chat,
            Some(ModelSelection {
                provider: "ollama".into(),
                model: "qwen3".into(),
            }),
        );
        assert_eq!(cfg.default_provider().unwrap().id, "ollama");
        assert!(cfg.remove("ollama"));
        assert_eq!(cfg.default_provider().unwrap().id, "lmstudio");
        assert_eq!(cfg.features.chat, None);
    }

    #[test]
    fn disabled_default_falls_back_to_enabled_provider() {
        let mut cfg = AiConfig::default();
        let mut a = ProviderConfig::preset(ProviderKind::Anthropic, "anthropic");
        a.enabled = false;
        cfg.upsert(a);
        cfg.upsert(ProviderConfig::preset(ProviderKind::Ollama, "ollama"));
        assert_eq!(cfg.default_provider().unwrap().id, "ollama");
    }

    #[test]
    fn unique_ids() {
        let mut cfg = AiConfig::default();
        assert_eq!(cfg.unique_id("LM Studio"), "lm-studio");
        cfg.upsert(ProviderConfig::preset(ProviderKind::LmStudio, "lm-studio"));
        assert_eq!(cfg.unique_id("LM Studio"), "lm-studio-2");
    }
}
