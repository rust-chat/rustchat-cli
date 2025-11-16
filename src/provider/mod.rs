mod trait_provider;

pub mod anthropic;
pub mod google;
pub mod openai;

use anyhow::Result;
use trait_provider::Provider;

pub use trait_provider::{ChatMessage, ChatRequestOptions, DynProvider, MessageRole};

use crate::config::ProviderConfig;

pub async fn build_provider(
    name: &str,
    cfg: &ProviderConfig,
    passphrase: Option<&str>,
    env_label: &str,
) -> Result<trait_provider::DynProvider> {
    Ok(match cfg {
        ProviderConfig::Google(google_cfg) => {
            let provider =
                google::GoogleProvider::new(name.into(), google_cfg.clone(), passphrase, env_label)
                    .await?;
            std::sync::Arc::new(provider) as trait_provider::DynProvider
        }
        ProviderConfig::Anthropic(anthropic_cfg) => {
            let provider = anthropic::AnthropicProvider::new(
                name.into(),
                anthropic_cfg.clone(),
                passphrase,
                env_label,
            )
            .await?;
            std::sync::Arc::new(provider) as trait_provider::DynProvider
        }
        ProviderConfig::Openai(openai_cfg) => {
            let provider =
                openai::OpenAiProvider::new(name.into(), openai_cfg.clone(), passphrase, env_label)
                    .await?;
            std::sync::Arc::new(provider) as trait_provider::DynProvider
        }
    })
}
