//! The one place Task Rail talks to a language model.
//!
//! The rail uses a model for exactly one job: reading an MCP server's tool
//! list and writing a Connector Profile. That is a single request with a
//! single JSON answer, so the interface is deliberately tiny: a system
//! prompt, a user prompt, and text back. No streaming, no tools, no
//! conversation. Bring your own key.
//!
//! Two wire formats cover the providers people asked for. Anthropic's
//! Messages API is spoken natively. OpenAI's chat completions format is
//! spoken for OpenAI itself and for OpenRouter, which uses the same shape.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const ANTHROPIC_API_URL: &str = "https://api.anthropic.com/v1";
pub const ANTHROPIC_DEFAULT_MODEL: &str = "claude-opus-5-5";
pub const OPENAI_API_URL: &str = "https://api.openai.com/v1";
pub const OPENROUTER_API_URL: &str = "https://openrouter.ai/api/v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Provider {
    Anthropic,
    OpenAi,
    OpenRouter,
}

impl Provider {
    pub fn default_base_url(self) -> &'static str {
        match self {
            Provider::Anthropic => ANTHROPIC_API_URL,
            Provider::OpenAi => OPENAI_API_URL,
            Provider::OpenRouter => OPENROUTER_API_URL,
        }
    }

    /// Only Anthropic has a default model here. OpenAI and OpenRouter model
    /// names change too often to guess; the user names one.
    pub fn default_model(self) -> Option<&'static str> {
        match self {
            Provider::Anthropic => Some(ANTHROPIC_DEFAULT_MODEL),
            _ => None,
        }
    }
}

/// Everything needed to build a client. The key is never stored in here
/// for long; the shell keeps it in the OS keychain and passes it in.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LlmConfig {
    pub provider: Provider,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub base_url: Option<String>,
}

impl LlmConfig {
    pub fn model(&self) -> Result<String, LlmError> {
        self.model
            .clone()
            .or_else(|| self.provider.default_model().map(str::to_owned))
            .ok_or_else(|| LlmError::Config(format!("{:?} needs a model name", self.provider)))
    }

    pub fn base_url(&self) -> String {
        self.base_url
            .clone()
            .unwrap_or_else(|| self.provider.default_base_url().to_owned())
            .trim_end_matches('/')
            .to_owned()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    User,
    Assistant,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LlmMessage {
    pub role: Role,
    pub content: String,
}

/// A system prompt and the turns so far, oldest first, ending with the
/// user. Discovery keeps the whole exchange so the model never loses what
/// it was told at the start.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LlmRequest {
    pub system: String,
    pub messages: Vec<LlmMessage>,
    pub max_tokens: u32,
}

impl LlmRequest {
    /// One question, no history.
    pub fn single(system: impl Into<String>, user: impl Into<String>, max_tokens: u32) -> Self {
        Self {
            system: system.into(),
            messages: vec![LlmMessage {
                role: Role::User,
                content: user.into(),
            }],
            max_tokens,
        }
    }

    /// The most recent user turn, for logs and tests.
    pub fn last_user(&self) -> &str {
        self.messages
            .iter()
            .rev()
            .find(|m| m.role == Role::User)
            .map(|m| m.content.as_str())
            .unwrap_or_default()
    }
}

fn wire_messages(request: &LlmRequest) -> Vec<Value> {
    request
        .messages
        .iter()
        .map(|m| {
            serde_json::json!({
                "role": match m.role { Role::User => "user", Role::Assistant => "assistant" },
                "content": m.content,
            })
        })
        .collect()
}

#[derive(Debug, thiserror::Error)]
pub enum LlmError {
    #[error("model configuration problem: {0}")]
    Config(String),
    #[error("the key was rejected")]
    Auth,
    #[error("rate limited; try again shortly")]
    RateLimited,
    #[error("the provider rejected the request: {0}")]
    BadRequest(String),
    #[error("the provider failed ({status}): {body}")]
    Server { status: u16, body: String },
    #[error("could not reach the provider: {0}")]
    Network(String),
    #[error("the model declined to answer: {0}")]
    Refused(String),
    #[error("the answer was not what was expected: {0}")]
    Malformed(String),
}

#[async_trait]
pub trait LlmClient: Send + Sync {
    /// One prompt in, text out.
    async fn complete(&self, request: &LlmRequest) -> Result<String, LlmError>;

