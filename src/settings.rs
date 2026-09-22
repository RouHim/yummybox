//! Central settings for the AI and Bring! integrations.
//!
//! Values live in the `settings` key/value table (migration 006). Every
//! effective value resolves as: stored value, then environment variable, then
//! unset. Secrets are never returned by the API — only their set state and
//! their origin.

use genai::adapter::AdapterKind;
use serde::Serialize;
use sqlx::SqlitePool;

use crate::db;
use crate::error::AppError;

// ---------------------------------------------------------------------------
// Stored keys and limits
// ---------------------------------------------------------------------------

pub const KEY_LLM_PROVIDER: &str = "llm.provider";
pub const KEY_LLM_MODEL: &str = "llm.model";
pub const KEY_LLM_BASE_URL: &str = "llm.base_url";
pub const KEY_LLM_API_KEY: &str = "llm.api_key";
pub const KEY_BRING_EMAIL: &str = "bring.email";
pub const KEY_BRING_PASSWORD: &str = "bring.password";

/// Longest accepted provider id.
pub const MAX_PROVIDER_LEN: usize = 40;
/// Longest accepted model name.
pub const MAX_MODEL_LEN: usize = 200;
/// Longest accepted custom base URL.
pub const MAX_BASE_URL_LEN: usize = 2048;
/// Longest accepted API key.
pub const MAX_API_KEY_LEN: usize = 4096;
/// Longest accepted Bring! email.
pub const MAX_BRING_EMAIL_LEN: usize = 320;
/// Longest accepted Bring! password.
pub const MAX_BRING_PASSWORD_LEN: usize = 256;

const BRING_EMAIL_ENV: &str = "BRING_EMAIL";
const BRING_PASSWORD_ENV: &str = "BRING_PASSWORD";

// ---------------------------------------------------------------------------
// Environment lookup
// ---------------------------------------------------------------------------

/// Environment lookup, injected so resolution is testable without mutating
/// the process environment.
pub type Env<'a> = &'a dyn Fn(&str) -> Option<String>;

/// The environment lookup the handlers use.
pub fn env_lookup(key: &str) -> Option<String> {
    std::env::var(key).ok()
}

// ---------------------------------------------------------------------------
// Stored values
// ---------------------------------------------------------------------------

/// Values read from the `settings` table, sanitized: blank, unknown-provider
/// and over-long values are treated as absent so a malformed or downgraded
/// record can never break the settings page or an AI flow.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StoredSettings {
    pub provider: Option<String>,
    pub model: Option<String>,
    pub base_url: Option<String>,
    pub api_key: Option<String>,
    pub bring_email: Option<String>,
    pub bring_password: Option<String>,
}

impl StoredSettings {
    /// Build from raw table rows, dropping every value that fails validation.
    pub fn from_pairs(pairs: Vec<(String, String)>) -> Self {
        let mut map: std::collections::HashMap<String, String> = pairs.into_iter().collect();
        let provider = take_text(&mut map, KEY_LLM_PROVIDER, MAX_PROVIDER_LEN)
            .filter(|id| crate::llm_import::provider_ids().contains(&id.as_str()));
        Self {
            provider,
            model: take_text(&mut map, KEY_LLM_MODEL, MAX_MODEL_LEN),
            base_url: take_text(&mut map, KEY_LLM_BASE_URL, MAX_BASE_URL_LEN),
            api_key: take_text(&mut map, KEY_LLM_API_KEY, MAX_API_KEY_LEN),
            bring_email: take_text(&mut map, KEY_BRING_EMAIL, MAX_BRING_EMAIL_LEN),
            bring_password: take_secret(&mut map, KEY_BRING_PASSWORD, MAX_BRING_PASSWORD_LEN),
        }
    }
}

/// Remove a text value, treating blank and over-long values as absent.
fn take_text(
    map: &mut std::collections::HashMap<String, String>,
    key: &str,
    max: usize,
) -> Option<String> {
    let raw = map.remove(key)?;
    let value = raw.trim();
    if value.is_empty() || value.chars().count() > max {
        return None;
    }
    Some(value.to_string())
}

/// Remove a secret value. Secrets are stored verbatim (surrounding spaces may
/// be part of the value); only an empty value counts as absent.
fn take_secret(
    map: &mut std::collections::HashMap<String, String>,
    key: &str,
    max: usize,
) -> Option<String> {
    let raw = map.remove(key)?;
    if raw.is_empty() || raw.chars().count() > max {
        return None;
    }
    Some(raw)
}

/// Read the stored settings from the database.
pub async fn load(pool: &SqlitePool) -> Result<StoredSettings, AppError> {
    Ok(StoredSettings::from_pairs(db::list_settings(pool).await?))
}

// ---------------------------------------------------------------------------
// Effective configuration
// ---------------------------------------------------------------------------

/// Where an effective value comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ValueSource {
    Settings,
    Environment,
    None,
}

/// A secret as exposed to the browser: never the value itself.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SecretState {
    pub set: bool,
    pub source: ValueSource,
}

/// The environment variable that supplies a provider's API key, if any.
fn provider_env_key(provider: &str, env: Env<'_>) -> Option<String> {
    let kind = AdapterKind::from_lower_str(provider)?;
    let name = kind.default_key_env_name()?;
    env(name).filter(|value| !value.is_empty())
}

// ---------------------------------------------------------------------------
// Snapshot
// ---------------------------------------------------------------------------

/// The AI section as sent to the browser.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiSnapshot {
    pub provider: String,
    pub model: String,
    pub custom_base_url: String,
    pub api_key: SecretState,
}

/// The Bring! section as sent to the browser.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BringSnapshot {
    pub email: String,
    pub email_source: ValueSource,
    pub password: SecretState,
}

/// Everything the settings page renders. Secrets appear only as set state.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsSnapshot {
    pub ai: AiSnapshot,
    pub bring: BringSnapshot,
}

/// Build the browser-visible snapshot of the effective configuration.
pub fn snapshot(stored: &StoredSettings, env: Env<'_>) -> SettingsSnapshot {
    let provider = stored.provider.clone().unwrap_or_default();
    // A stored key belongs to the provider it was stored with, so it only
    // applies while that provider is the selected one — and it never borrows
    // the environment key of a provider that is not selected.
    let stored_key_applies = stored.provider.is_some() && stored.api_key.is_some();
    let env_key_set = provider_env_key(&provider, env).is_some();
    let (email, email_source) = match &stored.bring_email {
        Some(value) => (value.clone(), ValueSource::Settings),
        None => match env(BRING_EMAIL_ENV).filter(|value| !value.is_empty()) {
            Some(value) => (value, ValueSource::Environment),
            None => (String::new(), ValueSource::None),
        },
    };
    SettingsSnapshot {
        ai: AiSnapshot {
            provider,
            model: stored.model.clone().unwrap_or_default(),
            custom_base_url: stored.base_url.clone().unwrap_or_default(),
            api_key: secret_state(stored_key_applies, env_key_set),
        },
        bring: BringSnapshot {
            email,
            email_source,
            password: secret_state(
                stored.bring_password.is_some(),
                env(BRING_PASSWORD_ENV).is_some_and(|value| !value.is_empty()),
            ),
        },
    }
}

/// Describe a secret by origin: stored beats environment.
fn secret_state(stored: bool, environment: bool) -> SecretState {
    if stored {
        SecretState {
            set: true,
            source: ValueSource::Settings,
        }
    } else if environment {
        SecretState {
            set: true,
            source: ValueSource::Environment,
        }
    } else {
        SecretState {
            set: false,
            source: ValueSource::None,
        }
    }
}
