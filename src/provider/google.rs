use std::sync::Arc;

use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use hyper::client::HttpConnector;
use hyper_rustls::HttpsConnector;
use parking_lot::Mutex;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tokio::time::{sleep, Duration};
use yup_oauth2::{authenticator::Authenticator, read_service_account_key, AccessToken, ServiceAccountAuthenticator};

use crate::config::GoogleProviderConfig;
use crate::provider::{ChatMessage, ChatRequestOptions, MessageRole, Provider};

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
    pub async fn new(name: String, config: GoogleProviderConfig) -> Result<Self> {
        let client = Client::builder().build()?;
        let authenticator = match config.service_account_file.as_ref() {
            Some(path) => {
                let key = read_service_account_key(path)
                    .await
                    .with_context(|| format!("failed to read service account JSON at {}", path.display()))?;
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
            None => return Err(anyhow!("service account not configured for google provider")),
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
        let mut last_err: Option<anyhow::Error> = None;

        for attempt in 0..3 {
            let mut request = self.client.post(&url).json(payload);
            if let Some(key) = &self.config.api_key {
                request = request.query(&[("key", key)]);
            } else if let Some(token) = self.ensure_token().await? {
                request = request.bearer_auth(token);
            }

            match request.send().await {
                Ok(response) => {
                    if response.status().as_u16() == 429 && attempt < 2 {
                        sleep(Duration::from_millis(500 * (attempt as u64 + 1))).await;
                        continue;
                    }
                    let response = response.error_for_status().context("google api error")?;
                    let payload: GeminiResponse = response
                        .json()
                        .await
                        .context("failed to deserialize gemini response")?;
                    return Ok(payload);
                }
                Err(err) => {
                    last_err = Some(err.into());
                    sleep(Duration::from_millis(250 * (attempt as u64 + 1))).await;
                }
            }
        }

        Err(last_err.unwrap_or_else(|| anyhow!("failed to call google api")))
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
        let system_instruction = system.map(|text| GeminiContent {
            role: "system".to_string(),
            parts: vec![GeminiPart {
                text: text.to_string(),
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
                    text: msg.content.clone(),
                }],
            })
            .collect();

        let payload = GeminiRequest {
            contents,
            system_instruction,
            generation_config: Some(GeminiGenerationConfig {
                temperature: options.temperature,
                max_output_tokens: options.max_output_tokens,
            }),
        };

        let response = self.execute_request(model, &payload).await?;
        response
            .candidates
            .first()
            .and_then(|candidate| candidate.content.parts.first())
            .map(|part| part.text.clone())
            .ok_or_else(|| anyhow!("gemini response missing content"))
    }

    async fn stream_chat(
        &self,
        _model: &str,
        _system: Option<&str>,
        _messages: &[ChatMessage],
        _options: &ChatRequestOptions,
    ) -> Result<crate::streaming::ChatStream> {
        anyhow::bail!("streaming is MVP+ and not available yet");
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
    text: String,
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
