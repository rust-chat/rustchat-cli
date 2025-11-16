//! Placeholder module for future Anthropic provider implementation.

use anyhow::Result;

use crate::config::ApiKeyProviderConfig;

pub struct AnthropicProvider {
    #[allow(dead_code)]
    name: String,
    #[allow(dead_code)]
    config: ApiKeyProviderConfig,
}

impl AnthropicProvider {
    pub async fn new(name: String, config: ApiKeyProviderConfig) -> Result<Self> {
        Ok(Self { name, config })
    }
}
