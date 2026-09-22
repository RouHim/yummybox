//! Central settings for the AI and Bring! integrations.
//!
//! Values live in the `settings` key/value table (migration 006). Every
//! effective value resolves as: stored value, then environment variable, then
//! unset. Secrets are never returned by the API — only their set state and
//! their origin.

use genai::adapter::AdapterKind;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use crate::bring::BringCredentials;
use crate::db;
use crate::error::AppError;
use crate::llm_import::PROVIDER_CUSTOM;

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

/// The endpoint and key used when talking to one provider.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProviderCredentials {
    pub base_url: Option<String>,
    pub api_key: Option<String>,
}

/// Base URL and API key for `provider`. A stored key only applies to the
/// provider it was stored for; everything else falls back to that provider's
/// API-key environment variable.
pub fn provider_credentials(
    stored: &StoredSettings,
    provider: &str,
    env: Env<'_>,
) -> ProviderCredentials {
    let stored_key_applies =
        stored.provider.as_deref() == Some(provider) && stored.api_key.is_some();
    let api_key = if stored_key_applies {
        stored.api_key.clone()
    } else {
        provider_env_key(provider, env)
    };
    let base_url = if provider == PROVIDER_CUSTOM {
        stored.base_url.clone()
    } else {
        None
    };
    ProviderCredentials { base_url, api_key }
}

/// The AI configuration a flow runs with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectiveAi {
    pub provider: String,
    pub model: String,
    pub base_url: Option<String>,
    pub api_key: Option<String>,
}

impl EffectiveAi {
    /// Borrow as the target used by `llm_import` calls.
    pub fn target(&self) -> crate::llm_import::LlmTarget<'_> {
        crate::llm_import::LlmTarget {
            provider_id: &self.provider,
            base_url: self.base_url.as_deref(),
            api_key: self.api_key.as_deref(),
        }
    }
}

/// Resolve the AI configuration: stored value, then the provider's API-key
/// environment variable, then unset. `Ok(None)` means the integration is not
/// configured (no provider or no model) and the caller answers
/// `llm_not_configured`.
pub fn resolve_ai(stored: &StoredSettings, env: Env<'_>) -> Result<Option<EffectiveAi>, AppError> {
    let (Some(provider), Some(model)) = (stored.provider.clone(), stored.model.clone()) else {
        return Ok(None);
    };
    let credentials = provider_credentials(stored, &provider, env);
    if provider == PROVIDER_CUSTOM && credentials.base_url.is_none() {
        return Err(AppError::Validation(
            "customBaseUrl must be set when the provider is custom".to_string(),
        ));
    }
    Ok(Some(EffectiveAi {
        provider,
        model,
        base_url: credentials.base_url,
        api_key: credentials.api_key,
    }))
}

/// Resolve the Bring! credentials: stored value, then environment variable,
/// then unset.
pub fn resolve_bring(stored: &StoredSettings, env: Env<'_>) -> Option<BringCredentials> {
    let email = stored
        .bring_email
        .clone()
        .or_else(|| env(BRING_EMAIL_ENV).filter(|value| !value.is_empty()))?;
    let password = stored
        .bring_password
        .clone()
        .or_else(|| env(BRING_PASSWORD_ENV).filter(|value| !value.is_empty()))?;
    Some(BringCredentials { email, password })
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

// ---------------------------------------------------------------------------
// Commits
// ---------------------------------------------------------------------------

/// A field update: absent leaves the stored value untouched, `null` clears it,
/// a string replaces it.
pub type FieldUpdate = Option<Option<String>>;

/// Deserialize a [`FieldUpdate`]. Serde turns a JSON `null` into a plain
/// `None` for `Option<Option<String>>`, which would make "clear this field"
/// indistinguishable from "leave it untouched", so an explicit `null` is
/// wrapped back into `Some(None)`; a missing field stays `None` through the
/// field's `#[serde(default)]`.
fn field_update<'de, D>(deserializer: D) -> Result<FieldUpdate, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Option::<String>::deserialize(deserializer).map(Some)
}

/// AI fields a commit may change.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AiPatch {
    #[serde(default, deserialize_with = "field_update")]
    pub provider: FieldUpdate,
    #[serde(default, deserialize_with = "field_update")]
    pub model: FieldUpdate,
    #[serde(default, deserialize_with = "field_update")]
    pub custom_base_url: FieldUpdate,
    #[serde(default, deserialize_with = "field_update")]
    pub api_key: FieldUpdate,
}

/// Bring! fields a commit may change.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BringPatch {
    #[serde(default, deserialize_with = "field_update")]
    pub email: FieldUpdate,
    #[serde(default, deserialize_with = "field_update")]
    pub password: FieldUpdate,
}

