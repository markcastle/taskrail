//! The model the rail may use, and the one job it is used for.

use serde::{Deserialize, Serialize};
use taskrail_core::ports::{KeyValueStore, SecretStore};
use taskrail_llm::{client_for, LlmClient, LlmConfig, LlmRequest, Provider};
use tauri::{AppHandle, Manager};

use crate::secrets::{llm_key_name, OsSecrets};
use crate::state::{lock, AppState};

const LLM_CONFIG_KEY: &str = "llm_config";

/// What the settings page shows. The key itself never leaves the keychain.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelSettings {
    pub provider: Provider,
    pub model: Option<String>,
    pub base_url: Option<String>,
    pub has_key: bool,
}

pub fn load_config(app: &AppHandle) -> LlmConfig {
    let state = app.state::<AppState>();
    let rail = lock(&state.rail);
    rail.service
        .repository()
        .get_value(LLM_CONFIG_KEY)
        .ok()
        .flatten()
        .and_then(|json| serde_json::from_str(&json).ok())
        .unwrap_or(LlmConfig {
            provider: Provider::Anthropic,
            model: None,
            base_url: None,
        })
}

pub fn settings(app: &AppHandle) -> ModelSettings {
    let config = load_config(app);
    let has_key = OsSecrets
        .get(&llm_key_name(config.provider))
        .ok()
        .flatten()
        .is_some_and(|k| !k.is_empty());
    ModelSettings {
        provider: config.provider,
        model: config.model,
        base_url: config.base_url,
        has_key,
    }
}

pub fn save(
    app: &AppHandle,
    config: LlmConfig,
    api_key: Option<String>,
) -> Result<ModelSettings, String> {
    {
        let state = app.state::<AppState>();
        let mut rail = lock(&state.rail);
        let json = serde_json::to_string(&config).map_err(|e| e.to_string())?;
        rail.service
            .repository_mut()
            .set_value(LLM_CONFIG_KEY, &json)
            .map_err(|e| e.to_string())?;
    }
    if let Some(key) = api_key {
        let name = llm_key_name(config.provider);
        if key.trim().is_empty() {
            OsSecrets.delete(&name).map_err(|e| e.to_string())?;
        } else {
            OsSecrets
                .set(&name, key.trim())
                .map_err(|e| e.to_string())?;
        }
    }
    Ok(settings(app))
}

/// A ready client, or a message saying what is missing.
pub fn client(app: &AppHandle) -> Result<Box<dyn LlmClient>, String> {
    let config = load_config(app);
    let key = OsSecrets
        .get(&llm_key_name(config.provider))
        .map_err(|e| e.to_string())?
        .ok_or_else(|| {
            format!(
                "no API key saved for {:?}; add one under Model in settings",
                config.provider
            )
        })?;
    client_for(&config, &key).map_err(|e| e.to_string())
}

/// One cheap round trip to prove the key and model work.
pub async fn test(app: &AppHandle) -> Result<String, String> {
    let client = client(app)?;
    log::info!("testing {}", client.describe());
    let reply = client
        .complete(&LlmRequest::single(
            "Answer in five words or fewer.",
            "Say hello to Task Rail.",
            64,
        ))
        .await
        .map_err(|e| e.to_string())?;
    Ok(format!("{} replied: {}", client.describe(), reply.trim()))
}