    /// For logs and the settings page: provider and model.
    fn describe(&self) -> String;
}

/// Build the right client for a config and key.
pub fn client_for(config: &LlmConfig, api_key: &str) -> Result<Box<dyn LlmClient>, LlmError> {
    if api_key.trim().is_empty() {
        return Err(LlmError::Config("no API key".into()));
    }
    let model = config.model()?;
    let http = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(180))
        .build()
        .map_err(|e| LlmError::Network(e.to_string()))?;
    Ok(match config.provider {
        Provider::Anthropic => Box::new(AnthropicClient {
            http,
            base_url: config.base_url(),
            api_key: api_key.to_owned(),
            model,
        }),
        Provider::OpenAi | Provider::OpenRouter => Box::new(OpenAiCompatibleClient {
            http,
            base_url: config.base_url(),
            api_key: api_key.to_owned(),
            model,
            provider: config.provider,
        }),
    })
}

/// Pull the JSON object out of a model's answer, tolerating code fences and
/// a sentence or two around it.
pub fn extract_json(text: &str) -> Result<Value, LlmError> {
    let trimmed = text.trim();
    if let Ok(value) = serde_json::from_str::<Value>(trimmed) {
        return Ok(value);
    }
    let unfenced = trimmed
        .strip_prefix("```json")
        .or_else(|| trimmed.strip_prefix("```"))
        .and_then(|s| s.strip_suffix("```"))
        .map(str::trim);
    if let Some(inner) = unfenced {
        if let Ok(value) = serde_json::from_str::<Value>(inner) {
            return Ok(value);
        }
    }
    let start = trimmed.find('{');
    let end = trimmed.rfind('}');
    if let (Some(start), Some(end)) = (start, end) {
        if end > start {
            if let Ok(value) = serde_json::from_str::<Value>(&trimmed[start..=end]) {
                return Ok(value);
            }
        }
    }
    Err(LlmError::Malformed(format!(
        "no JSON object found in: {}",
        trimmed.chars().take(200).collect::<String>()
    )))
}

fn classify(status: reqwest::StatusCode, body: String) -> LlmError {
    match status.as_u16() {
        401 | 403 => LlmError::Auth,
        429 => LlmError::RateLimited,
        400..=499 => LlmError::BadRequest(body),
        code => LlmError::Server { status: code, body },
    }
}

async fn post(http: &reqwest::Client, request: reqwest::RequestBuilder) -> Result<Value, LlmError> {
    let _ = http;
    let response = request
        .send()
        .await
        .map_err(|e| LlmError::Network(e.to_string()))?;
    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|e| LlmError::Network(e.to_string()))?;
    if !status.is_success() {
        return Err(classify(status, body));
    }
    serde_json::from_str(&body).map_err(|e| LlmError::Malformed(format!("{e}: {body}")))
}

// ---------------------------------------------------------------------------

struct AnthropicClient {
    http: reqwest::Client,
    base_url: String,
    api_key: String,
    model: String,
}

