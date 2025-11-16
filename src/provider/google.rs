use std::sync::Arc;

use anyhow::{anyhow, Context, Result};
use async_stream::try_stream;
use async_trait::async_trait;
use futures::{pin_mut, StreamExt};
use hyper::client::HttpConnector;
use hyper_rustls::HttpsConnector;
use parking_lot::Mutex;
use reqwest::{Client, Response, StatusCode};
use serde::{Deserialize, Serialize};
use tokio::time::{sleep, Duration};
use yup_oauth2::{
    authenticator::Authenticator, read_service_account_key, AccessToken,
    ServiceAccountAuthenticator,
};

use crate::config::GoogleProviderConfig;
use crate::provider::{ChatMessage, ChatRequestOptions, MessageRole, Provider};
use crate::secrets;
use crate::streaming::ChatStream;

const BASE_URL: &str = "https://generativelanguage.googleapis.com/v1";
const GENERATIVE_SCOPE: &str = "https://www.googleapis.com/auth/generative-language";

type GoogleAuthenticator = Authenticator<HttpsConnector<HttpConnector>>;

pub struct GoogleProvider {
    name: String,
    config: GoogleProviderConfig,
    client: Client,
    authenticator: Option<Arc<GoogleAuthenticator>>,
    cached_token: Mutex<Option<AccessToken>>,
}

impl GoogleProvider {
    pub async fn new(
        name: String,
        mut config: GoogleProviderConfig,
        passphrase: Option<&str>,
        env_label: &str,
    ) -> Result<Self> {
        let resolved_api_key = secrets::resolve_secret(
            config.api_key.as_deref(),
            config.encrypted_api_key.as_ref(),
            passphrase,
            env_label,
        )?;
        config.api_key = resolved_api_key;
        config.encrypted_api_key = None;

        let client = Client::builder().build()?;
        let authenticator = match config.service_account_file.as_ref() {
            Some(path) => {
                let key = read_service_account_key(path).await.with_context(|| {
                    format!("failed to read service account JSON at {}", path.display())
                })?;
                let auth = ServiceAccountAuthenticator::builder(key)
                    .build()
                    .await
                    .context("failed to build google authenticator")?;
                Some(Arc::new(auth))
            }
            None => None,
        };

        if authenticator.is_none() && config.api_key.is_none() {
            return Err(anyhow!(
                "google provider '{}' requires --service-account or --api-key",
                name
            ));
        }

        Ok(Self {
            name,
            config,
            client,
            authenticator,
            cached_token: Mutex::new(None),
        })
    }

    async fn ensure_token(&self) -> Result<Option<String>> {
        if self.config.api_key.is_some() {
            return Ok(None);
        }
        let auth = match &self.authenticator {
            Some(a) => a,
            None => {
                return Err(anyhow!(
                    "service account not configured for google provider"
                ))
            }
        };

        {
            let cached = self.cached_token.lock();
            if let Some(token) = cached.as_ref() {
                if !token.is_expired() {
                    if let Some(value) = token.token() {
                        return Ok(Some(value.to_string()));
                    }
                }
            }
        }

        let token = auth
            .token(&[GENERATIVE_SCOPE])
            .await
            .context("failed to obtain oauth token")?;
        let bearer = token
            .token()
            .map(|value| value.to_string())
            .ok_or_else(|| anyhow!("oauth token response missing access_token"))?;
        {
            let mut cached = self.cached_token.lock();
            *cached = Some(token);
        }
        Ok(Some(bearer))
    }

    async fn execute_request(
        &self,
        model: &str,
        payload: &GeminiRequest,
    ) -> Result<GeminiResponse> {
        let url = format!("{BASE_URL}/models/{model}:generateContent");
        self.with_retries(&url, payload, |response| async move {
            let response = response.error_for_status().context("google api error")?;
            let payload: GeminiResponse = response
                .json()
                .await
                .context("failed to deserialize gemini response")?;
            Ok(payload)
        })
        .await
    }

