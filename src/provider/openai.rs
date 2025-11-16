use anyhow::{anyhow, Context, Result};
use async_stream::try_stream;
use async_trait::async_trait;
use futures::{pin_mut, StreamExt};
use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::config::ApiKeyProviderConfig;
use crate::provider::{ChatMessage, ChatRequestOptions, MessageRole, Provider};
use crate::secrets;
use crate::streaming::ChatStream;

const DEFAULT_BASE_URL: &str = "https://api.openai.com";

pub struct OpenAiProvider {
    name: String,
    config: ApiKeyProviderConfig,
    client: Client,
    api_key: String,
    base_url: String,
}

impl OpenAiProvider {
    pub async fn new(
        name: String,
        mut config: ApiKeyProviderConfig,
        passphrase: Option<&str>,
        env_label: &str,
    ) -> Result<Self> {
        let api_key = secrets::require_secret(
            config.api_key.as_deref(),
            config.encrypted_api_key.as_ref(),
            passphrase,
            env_label,
            &format!("openai provider '{name}' requires --api-key"),
        )?;
        config.api_key = Some(api_key.clone());
        config.encrypted_api_key = None;
        let client = Client::builder().build()?;
        let base_url = config
            .base_url
            .clone()
            .unwrap_or_else(|| DEFAULT_BASE_URL.to_string());

        Ok(Self {
            name,
            config,
            client,
            api_key,
            base_url,
        })
    }

    fn endpoint(&self) -> String {
        format!(
            "{}/v1/chat/completions",
            self.base_url.trim_end_matches('/')
        )
    }

    fn build_payload(
        &self,
        model: &str,
        system: Option<&str>,
        messages: &[ChatMessage],
        options: &ChatRequestOptions,
        stream: bool,
    ) -> OpenAiRequest {
        let selected_model = if model.trim().is_empty() {
            self.config
                .default_model
                .clone()
                .unwrap_or_else(|| "gpt-4o-mini".to_string())
        } else {
            model.to_string()
        };

        let mut converted = Vec::new();
        if let Some(system_prompt) = system {
            converted.push(OpenAiMessage::new("system", system_prompt));
        }
        for msg in messages {
            match msg.role {
                MessageRole::System => {
                    converted.push(OpenAiMessage::new("system", &msg.content));
                }
                MessageRole::User => converted.push(OpenAiMessage::new("user", &msg.content)),
                MessageRole::Assistant => {
                    converted.push(OpenAiMessage::new("assistant", &msg.content))
                }
            }
        }

        OpenAiRequest {
            model: selected_model,
            messages: converted,
            max_tokens: options.max_output_tokens,
            temperature: options.temperature,
            stream,
        }
    }

    fn request_builder(&self) -> reqwest::RequestBuilder {
        self.client
            .post(self.endpoint())
            .header("authorization", format!("Bearer {}", self.api_key))
    }

    fn parse_stream_event(payload: &str) -> Result<Vec<String>> {
        let trimmed = payload.trim();
        if trimmed.is_empty() || trimmed == "[DONE]" {
            return Ok(Vec::new());
        }

        let chunk: OpenAiStreamChunk = serde_json::from_str(trimmed)
            .with_context(|| format!("failed to parse openai stream chunk: {trimmed}"))?;
        let mut texts = Vec::new();
        for choice in chunk.choices {
            if let Some(delta) = choice.delta {
                if let Some(content) = delta.content {
                    if !content.is_empty() {
                        texts.push(content);
                    }
                }
            }
        }
        Ok(texts)
    }
}

#[async_trait]
impl Provider for OpenAiProvider {
    async fn chat(
        &self,
        model: &str,
        system: Option<&str>,
        messages: &[ChatMessage],
        options: &ChatRequestOptions,
    ) -> Result<String> {
        let payload = self.build_payload(model, system, messages, options, false);
        let response = self
            .request_builder()
            .json(&payload)
            .send()
            .await
            .context("openai request failed")?
            .error_for_status()
            .context("openai api error")?
            .json::<OpenAiResponse>()
            .await
            .context("failed to parse openai response")?;

        response
            .choices
            .first()
            .and_then(|choice| choice.message.content.clone())
            .filter(|text| !text.is_empty())
            .ok_or_else(|| anyhow!("openai response missing content"))
    }

    async fn stream_chat(
        &self,
        model: &str,
        system: Option<&str>,
        messages: &[ChatMessage],
        options: &ChatRequestOptions,
    ) -> Result<ChatStream> {
        let payload = self.build_payload(model, system, messages, options, true);
        let response = self
            .request_builder()
            .header("accept", "text/event-stream")
            .json(&payload)
            .send()
            .await
            .context("openai stream request failed")?
            .error_for_status()
            .context("openai stream api error")?;

        let body = response.bytes_stream();
        let stream = try_stream! {
            let mut buffer = String::new();
            let mut event_payload = String::new();
            pin_mut!(body);

            while let Some(chunk) = body.next().await {
                let chunk = chunk.context("openai stream chunk error")?;
                let text = String::from_utf8_lossy(&chunk);
                buffer.push_str(&text);

                while let Some(pos) = buffer.find('\n') {
                    let mut line: String = buffer.drain(..=pos).collect();
                    if line.ends_with('\n') {
                        line.pop();
                    }
                    if line.ends_with('\r') {
                        line.pop();
                    }

                    if line.is_empty() {
                        if !event_payload.is_empty() {
                            for text in Self::parse_stream_event(&event_payload)? {
                                if !text.is_empty() {
                                    yield text;
                                }
                            }
                            event_payload.clear();
                        }
                        continue;
                    }

                    if let Some(rest) = line.strip_prefix("data:") {
                        let trimmed = rest.trim_start();
                        if trimmed == "[DONE]" {
                            event_payload.clear();
                            continue;
                        }
                        event_payload.push_str(trimmed);
                    }
                }
            }

            if !event_payload.trim().is_empty() {
                for text in Self::parse_stream_event(&event_payload)? {
                    if !text.is_empty() {
                        yield text;
                    }
                }
            }
        };

        Ok(Box::pin(stream) as ChatStream)
    }
}

#[derive(Serialize)]
struct OpenAiRequest {
    model: String,
    messages: Vec<OpenAiMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(default)]
    stream: bool,
}

#[derive(Serialize)]
struct OpenAiMessage {
    role: String,
    content: String,
}

impl OpenAiMessage {
    fn new(role: &str, content: &str) -> Self {
        Self {
            role: role.to_string(),
            content: content.to_string(),
        }
    }
}

#[derive(Deserialize)]
struct OpenAiResponse {
    choices: Vec<OpenAiChoice>,
}

#[derive(Deserialize)]
struct OpenAiChoice {
    message: OpenAiChoiceMessage,
    #[allow(dead_code)]
    finish_reason: Option<String>,
}

#[derive(Deserialize)]
struct OpenAiChoiceMessage {
    role: Option<String>,
    content: Option<String>,
}

#[derive(Deserialize)]
struct OpenAiStreamChunk {
    choices: Vec<OpenAiStreamChoice>,
}

#[derive(Deserialize)]
struct OpenAiStreamChoice {
    delta: Option<OpenAiStreamDelta>,
    #[allow(dead_code)]
    finish_reason: Option<String>,
}

#[derive(Deserialize)]
struct OpenAiStreamDelta {
    #[allow(dead_code)]
    role: Option<String>,
    content: Option<String>,
}