#[async_trait]
impl LlmClient for AnthropicClient {
    async fn complete(&self, request: &LlmRequest) -> Result<String, LlmError> {
        let body = serde_json::json!({
            "model": self.model,
            "max_tokens": request.max_tokens,
            "system": request.system,
            "messages": wire_messages(request),
        });
        let value = post(
            &self.http,
            self.http
                .post(format!("{}/messages", self.base_url))
                .header("x-api-key", &self.api_key)
                .header("anthropic-version", "2026-10-21")
                .header("content-type", "application/json")
                .json(&body),
        )
        .await?;

        if value["stop_reason"] == "refusal" {
            let why = value["stop_details"]["explanation"]
                .as_str()
                .unwrap_or("no reason given");
            return Err(LlmError::Refused(why.to_owned()));
        }
        let text: String = value["content"]
            .as_array()
            .map(|blocks| {
                blocks
                    .iter()
                    .filter(|b| b["type"] == "text")
                    .filter_map(|b| b["text"].as_str())
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .unwrap_or_default();
        if text.is_empty() {
            return Err(LlmError::Malformed(format!("no text in response: {value}")));
        }
        Ok(text)
    }

    fn describe(&self) -> String {
        format!("Anthropic · {}", self.model)
    }
}

struct OpenAiCompatibleClient {
    http: reqwest::Client,
    base_url: String,
    api_key: String,
    model: String,
    provider: Provider,
}

#[async_trait]
impl LlmClient for OpenAiCompatibleClient {
    async fn complete(&self, request: &LlmRequest) -> Result<String, LlmError> {
        let mut messages = vec![serde_json::json!({ "role": "system", "content": request.system })];
        messages.extend(wire_messages(request));
        let body = serde_json::json!({
            "model": self.model,
            "max_tokens": request.max_tokens,
            "messages": messages,
        });
        let mut builder = self
            .http
            .post(format!("{}/chat/completions", self.base_url))
            .bearer_auth(&self.api_key)
            .header("content-type", "application/json");
        if self.provider == Provider::OpenRouter {
            builder = builder
                .header("HTTP-Referer", "https://github.com/markcastle/taskrail")
                .header("X-Title", "Task Rail");
        }
        let value = post(&self.http, builder.json(&body)).await?;
        let text = value["choices"][0]["message"]["content"]
            .as_str()
            .map(str::to_owned)
            .ok_or_else(|| LlmError::Malformed(format!("no choices in response: {value}")))?;
        Ok(text)
    }

    fn describe(&self) -> String {
        let name = match self.provider {
            Provider::OpenRouter => "OpenRouter",
            _ => "OpenAI",
        };
        format!("{name} · {}", self.model)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{body_partial_json, header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn request() -> LlmRequest {
        LlmRequest::single("You write JSON.", "Describe a cat.", 500)
    }

    #[tokio::test]
    async fn anthropic_sends_the_messages_api_shape_and_reads_text_blocks() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/messages"))
            .and(header("x-api-key", "sk-test"))
            .and(header("anthropic-version", "2026-10-21"))
            .and(body_partial_json(serde_json::json!({
                "model": "claude-opus-5-5",
                "system": "You write JSON.",
                "messages": [{ "role": "user", "content": "Describe a cat." }]
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "content": [
                    { "type": "thinking", "thinking": "" },
                    { "type": "text", "text": "{\"animal\": \"cat\"}" }
                ],
                "stop_reason": "end_turn"
            })))
            .mount(&server)
            .await;

        let config = LlmConfig {
            provider: Provider::Anthropic,
            model: None,
            base_url: Some(server.uri()),
        };
        let client = client_for(&config, "sk-test").unwrap();
        let text = client.complete(&request()).await.unwrap();
        assert_eq!(extract_json(&text).unwrap()["animal"], "cat");
        assert_eq!(client.describe(), "Anthropic · claude-opus-5-5");
    }

    #[tokio::test]
    async fn anthropic_refusals_and_auth_failures_are_typed() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/messages"))
            .and(header("x-api-key", "bad"))
            .respond_with(ResponseTemplate::new(401).set_body_string("{\"error\":\"nope\"}"))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/messages"))
            .and(header("x-api-key", "good"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "content": [],
                "stop_reason": "refusal",
                "stop_details": { "type": "refusal", "category": "cyber", "explanation": "not that" }
            })))
            .mount(&server)
            .await;
        let config = LlmConfig {
            provider: Provider::Anthropic,
            model: None,
            base_url: Some(server.uri()),
        };
        let bad = client_for(&config, "bad").unwrap();
        assert!(matches!(
            bad.complete(&request()).await,
            Err(LlmError::Auth)
        ));
        let good = client_for(&config, "good").unwrap();
        assert!(
            matches!(good.complete(&request()).await, Err(LlmError::Refused(m)) if m == "not that")
        );
    }

