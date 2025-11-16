use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};

use crate::cli::ProviderKindArg;
use crate::secrets::{self, EncryptedSecret, DEFAULT_MASTER_ENV};

pub const APP_DIR: &str = "rustchat-cli";
const CONFIG_FILE: &str = "config.toml";

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AppConfig {
    pub default_provider: Option<String>,
    #[serde(default)]
    pub providers: BTreeMap<String, ProviderConfig>,
}

impl AppConfig {
    pub fn load() -> Result<Self> {
        let path = config_path()?;
        if !path.exists() {
            return Ok(Self::default());
        }
        let data = fs::read_to_string(&path)
            .with_context(|| format!("failed to read config at {}", path.display()))?;
        let cfg: AppConfig =
            toml::from_str(&data).with_context(|| "failed to parse config file (toml)")?;
        Ok(cfg)
    }

    pub fn save(&self) -> Result<()> {
        let path = config_path()?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create config dir {}", parent.display()))?;
        }
        let data = toml::to_string_pretty(self)?;
        fs::write(&path, data)
            .with_context(|| format!("failed to write config at {}", path.display()))?;
        Ok(())
    }

    pub fn upsert_provider(&mut self, name: String, cfg: ProviderConfig) {
        self.providers.insert(name, cfg);
    }

    pub fn remove_provider(&mut self, name: &str) -> bool {
        self.providers.remove(name).is_some()
    }

    pub fn require_provider(&self, provider: &str) -> Result<&ProviderConfig> {
        self.providers
            .get(provider)
            .ok_or_else(|| anyhow!("provider '{provider}' not found in config"))
    }

    pub fn infer_default_provider(&self, explicit: &Option<String>) -> Result<String> {
        if let Some(name) = explicit {
            return Ok(name.clone());
        }
        self.default_provider
            .clone()
            .ok_or_else(|| anyhow!("no provider selected and no default configured"))
    }
}

pub fn config_path() -> Result<PathBuf> {
    let base = dirs::config_dir().ok_or_else(|| anyhow!("unable to locate platform config dir"))?;
    Ok(base.join(APP_DIR).join(CONFIG_FILE))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum ProviderConfig {
    Google(GoogleProviderConfig),
    Anthropic(ApiKeyProviderConfig),
    Openai(ApiKeyProviderConfig),
}

impl ProviderConfig {
    pub fn kind(&self) -> ProviderKind {
        match self {
            ProviderConfig::Google(_) => ProviderKind::Google,
            ProviderConfig::Anthropic(_) => ProviderKind::Anthropic,
            ProviderConfig::Openai(_) => ProviderKind::Openai,
        }
    }

    pub fn default_model(&self) -> Option<&str> {
        match self {
            ProviderConfig::Google(cfg) => cfg.default_model.as_deref(),
            ProviderConfig::Anthropic(cfg) | ProviderConfig::Openai(cfg) => {
                cfg.default_model.as_deref()
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GoogleProviderConfig {
    pub service_account_file: Option<PathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub encrypted_api_key: Option<EncryptedSecret>,
    pub project_id: Option<String>,
    pub location: Option<String>,
    pub default_model: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ApiKeyProviderConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub encrypted_api_key: Option<EncryptedSecret>,
    pub base_url: Option<String>,
    pub default_model: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderKind {
    Google,
    Anthropic,
    Openai,
}

impl From<ProviderKindArg> for ProviderKind {
    fn from(value: ProviderKindArg) -> Self {
        match value {
            ProviderKindArg::Google => ProviderKind::Google,
            ProviderKindArg::Anthropic => ProviderKind::Anthropic,
            ProviderKindArg::Openai => ProviderKind::Openai,
        }
    }
}

impl ProviderKind {
    pub fn infer(name: &str) -> Option<Self> {
        ProviderKindArg::infer_from_name(name).map(ProviderKind::from)
    }
}

pub fn build_provider_config(
    kind: ProviderKind,
    set: &crate::cli::ConfigSetArgs,
) -> Result<ProviderConfig> {
    let env_label = set.secret_env.as_deref().unwrap_or(DEFAULT_MASTER_ENV);
    let passphrase = if set.encrypt_secrets {
        Some(secrets::require_passphrase_from_env(env_label)?)
    } else {
        None
    };
    Ok(match kind {
        ProviderKind::Google => {
            let (api_key, encrypted_api_key) = secrets::maybe_encrypt_secret(
                set.shared_api.api_key.clone(),
                set.encrypt_secrets,
                passphrase.as_deref(),
                env_label,
            )?;
            ProviderConfig::Google(GoogleProviderConfig {
                service_account_file: set.google.service_account.clone(),
                api_key,
                encrypted_api_key,
                project_id: set.google.project_id.clone(),
                location: set.google.location.clone(),
                default_model: set
                    .google
                    .default_model
                    .clone()
                    .or_else(|| set.shared_api.shared_default_model.clone()),
            })
        }
        ProviderKind::Anthropic => {
            let provided = set
                .shared_api
                .api_key
                .clone()
                .ok_or_else(|| anyhow!("--api-key is required for anthropic"))?;
            let (api_key, encrypted_api_key) = secrets::maybe_encrypt_secret(
                Some(provided),
                set.encrypt_secrets,
                passphrase.as_deref(),
                env_label,
            )?;
            ProviderConfig::Anthropic(ApiKeyProviderConfig {
                api_key,
                encrypted_api_key,
                base_url: set.shared_api.base_url.clone(),
                default_model: set.shared_api.shared_default_model.clone(),
            })
        }
        ProviderKind::Openai => {
            let provided = set
                .shared_api
                .api_key
                .clone()
                .ok_or_else(|| anyhow!("--api-key is required for openai"))?;
            let (api_key, encrypted_api_key) = secrets::maybe_encrypt_secret(
                Some(provided),
                set.encrypt_secrets,
                passphrase.as_deref(),
                env_label,
            )?;
            ProviderConfig::Openai(ApiKeyProviderConfig {
                api_key,
                encrypted_api_key,
                base_url: set.shared_api.base_url.clone(),
                default_model: set.shared_api.shared_default_model.clone(),
            })
        }
    })
}

pub fn ensure_permissions(path: &Path) -> Result<()> {
    #[cfg(not(unix))]
    let _ = path;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        const PERM: u32 = 0o600;
        let metadata = fs::metadata(path)?;
        if metadata.permissions().mode() & 0o777 != PERM {
            let mut perm = metadata.permissions();
            perm.set_mode(PERM);
            fs::set_permissions(path, perm)?;
        }
    }
    Ok(())
}