    async fn execute_stream_request(
        &self,
        model: &str,
        payload: &GeminiRequest,
    ) -> Result<ChatStream> {
        let url = format!("{BASE_URL}/models/{model}:streamGenerateContent");
        self.with_retries(&url, payload, |response| async move {
            if !response.status().is_success() {
                let status = response.status();
                let text = response.text().await.unwrap_or_default();
                return Err(anyhow!("google stream api error {status}: {text}"));
            }
            let body = response.bytes_stream();
            let stream = try_stream! {
                let mut buffer = String::new();
                let mut last_snapshot = String::new();
                pin_mut!(body);

                while let Some(chunk) = body.next().await {
                    let chunk = chunk.context("stream chunk error")?;
                    let text = String::from_utf8_lossy(&chunk);
                    buffer.push_str(&text);

                    while let Some(texts) = Self::try_extract_json(&mut buffer)? {
                        for t in texts {
                            let delta = Self::extract_delta(&mut last_snapshot, &t);
                            if !delta.is_empty() {
                                yield delta;
                            }
                        }
                    }
                }

                if !buffer.trim().is_empty() {
                    if let Some(texts) = Self::try_extract_json(&mut buffer)? {
                        for t in texts {
                            let delta = Self::extract_delta(&mut last_snapshot, &t);
                            if !delta.is_empty() {
                                yield delta;
                            }
                        }
                    }
                }
            };

            Ok(Box::pin(stream) as ChatStream)
        })
        .await
    }

    fn try_extract_json(buffer: &mut String) -> Result<Option<Vec<String>>> {
        let trimmed = buffer.trim_start();
        if trimmed.is_empty() {
            buffer.clear();
            return Ok(None);
        }

        if let Some(stripped) = trimmed.strip_prefix("data:") {
            *buffer = stripped.trim_start().to_string();
            return Self::try_extract_json(buffer);
        }

        let first_char = trimmed.chars().next().unwrap();
        if first_char != '{' && first_char != '[' {
            if let Some(pos) = trimmed.find(|c| c == '{' || c == '[') {
                *buffer = trimmed[pos..].to_string();
                return Self::try_extract_json(buffer);
            } else {
                buffer.clear();
                return Ok(None);
            }
        }

        let end_char = if first_char == '{' { '}' } else { ']' };
        let mut depth = 0;
        let mut in_string = false;
        let mut escape = false;
        let mut end_pos = None;

        for (i, ch) in trimmed.char_indices() {
            if escape {
                escape = false;
                continue;
            }

            match ch {
                '\\' if in_string => escape = true,
                '"' => in_string = !in_string,
                c if c == first_char && !in_string => depth += 1,
                c if c == end_char && !in_string => {
                    depth -= 1;
                    if depth == 0 {
                        end_pos = Some(i + ch.len_utf8());
                        break;
                    }
                }
                _ => {}
            }
        }

        if let Some(pos) = end_pos {
            let json_str = &trimmed[..pos];
            let result = Self::parse_stream_payload(json_str)?;
            *buffer = trimmed[pos..].to_string();
            Ok(Some(result))
        } else {
            Ok(None)
        }
    }

    async fn with_retries<F, Fut, T>(
        &self,
        url: &str,
        payload: &GeminiRequest,
        handler: F,
    ) -> Result<T>
    where
        F: Fn(Response) -> Fut,
        Fut: std::future::Future<Output = Result<T>>,
    {
        let mut last_err: Option<anyhow::Error> = None;

        for attempt in 0..3 {
            let mut request = self.client.post(url).json(payload);
            request = self.apply_auth(request).await?;

            match request.send().await {
                Ok(response) => {
                    if response.status() == StatusCode::TOO_MANY_REQUESTS && attempt < 2 {
                        sleep(Duration::from_millis(500 * (attempt as u64 + 1))).await;
                        continue;
                    }
                    match handler(response).await {
                        Ok(value) => return Ok(value),
                        Err(err) => {
                            last_err = Some(err);
                            break;
                        }
                    }
                }
                Err(err) => {
                    last_err = Some(err.into());
                    sleep(Duration::from_millis(250 * (attempt as u64 + 1))).await;
                }
            }
        }

        Err(last_err.unwrap_or_else(|| anyhow!("failed to call google api")))
    }

    async fn apply_auth(
        &self,
        request: reqwest::RequestBuilder,
    ) -> Result<reqwest::RequestBuilder> {
        if let Some(key) = &self.config.api_key {
            Ok(request.query(&[("key", key)]))
        } else if let Some(token) = self.ensure_token().await? {
            Ok(request.bearer_auth(token))
        } else {
            Err(anyhow!("google provider '{}' lacks credentials", self.name))
        }
    }