    #[tokio::test]
    async fn openai_compatible_sends_chat_completions_and_openrouter_headers() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .and(header("authorization", "Bearer or-key"))
            .and(header("X-Title", "Task Rail"))
            .and(body_partial_json(serde_json::json!({
                "model": "anthropic/claude-opus-5.5",
                "messages": [
                    { "role": "system", "content": "You write JSON." },
                    { "role": "user", "content": "Describe a cat." }
                ]
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "choices": [{ "message": { "role": "assistant", "content": "```json\n{\"animal\": \"cat\"}\n```" } }]
            })))
            .mount(&server)
            .await;
        let config = LlmConfig {
            provider: Provider::OpenRouter,
            model: Some("anthropic/claude-opus-5.5".into()),
            base_url: Some(server.uri()),
        };
        let client = client_for(&config, "or-key").unwrap();
        let text = client.complete(&request()).await.unwrap();
        assert_eq!(extract_json(&text).unwrap()["animal"], "cat");
    }

    #[tokio::test]
    async fn a_conversation_is_sent_in_order_on_both_wires() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/messages"))
            .and(body_partial_json(serde_json::json!({
                "messages": [
                    { "role": "user", "content": "first" },
                    { "role": "assistant", "content": "reply" },
                    { "role": "user", "content": "second" }
                ]
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "content": [{ "type": "text", "text": "ok" }], "stop_reason": "end_turn"
            })))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .and(body_partial_json(serde_json::json!({
                "messages": [
                    { "role": "system", "content": "sys" },
                    { "role": "user", "content": "first" },
                    { "role": "assistant", "content": "reply" },
                    { "role": "user", "content": "second" }
                ]
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "choices": [{ "message": { "role": "assistant", "content": "ok" } }]
            })))
            .mount(&server)
            .await;
        let request = LlmRequest {
            system: "sys".into(),
            messages: vec![
                LlmMessage {
                    role: Role::User,
                    content: "first".into(),
                },
                LlmMessage {
                    role: Role::Assistant,
                    content: "reply".into(),
                },
                LlmMessage {
                    role: Role::User,
                    content: "second".into(),
                },
            ],
            max_tokens: 100,
        };
        assert_eq!(request.last_user(), "second");
        let anthropic = client_for(
            &LlmConfig {
                provider: Provider::Anthropic,
                model: None,
                base_url: Some(server.uri()),
            },
            "k",
        )
        .unwrap();
        assert_eq!(anthropic.complete(&request).await.unwrap(), "ok");
        let openai = client_for(
            &LlmConfig {
                provider: Provider::OpenAi,
                model: Some("gpt-5".into()),
                base_url: Some(server.uri()),
            },
            "k",
        )
        .unwrap();
        assert_eq!(openai.complete(&request).await.unwrap(), "ok");
    }

    #[test]
    fn a_model_is_required_except_for_anthropic() {
        let openai = LlmConfig {
            provider: Provider::OpenAi,
            model: None,
            base_url: None,
        };
        assert!(matches!(client_for(&openai, "k"), Err(LlmError::Config(_))));
        let anthropic = LlmConfig {
            provider: Provider::Anthropic,
            model: None,
            base_url: None,
        };
        assert!(client_for(&anthropic, "k").is_ok());
        assert!(matches!(
            client_for(&anthropic, "  "),
            Err(LlmError::Config(_))
        ));
    }

    #[test]
    fn json_is_found_in_fences_and_prose() {
        assert_eq!(extract_json("{\"a\":1}").unwrap()["a"], 1);
        assert_eq!(extract_json("```json\n{\"a\":2}\n```").unwrap()["a"], 2);
        assert_eq!(
            extract_json("Here you go:\n{\"a\":3}\nAnything else?").unwrap()["a"],
            3
        );
        assert!(extract_json("no json here").is_err());
    }
}
