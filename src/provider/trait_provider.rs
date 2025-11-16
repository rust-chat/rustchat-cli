use anyhow::{bail, Result};
use async_trait::async_trait;
use std::fmt;

use crate::streaming::ChatStream;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MessageRole {
    System,
    User,
    Assistant,
}

impl fmt::Display for MessageRole {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MessageRole::System => write!(f, "system"),
            MessageRole::User => write!(f, "user"),
            MessageRole::Assistant => write!(f, "assistant"),
        }
    }
}

#[derive(Clone, Debug)]
pub struct ChatMessage {
    pub role: MessageRole,
    pub content: String,
}

impl ChatMessage {
    pub fn new<S: Into<String>>(role: MessageRole, content: S) -> Self {
        Self {
            role,
            content: content.into(),
        }
    }

    pub fn system<S: Into<String>>(content: S) -> Self {
        Self::new(MessageRole::System, content)
    }

    pub fn user<S: Into<String>>(content: S) -> Self {
        Self::new(MessageRole::User, content)
    }

    pub fn assistant<S: Into<String>>(content: S) -> Self {
        Self::new(MessageRole::Assistant, content)
    }
}

#[derive(Clone, Debug, Default)]
pub struct ChatRequestOptions {
    pub temperature: Option<f32>,
    pub max_output_tokens: Option<u32>,
}

#[async_trait]
pub trait Provider: Send + Sync {
    async fn chat(
        &self,
        model: &str,
        system: Option<&str>,
        messages: &[ChatMessage],
        options: &ChatRequestOptions,
    ) -> Result<String>;

    async fn stream_chat(
        &self,
        _model: &str,
        _system: Option<&str>,
        _messages: &[ChatMessage],
        _options: &ChatRequestOptions,
    ) -> Result<ChatStream> {
        bail!("streaming not supported by this provider yet");
    }
}

pub type DynProvider = std::sync::Arc<dyn Provider>;
