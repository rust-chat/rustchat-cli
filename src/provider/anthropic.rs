use anyhow::{anyhow, Context, Result};
use async_stream::try_stream;
use async_trait::async_trait;
use futures::{pin_mut, StreamExt};
use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::config::ApiKeyProviderConfig;
use crate::provider::{ChatMessage, ChatRequestOptions, MessageRole, Provider};
use crate::streaming::ChatStream;

const DEFAULT_BASE_URL: &str = "https://api.anthropic.com";
const ANTHROPIC_VERSION: &str = "2023-06-01";

pub struct AnthropicProvider {
    name: String,
    config: ApiKeyProviderConfig,
    client: Client,
    api_key: String,
    base_url: String,
}

impl AnthropicProvider {
    pub async fn new(name: String, config: ApiKeyProviderConfig) -> Result<Self> {
        let api_key = config
            .api_key
            .clone()
            .ok_or_else(|| anyhow!("anthropic provider '{name}' requires --api-key"))?;
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
        format!("{}/v1/messages", self.base_url.trim_end_matches('/'))
    }

    fn build_payload(
        &self,
        model: &str,
        system: Option<&str>,
        messages: &[ChatMessage],
        options: &ChatRequestOptions,
        stream: bool,
    ) -> AnthropicRequest {
        let model = if model.trim().is_empty() {
            self.config
                .default_model
                .clone()
                .unwrap_or_else(|| "claude-3-sonnet-20240229".to_string())
        } else {
            model.to_string()
        };

        let mut system_prompts: Vec<String> = Vec::new();
        if let Some(s) = system {
            system_prompts.push(s.to_string());
        }

        let mut converted = Vec::new();
        for msg in messages {
            match msg.role {
                MessageRole::System => system_prompts.push(msg.content.clone()),
                _ => converted.push(AnthropicMessage::from_chat(msg)),
            }
        }

        AnthropicRequest {
            model,
            max_tokens: options.max_output_tokens.unwrap_or(1024).max(1),
            temperature: options.temperature,
            system: if system_prompts.is_empty() {
                None
            } else {
                Some(system_prompts.join("\n"))
            },
            messages: converted,
            stream,
        }
    }

    fn request_builder(&self) -> reqwest::RequestBuilder {
        self.client
            .post(self.endpoint())
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", ANTHROPIC_VERSION)
    }

    fn parse_stream_event(payload: &str) -> Result<Vec<String>> {
        let trimmed = payload.trim();
        if trimmed.is_empty() || trimmed == "[DONE]" {
            return Ok(Vec::new());
        }

        let event: AnthropicStreamEvent = serde_json::from_str(trimmed)
            .with_context(|| format!("failed to parse anthropic stream event: {trimmed}"))?;
        if let Some(text) = event.text_fragment() {
            Ok(vec![text.to_string()])
        } else {
            Ok(Vec::new())
        }
    }
}

#[async_trait]
impl Provider for AnthropicProvider {
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
            .context("anthropic request failed")?
            .error_for_status()
            .context("anthropic api error")?
            .json::<AnthropicMessageResponse>()
            .await
            .context("failed to parse anthropic response")?;

        response
            .merged_text()
            .ok_or_else(|| anyhow!("anthropic response missing text"))
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
            .context("anthropic stream request failed")?
            .error_for_status()
            .context("anthropic stream api error")?;

        let body = response.bytes_stream();
        let stream = try_stream! {
            let mut buffer = String::new();
            let mut event_payload = String::new();
            pin_mut!(body);

            while let Some(chunk) = body.next().await {
                let chunk = chunk.context("anthropic stream chunk error")?;
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
                        event_payload.push_str(rest.trim_start());
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
struct AnthropicRequest {
    model: String,
    max_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<String>,
    messages: Vec<AnthropicMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(default)]
    stream: bool,
}

#[derive(Serialize)]
struct AnthropicMessage {
    role: String,
    content: Vec<AnthropicContent>,
}

impl AnthropicMessage {
    fn from_chat(message: &ChatMessage) -> Self {
        let role = match message.role {
            MessageRole::User => "user",
            MessageRole::Assistant => "assistant",
            MessageRole::System => "user",
        }
        .to_string();

        Self {
            role,
            content: vec![AnthropicContent::text(message.content.clone())],
        }
    }
}

#[derive(Serialize)]
struct AnthropicContent {
    #[serde(rename = "type")]
    kind: &'static str,
    text: String,
}

impl AnthropicContent {
    fn text(text: String) -> Self {
        Self { kind: "text", text }
    }
}

#[derive(Deserialize)]
struct AnthropicMessageResponse {
    content: Vec<AnthropicContentBlock>,
}

impl AnthropicMessageResponse {
    fn merged_text(&self) -> Option<String> {
        let mut out = String::new();
        for block in &self.content {
            if let Some(text) = &block.text {
                out.push_str(text);
            }
        }
        if out.trim().is_empty() {
            None
        } else {
            Some(out)
        }
    }
}

#[derive(Deserialize)]
struct AnthropicContentBlock {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    text: Option<String>,
}

#[derive(Deserialize)]
struct AnthropicStreamEvent {
    #[serde(rename = "type")]
    event_type: String,
    #[serde(default)]
    delta: Option<AnthropicStreamDelta>,
    #[serde(default)]
    content_block: Option<AnthropicContentBlock>,
}

impl AnthropicStreamEvent {
    fn text_fragment(&self) -> Option<&str> {
        match self.event_type.as_str() {
            "content_block_delta" => self
                .delta
                .as_ref()
                .and_then(|delta| delta.text.as_deref()),
            "content_block_start" => self
                .content_block
                .as_ref()
                .and_then(|block| block.text.as_deref()),
            _ => None,
        }
    }
}

#[derive(Deserialize)]
struct AnthropicStreamDelta {
    #[serde(rename = "type")]
    delta_type: String,
    #[serde(default)]
    text: Option<String>,
}
