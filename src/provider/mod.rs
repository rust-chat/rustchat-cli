mod trait_provider;

pub mod anthropic;
pub mod google;
pub mod openai;

use anyhow::{anyhow, Result};
use trait_provider::Provider;

pub use trait_provider::{ChatMessage, ChatRequestOptions, DynProvider, MessageRole};

use crate::config::ProviderConfig;

pub async fn build_provider(
    name: &str,
    cfg: &ProviderConfig,
) -> Result<trait_provider::DynProvider> {
    Ok(match cfg {
        ProviderConfig::Google(google_cfg) => {
            let provider = google::GoogleProvider::new(name.into(), google_cfg.clone()).await?;
            std::sync::Arc::new(provider) as trait_provider::DynProvider
        }
        ProviderConfig::Anthropic(_) | ProviderConfig::Openai(_) => {
            return Err(anyhow!(
                "provider '{}' ({:?}) not implemented yet",
                name,
                cfg.kind()
            ));
        }
    })
}