    fn build_payload(
        &self,
        system: Option<&str>,
        messages: &[ChatMessage],
        options: &ChatRequestOptions,
    ) -> GeminiRequest {
        let system_instruction = system.map(|text| GeminiContent {
            role: "system".to_string(),
            parts: vec![GeminiPart {
                text: Some(text.to_string()),
            }],
        });

        let contents: Vec<GeminiContent> = messages
            .iter()
            .filter(|msg| msg.role != MessageRole::System)
            .map(|msg| GeminiContent {
                role: match msg.role {
                    MessageRole::User => "user",
                    MessageRole::Assistant => "model",
                    MessageRole::System => "user",
                }
                .to_string(),
                parts: vec![GeminiPart {
                    text: Some(msg.content.clone()),
                }],
            })
            .collect();

        GeminiRequest {
            contents,
            system_instruction,
            generation_config: Some(GeminiGenerationConfig {
                temperature: options.temperature,
                max_output_tokens: options.max_output_tokens,
            }),
        }
    }

    fn parse_stream_payload(payload: &str) -> Result<Vec<String>> {
        let body = payload.trim();
        if body.is_empty() || body == "[DONE]" {
            return Ok(Vec::new());
        }

        if body.starts_with('[') {
            let chunks: Vec<GeminiStreamChunk> = serde_json::from_str(body)
                .with_context(|| format!("failed to parse stream chunk array: {body}"))?;
            let mut texts = Vec::new();
            for chunk in chunks {
                if let Some(text) = chunk.merge_text() {
                    texts.push(text);
                }
            }
            Ok(texts)
        } else {
            let chunk: GeminiStreamChunk = serde_json::from_str(body)
                .with_context(|| format!("failed to parse stream chunk: {body}"))?;
            Ok(chunk.merge_text().into_iter().collect())
        }
    }

    fn extract_delta(snapshot: &mut String, incoming: &str) -> String {
        if incoming.is_empty() {
            return String::new();
        }

        if incoming.starts_with(snapshot.as_str()) {
            let delta = &incoming[snapshot.len()..];
            *snapshot = incoming.to_string();
            return delta.to_string();
        }

        if snapshot.starts_with(incoming) {
            return String::new();
        }

        *snapshot = incoming.to_string();
        incoming.to_string()
    }
}

#[async_trait]
impl Provider for GoogleProvider {
    async fn chat(
        &self,
        model: &str,
        system: Option<&str>,
        messages: &[ChatMessage],
        options: &ChatRequestOptions,
    ) -> Result<String> {
        let payload = self.build_payload(system, messages, options);

        let response = self.execute_request(model, &payload).await?;
        response
            .candidates
            .first()
            .and_then(|candidate| candidate.content.text())
            .ok_or_else(|| anyhow!("gemini response missing content"))
    }

    async fn stream_chat(
        &self,
        model: &str,
        system: Option<&str>,
        messages: &[ChatMessage],
        options: &ChatRequestOptions,
    ) -> Result<ChatStream> {
        let payload = self.build_payload(system, messages, options);
        self.execute_stream_request(model, &payload).await
    }
}

#[derive(Debug, Serialize)]
struct GeminiRequest {
    contents: Vec<GeminiContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    system_instruction: Option<GeminiContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    generation_config: Option<GeminiGenerationConfig>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct GeminiContent {
    role: String,
    parts: Vec<GeminiPart>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct GeminiPart {
    #[serde(skip_serializing_if = "Option::is_none")]
    text: Option<String>,
}

#[derive(Debug, Serialize)]
struct GeminiGenerationConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_output_tokens: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct GeminiResponse {
    candidates: Vec<GeminiCandidate>,
}

#[derive(Debug, Deserialize)]
struct GeminiCandidate {
    content: GeminiContent,
}

#[derive(Debug, Deserialize)]
struct GeminiStreamChunk {
    #[serde(default)]
    candidates: Vec<GeminiCandidate>,
}

impl GeminiContent {
    fn text(&self) -> Option<String> {
        let mut buf = String::new();
        for part in &self.parts {
            if let Some(piece) = part.text.as_ref() {
                buf.push_str(piece);
            }
        }
        if buf.is_empty() {
            None
        } else {
            Some(buf)
        }
    }
}

impl GeminiStreamChunk {
    fn merge_text(&self) -> Option<String> {
        self.candidates
            .first()
            .and_then(|candidate| candidate.content.text())
    }
}