/// A settings commit. Omitting a section or a field leaves it untouched.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SettingsPatch {
    #[serde(default)]
    pub ai: Option<AiPatch>,
    #[serde(default)]
    pub bring: Option<BringPatch>,
}

/// A validated commit: keys to upsert and keys to remove.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct SettingWrites {
    pub set: Vec<(&'static str, String)>,
    pub delete: Vec<&'static str>,
}

/// Validate a patch and turn it into the writes it performs. A rejection names
/// the offending field and the violated constraint; nothing is written.
pub fn plan_writes(patch: &SettingsPatch) -> Result<SettingWrites, AppError> {
    let mut writes = SettingWrites::default();
    if let Some(ai) = &patch.ai {
        plan_text(
            &mut writes,
            ai.provider.as_ref(),
            KEY_LLM_PROVIDER,
            validate_provider,
        )?;
        plan_text(&mut writes, ai.model.as_ref(), KEY_LLM_MODEL, |value| {
            validate_len("model", value, MAX_MODEL_LEN)
        })?;
        plan_text(
            &mut writes,
            ai.custom_base_url.as_ref(),
            KEY_LLM_BASE_URL,
            validate_base_url,
        )?;
        plan_text(&mut writes, ai.api_key.as_ref(), KEY_LLM_API_KEY, |value| {
            validate_len("apiKey", value, MAX_API_KEY_LEN)
        })?;
    }
    if let Some(bring) = &patch.bring {
        plan_text(
            &mut writes,
            bring.email.as_ref(),
            KEY_BRING_EMAIL,
            |value| validate_len("email", value, MAX_BRING_EMAIL_LEN),
        )?;
        plan_secret(
            &mut writes,
            bring.password.as_ref(),
            KEY_BRING_PASSWORD,
            "password",
            MAX_BRING_PASSWORD_LEN,
        )?;
    }
    Ok(writes)
}

/// Record a text field: absent = untouched, `null` = clear, blank = clear,
/// otherwise validate and set.
fn plan_text(
    writes: &mut SettingWrites,
    update: Option<&Option<String>>,
    key: &'static str,
    validate: impl FnOnce(&str) -> Result<(), AppError>,
) -> Result<(), AppError> {
    let Some(update) = update else {
        return Ok(());
    };
    let Some(raw) = update else {
        writes.delete.push(key);
        return Ok(());
    };
    let value = raw.trim();
    if value.is_empty() {
        writes.delete.push(key);
        return Ok(());
    }
    validate(value)?;
    writes.set.push((key, value.to_string()));
    Ok(())
}

/// Record a secret field: stored verbatim, because leading or trailing spaces
/// may be part of the value; only `null` or an empty string clears it. The
/// field name is passed in so the rejection names the field that was sent.
fn plan_secret(
    writes: &mut SettingWrites,
    update: Option<&Option<String>>,
    key: &'static str,
    field: &str,
    max: usize,
) -> Result<(), AppError> {
    let Some(update) = update else {
        return Ok(());
    };
    let Some(value) = update else {
        writes.delete.push(key);
        return Ok(());
    };
    if value.is_empty() {
        writes.delete.push(key);
        return Ok(());
    }
    validate_len(field, value, max)?;
    writes.set.push((key, value.clone()));
    Ok(())
}

fn validate_provider(value: &str) -> Result<(), AppError> {
    validate_len("provider", value, MAX_PROVIDER_LEN)?;
    let ids = crate::llm_import::provider_ids();
    if !ids.contains(&value) {
        return Err(AppError::Validation(format!(
            "provider must be one of {}, got '{value}'",
            ids.join(", ")
        )));
    }
    Ok(())
}

fn validate_len(field: &str, value: &str, max: usize) -> Result<(), AppError> {
    if value.chars().count() > max {
        return Err(AppError::Validation(format!(
            "{field} must be at most {max} characters, got {}",
            value.chars().count()
        )));
    }
    Ok(())
}

fn validate_base_url(value: &str) -> Result<(), AppError> {
    validate_len("customBaseUrl", value, MAX_BASE_URL_LEN)?;
    let lower = value.to_ascii_lowercase();
    if !(lower.starts_with("http://") || lower.starts_with("https://")) {
        return Err(AppError::Validation(
            "customBaseUrl must start with http:// or https://".to_string(),
        ));
    }
    if value.chars().any(char::is_whitespace) {
        return Err(AppError::Validation(
            "customBaseUrl must not contain whitespace".to_string(),
        ));
    }
    Ok(())
}

/// Apply a commit: validate, then write every change in one transaction, so a
/// rejected or failing commit leaves the previous values in place.
pub async fn apply(pool: &SqlitePool, patch: &SettingsPatch) -> Result<(), AppError> {
    let writes = plan_writes(patch)?;
    db::apply_setting_writes(pool, &writes.set, &writes.delete).await
}
