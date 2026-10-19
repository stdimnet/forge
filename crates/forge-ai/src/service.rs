//! `AiService`: owns the provider config + key store and builds providers on demand.
//! This is what the app's IPC layer talks to.

use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use std::time::Instant;

use serde::Serialize;

use crate::anthropic::AnthropicProvider;
use crate::config::{AiConfig, AiFeature, ModelSelection, ProviderConfig};
use crate::error::{AiError, AiResult};
use crate::openai::OpenAiCompatProvider;
use crate::provider::{http_client, Endpoint, Provider};
use crate::secrets::{resolve_api_key, KeySource, SecretStore};
use crate::types::ModelInfo;

type EnvLookup = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

/// A provider as shown in Settings (never contains the key).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderInfo {
    #[serde(flatten)]
    pub config: ProviderConfig,
    pub has_api_key: bool,
    pub key_source: KeySource,
    pub is_default: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProvidersState {
    pub providers: Vec<ProviderInfo>,
    pub default_provider: Option<String>,
    pub features: crate::config::FeatureModels,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TestResult {
    pub ok: bool,
    pub latency_ms: u64,
    pub error: Option<String>,
    pub model_count: Option<usize>,
}

pub struct AiService {
    path: Option<PathBuf>,
    config: RwLock<AiConfig>,
    secrets: Arc<dyn SecretStore>,
    env: EnvLookup,
    client: reqwest::Client,
}

impl AiService {
    /// Load config from `path` (`None` keeps it in memory only).
    pub fn new(path: Option<PathBuf>, secrets: Arc<dyn SecretStore>) -> AiResult<Self> {
        let config = match &path {
            Some(p) => AiConfig::load(p)?,
            None => AiConfig::default(),
        };
        Ok(Self {
            path,
            config: RwLock::new(config),
            secrets,
            env: Arc::new(|k| std::env::var(k).ok()),
            client: http_client(),
        })
    }

    pub fn with_env(mut self, env: impl Fn(&str) -> Option<String> + Send + Sync + 'static) -> Self {
        self.env = Arc::new(env);
        self
    }

    pub fn with_client(mut self, client: reqwest::Client) -> Self {
        self.client = client;
        self
    }

    pub fn config(&self) -> AiConfig {
        self.config.read().expect("ai config lock").clone()
    }

    fn update(&self, f: impl FnOnce(&mut AiConfig) -> AiResult<()>) -> AiResult<()> {
        let mut cfg = self.config.write().expect("ai config lock");
        let mut next = cfg.clone();
        f(&mut next)?;
        if let Some(p) = &self.path {
            next.save(p)?;
        }
        *cfg = next;
        Ok(())
    }

    pub fn state(&self) -> ProvidersState {
        let cfg = self.config();
        let default = cfg.default_provider().map(|p| p.id.clone());
        let providers = cfg
            .providers
            .iter()
            .map(|p| {
                let (key, source) = resolve_api_key(self.secrets.as_ref(), p, &*self.env);
                ProviderInfo {
                    config: p.clone(),
                    has_api_key: key.is_some(),
                    key_source: source,
                    is_default: default.as_deref() == Some(p.id.as_str()),
                }
            })
            .collect();
        ProvidersState {
            providers,
            default_provider: default,
            features: cfg.features,
        }
    }

    /// Create or update a provider. `api_key`: `Some("")` removes the stored key,
    /// `None` leaves it unchanged. An empty id gets a fresh one derived from the name.
    pub fn save_provider(&self, mut cfg: ProviderConfig, api_key: Option<&str>) -> AiResult<ProviderConfig> {
        if cfg.name.trim().is_empty() {
            return Err(AiError::Config("provider name must not be empty".into()));
        }
        if !(cfg.base_url.starts_with("http://") || cfg.base_url.starts_with("https://")) {
            return Err(AiError::Config(format!("invalid base URL: {}", cfg.base_url)));
        }
        if cfg.id.trim().is_empty() {
            cfg.id = self.config().unique_id(&cfg.name);
        }
        cfg.default_model = cfg.default_model.filter(|m| !m.trim().is_empty());
        match api_key.map(str::trim) {
            Some("") => self.secrets.delete(&cfg.id)?,
            Some(k) => self.secrets.set(&cfg.id, k)?,
            None => {}
        }
        let saved = cfg.clone();
        self.update(|c| {
            c.upsert(cfg);
            Ok(())
        })?;
        Ok(saved)
    }

    pub fn delete_provider(&self, id: &str) -> AiResult<()> {
        self.update(|c| {
            if c.remove(id) {
                Ok(())
            } else {
                Err(AiError::UnknownProvider(id.into()))
            }
        })?;
        if let Err(e) = self.secrets.delete(id) {
            tracing::warn!("could not remove key for {id}: {e}");
        }
        Ok(())
    }

    /// Set the default provider (and its default model), or a feature's model when
    /// `feature` is given.
    pub fn set_default(&self, provider: &str, model: Option<&str>, feature: Option<AiFeature>) -> AiResult<()> {
        self.update(|c| {
            let p = c
                .providers
                .iter_mut()
                .find(|p| p.id == provider)
                .ok_or_else(|| AiError::UnknownProvider(provider.into()))?;
            let model = model.map(str::trim).filter(|m| !m.is_empty());
            match feature {
                None => {
                    if let Some(m) = model {
                        p.default_model = Some(m.into());
                    }
                    c.default_provider = Some(provider.into());
                }
                Some(f) => {
                    let model = model
                        .map(String::from)
                        .or_else(|| p.default_model.clone())
                        .ok_or_else(|| AiError::NoModel(p.name.clone()))?;
                    c.features.set(
                        f,
                        Some(ModelSelection {
                            provider: provider.into(),
                            model,
                        }),
                    );
                }
            }
            Ok(())
        })
    }

    pub fn clear_feature(&self, feature: AiFeature) -> AiResult<()> {
        self.update(|c| {
            c.features.set(feature, None);
            Ok(())
        })
    }

    /// Build a provider from a config (not necessarily saved).
    pub fn build(&self, cfg: &ProviderConfig) -> Box<dyn Provider> {
        let (api_key, _) = resolve_api_key(self.secrets.as_ref(), cfg, &*self.env);
        let ep = Endpoint {
            name: cfg.name.clone(),
            kind: cfg.kind,
            base_url: cfg.base_url.clone(),
            api_key,
            headers: cfg.headers.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
            default_model: cfg.default_model.clone(),
            client: self.client.clone(),
        };
        if cfg.kind.is_anthropic() {
            Box::new(AnthropicProvider::new(ep))
        } else {
            Box::new(OpenAiCompatProvider::new(ep))
        }
    }

    pub fn provider(&self, id: &str) -> AiResult<Box<dyn Provider>> {
        let cfg = self.config();
        let p = cfg
            .provider(id)
            .ok_or_else(|| AiError::UnknownProvider(id.into()))?;
        Ok(self.build(p))
    }

    /// Pick provider + model for a request: explicit values win, then the feature's
    /// selection, then the default provider and its default model.
    pub fn resolve(
        &self,
        provider: Option<&str>,
        model: Option<&str>,
        feature: Option<AiFeature>,
    ) -> AiResult<(Box<dyn Provider>, Option<String>)> {
        let cfg = self.config();
        let feature_sel = feature.and_then(|f| cfg.features.get(f)).cloned();
        let provider_id = provider
            .map(String::from)
            .or_else(|| feature_sel.as_ref().map(|s| s.provider.clone()))
            .or_else(|| cfg.default_provider().map(|p| p.id.clone()))
            .ok_or(AiError::NoProvider)?;
        let model = model.map(String::from).or_else(|| {
            feature_sel
                .filter(|s| s.provider == provider_id)
                .map(|s| s.model)
        });
        Ok((self.provider(&provider_id)?, model))
    }

    pub async fn models(&self, id: &str) -> AiResult<Vec<ModelInfo>> {
        self.provider(id)?.list_models().await
    }

    /// Check a provider by listing its models.
    pub async fn test(&self, id: &str) -> AiResult<TestResult> {
        let provider = self.provider(id)?;
        let start = Instant::now();
        let res = provider.list_models().await;
        let latency_ms = start.elapsed().as_millis() as u64;
        Ok(match res {
            Ok(models) => TestResult {
                ok: true,
                latency_ms,
                error: None,
                model_count: Some(models.len()),
            },
            Err(e) => TestResult {
                ok: false,
                latency_ms,
                error: Some(e.to_string()),
                model_count: None,
            },
        })
    }
}
