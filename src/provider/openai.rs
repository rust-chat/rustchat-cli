//! Placeholder module for future OpenAI provider implementation.

use anyhow::Result;

use crate::config::ApiKeyProviderConfig;

pub struct OpenAiProvider {
    #[allow(dead_code)]
    name: String,
    #[allow(dead_code)]
    config: ApiKeyProviderConfig,
}

impl OpenAiProvider {
    pub async fn new(name: String, config: ApiKeyProviderConfig) -> Result<Self> {
        Ok(Self { name, config })
    }
}
