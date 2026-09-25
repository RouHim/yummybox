# Central Settings Page (AI + Bring!) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a `/settings` page where the AI provider/model/endpoint/API key and the Bring! email/password are configured once, stored server-side in the existing SQLite database, and consumed by every AI and Bring! flow.

**Architecture:** A `settings` key/value table in the existing database backs a new `src/settings.rs` domain module that owns validation, persistence and *effective-configuration resolution* (stored value → environment variable → unset). Two handlers (`GET`/`PATCH /api/settings`) expose a snapshot in which secrets appear only as `{ set, source }`. Every AI flow resolves its provider, model, base URL and key server-side from that store, so the request no longer carries AI configuration. The Svelte side gets one settings-backed `LlmConfigPicker` (used by the settings page, the add-meal dialog and the generate page), a new `/settings` route with a Bring! section, and a gear control in the top bar. The legacy `yummybox-llm-config` localStorage module is deleted.

**Tech Stack:** Rust (axum 0.8, sqlx 0.9 runtime queries, genai 0.6, thiserror), Svelte 5 runes + TypeScript, Vitest, Playwright.

**Spec:** `.spec/zentrales-einstellungsmenue.md`

## Global Constraints

Copied verbatim from the spec and the repository rules; every task implicitly includes them.

- FR-004: Settings persist server-side in the application database and survive restarts; no browser storage is required.
- FR-005: Text values persist on blur or Enter, never per keystroke; selections persist on change.
- FR-006: Every commit shows a saving, saved or failure state; no commit may fail silently.
- FR-007: Stored secrets are never returned in full; only the set state and the origin (settings or environment).
- FR-008: Stored secrets are replaceable and clearable; clearing falls back to the environment value, otherwise reports not configured.
- FR-009: Effective configuration = stored value, then environment variable, then unset.
- FR-010: Environment values are displayed as inherited and are never replaced by an empty stored value.
- FR-017: All new user-facing strings exist in English and German; the settings page uses the existing design tokens, dark mode and reduced-motion rules.
- FR-018: The legacy localStorage key `yummybox-llm-config` is not read by any flow and is not required by any flow.
- FR-019: A rejected commit names the offending field and the violated constraint in the response body and leaves previously committed values unchanged.
- Voice: no em-dashes in UI strings; avoid colloquialisms and comma splices.
- Test naming: `given_<precondition>_when_<action>_then_<expected_result>` (Rust), `it('<observable behavior>')` under `describe('<unit>')` (Vitest).
- Lint gates: `cargo fmt --all -- --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cd web && npm run check`.
- Flat modules only: every new Rust file is declared with `mod` in `src/main.rs`, no nested directories.
- The app has no authentication: secrets are plaintext in the database file; operators are documented to treat file permissions and network exposure as the boundary.

## Review Focus

The five input classes and failure modes this feature implies that no single task's happy path exercises. Each line is pinned by a test in the task named in brackets.

1. **Secrets leaking back out** — a stored API key or Bring! password must never appear in a response body, in an error message, in a tracing span or log line, or in the rendered page. [Task 2: response-body assertions and the `#[instrument(skip(state, payload))]` on the commit handler; Task 8: an E2E assertion that reads the whole page text]
2. **Out-of-order commits** — a slow provider commit landing after a newer model commit would silently clear the model. [Task 6: `SettingsCommitter` serialization test]
3. **Clearing a value that has an environment fallback** — clearing must restore the environment value in the same commit, never leave the integration in a state the user did not ask for. [Task 3: snapshot after clear with injected environment; Task 5: Bring! resolution after clear]
4. **Blank, whitespace-only and over-long values** — an empty string, an over-long model, a base URL without scheme or with whitespace must be rejected with the field named, leaving every other committed value untouched. [Task 3: `plan_writes` boundary tests]
5. **Stored values from an older or foreign writer** — an unknown provider id, a blank value or an over-long value in the table must be treated as unset instead of failing the settings page or an AI flow. [Task 1: `StoredSettings::from_pairs` tests]

---

## Task 1: Settings table and the settings snapshot (`GET /api/settings`)

**Files:**
- Create: `migrations/006_settings.sql`
- Create: `src/settings.rs`
- Create: `src/settings_tests.rs`
- Modify: `src/db.rs` (append the settings section at the end of the file)
- Modify: `src/main.rs` (module declarations next to `mod routes;`, route table near `"/bring/status"`)
- Modify: `src/routes.rs` (new handler section at the end of the file)
- Modify: `src/routes_tests.rs` (router in `setup()` at lines 36-59, import list at lines 19-23)

**Interfaces:**
- Consumes: `db::init_db`, `AppState { pool }`, `AppError::Validation`.
- Produces:
  - `settings::KEY_LLM_PROVIDER|KEY_LLM_MODEL|KEY_LLM_BASE_URL|KEY_LLM_API_KEY|KEY_BRING_EMAIL|KEY_BRING_PASSWORD: &str`
  - `settings::Env<'a> = &'a dyn Fn(&str) -> Option<String>`
  - `settings::ValueSource { Settings, Environment, None }` (serde: `"settings" | "environment" | "none"`)
  - `settings::SecretState { set: bool, source: ValueSource }`
  - `settings::StoredSettings { provider, model, base_url, api_key, bring_email, bring_password: Option<String> }`
  - `settings::StoredSettings::from_pairs(Vec<(String, String)>) -> StoredSettings`
  - `settings::load(&SqlitePool) -> Result<StoredSettings, AppError>`
  - `settings::snapshot(&StoredSettings, Env) -> SettingsSnapshot`
  - `settings::env_lookup(key: &str) -> Option<String>`
  - `db::list_settings(&SqlitePool) -> Result<Vec<(String, String)>, AppError>`
  - `routes::get_settings` (`GET /api/settings`)
  - `llm_import::PROVIDER_CUSTOM: &str` and `llm_import::provider_ids() -> Vec<&'static str>`

- [ ] **Step 1: Write the failing test for the storage schema**

Create `src/settings_tests.rs` with the storage-read tests. `init_db` runs the migrations, so the test only needs a temp database.

```rust
// Tests for the settings domain module. Kept in a separate flat module so
// src/settings.rs stays focused on production code.

use crate::db;
use crate::settings::{
    KEY_BRING_EMAIL, KEY_BRING_PASSWORD, KEY_LLM_API_KEY, KEY_LLM_BASE_URL, KEY_LLM_MODEL,
    KEY_LLM_PROVIDER, SecretState, StoredSettings, ValueSource, env_lookup, load, snapshot,
};

async fn setup_db() -> (sqlx::SqlitePool, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("tempdir");
    let db_path = dir.path().join("test.db");
    let pool = db::init_db(&db_path).await.expect("init_db");
    (pool, dir)
}

/// Environment lookup with nothing set.
fn empty_env() -> impl Fn(&str) -> Option<String> {
    |_: &str| None
}

#[tokio::test]
async fn given_fresh_database_when_list_settings_then_empty() {
    let (pool, _dir) = setup_db().await;
    let rows = db::list_settings(&pool).await.expect("list_settings");
    assert!(rows.is_empty());
}

#[tokio::test]
async fn given_stored_rows_when_load_then_fields_are_populated() {
    let (pool, _dir) = setup_db().await;
    let mut tx = pool.begin().await.expect("begin");
    sqlx::query("INSERT INTO settings (key, value) VALUES (?1, ?2)")
        .bind(KEY_LLM_PROVIDER)
        .bind("openai")
        .execute(&mut *tx)
        .await
        .expect("insert provider");
    sqlx::query("INSERT INTO settings (key, value) VALUES (?1, ?2)")
        .bind(KEY_LLM_MODEL)
        .bind("gpt-4o-mini")
        .execute(&mut *tx)
        .await
        .expect("insert model");
    tx.commit().await.expect("commit");

    let stored = load(&pool).await.expect("load");
    assert_eq!(stored.provider.as_deref(), Some("openai"));
    assert_eq!(stored.model.as_deref(), Some("gpt-4o-mini"));
    assert_eq!(stored.api_key, None);
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test --quiet given_fresh_database_when_list_settings_then_empty`
Expected: FAIL — `unresolved import crate::settings` / no `settings` table.

- [ ] **Step 3: Add the migration**

Create `migrations/006_settings.sql`:

```sql
CREATE TABLE IF NOT EXISTS settings (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
```

- [ ] **Step 4: Add the database accessors**

Append to `src/db.rs` (after the image helpers, at the end of the file):

```rust
// ---------------------------------------------------------------------------
// Settings
// ---------------------------------------------------------------------------

/// Read every stored setting as (key, value) pairs.
pub async fn list_settings(pool: &SqlitePool) -> Result<Vec<(String, String)>, AppError> {
    let rows: Vec<(String, String)> = sqlx::query_as("SELECT key, value FROM settings")
        .fetch_all(pool)
        .await?;
    Ok(rows)
}
```

- [ ] **Step 5: Write the failing test for the snapshot resolution**

Append to `src/settings_tests.rs`:

```rust
#[tokio::test]
async fn given_stored_secret_when_snapshot_then_set_from_settings_and_value_hidden() {
    let (pool, _dir) = setup_db().await;
    let mut tx = pool.begin().await.expect("begin");
    for (key, value) in [
        (KEY_LLM_PROVIDER, "openai"),
        (KEY_LLM_MODEL, "gpt-4o-mini"),
        (KEY_LLM_API_KEY, "sk-secret"),
        (KEY_BRING_EMAIL, "cook@example.com"),
        (KEY_BRING_PASSWORD, "hunter2"),
    ] {
        sqlx::query("INSERT INTO settings (key, value) VALUES (?1, ?2)")
            .bind(key)
            .bind(value)
            .execute(&mut *tx)
            .await
            .expect("insert setting");
    }
    tx.commit().await.expect("commit");

    let stored = load(&pool).await.expect("load");
    let snapshot = snapshot(&stored, &empty_env());

    assert_eq!(snapshot.ai.provider, "openai");
    assert_eq!(snapshot.ai.model, "gpt-4o-mini");
    assert_eq!(
        snapshot.ai.api_key,
        SecretState { set: true, source: ValueSource::Settings }
    );
    assert_eq!(snapshot.bring.email, "cook@example.com");
    assert_eq!(snapshot.bring.email_source, ValueSource::Settings);
    assert_eq!(
        snapshot.bring.password,
        SecretState { set: true, source: ValueSource::Settings }
    );

    // The snapshot is the only thing the browser ever sees: it must not carry
    // either secret value in any form.
    let json = serde_json::to_string(&snapshot).expect("serialize snapshot");
    assert!(!json.contains("sk-secret"), "api key leaked: {json}");
    assert!(!json.contains("hunter2"), "bring password leaked: {json}");
}

#[test]
fn given_no_stored_values_and_env_key_when_snapshot_then_inherited_from_environment() {
    let stored = StoredSettings::default();
    let env = |key: &str| match key {
        "OPENAI_API_KEY" => Some("sk-env".to_string()),
        "BRING_EMAIL" => Some("env@example.com".to_string()),
        "BRING_PASSWORD" => Some("env-pass".to_string()),
        _ => None,
    };
    // The provider has to be set before its environment key can apply.
    let stored = StoredSettings {
        provider: Some("openai".to_string()),
        ..stored
    };

    let snapshot = snapshot(&stored, &env);

    assert_eq!(snapshot.ai.provider, "openai");
    assert_eq!(snapshot.ai.api_key.source, ValueSource::Environment);
    assert!(snapshot.ai.api_key.set);
    assert_eq!(snapshot.bring.email, "env@example.com");
    assert_eq!(snapshot.bring.email_source, ValueSource::Environment);
    assert_eq!(snapshot.bring.password.source, ValueSource::Environment);
}

#[test]
fn given_provider_without_matching_key_when_snapshot_then_api_key_not_set() {
    let stored = StoredSettings {
        provider: Some("anthropic".to_string()),
        api_key: Some("sk-openai".to_string()),
        ..StoredSettings::default()
    };
    let env = |key: &str| (key == "OPENAI_API_KEY").then(|| "sk-env".to_string());

    let snapshot = snapshot(&stored, &env);

    // A stored key only belongs to the provider it was stored for.
    assert_eq!(
        snapshot.ai.api_key,
        SecretState { set: false, source: ValueSource::None }
    );
}

#[test]
fn given_blank_or_unknown_stored_values_when_read_then_treated_as_unset() {
    let stored = StoredSettings::from_pairs(vec![
        (KEY_LLM_PROVIDER.to_string(), "   ".to_string()),
        (KEY_LLM_MODEL.to_string(), "gpt-4o-mini".to_string()),
        (KEY_LLM_BASE_URL.to_string(), "x".repeat(3000)),
        (KEY_BRING_EMAIL.to_string(), String::new()),
    ]);

    assert_eq!(stored.provider, None, "blank provider must be unset");
    assert_eq!(stored.model.as_deref(), Some("gpt-4o-mini"));
    assert_eq!(stored.base_url, None, "over-long base url must be unset");
    assert_eq!(stored.bring_email, None, "empty email must be unset");
}

#[test]
fn given_unknown_provider_id_when_read_then_treated_as_unset() {
    let stored = StoredSettings::from_pairs(vec![(
        KEY_LLM_PROVIDER.to_string(),
        "gpt-5-legacy".to_string(),
    )]);
    assert_eq!(stored.provider, None);
}

#[test]
fn given_stored_password_with_spaces_when_read_then_preserved() {
    let stored = StoredSettings::from_pairs(vec![(
        KEY_BRING_PASSWORD.to_string(),
        " pa ss ".to_string(),
    )]);
    assert_eq!(stored.bring_password.as_deref(), Some(" pa ss "));
}

#[test]
fn given_env_lookup_when_reading_unset_variable_then_none() {
    assert_eq!(env_lookup("YUMMYBOX_DEFINITELY_UNSET_VARIABLE"), None);
}
```

- [ ] **Step 6: Run the tests to verify they fail**

Run: `cargo test --quiet settings_tests`
Expected: FAIL — `settings` module does not exist.

- [ ] **Step 7: Implement `src/settings.rs`**

```rust
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
    let stored_key_applies =
        stored.provider.as_deref() == Some(provider.as_str()) && stored.api_key.is_some();
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
```

`BringCredentials`, `resolve_bring`, `ProviderCredentials`, `provider_credentials`, `EffectiveAi` and `resolve_ai` are deliberately **not** part of this task: nothing in this task's production code calls them, and an unused `pub fn` in a binary crate fails the `clippy --all-targets -D warnings` gate. They arrive in the task that first consumes them (Task 4 for the AI pair, Task 5 for the Bring! pair).

- [ ] **Step 8: Add the provider vocabulary to `llm_import.rs`**

Replace the local `let kinds` table inside `list_providers()` with a module-level table and add the id list (the body of `list_providers` is rewritten in Task 3; for now only the table moves):

```rust
/// The synthetic OpenAI-compatible provider id.
pub const PROVIDER_CUSTOM: &str = "custom";

/// Provider adapters offered by the app, with their display names.
const PROVIDER_KINDS: &[(AdapterKind, &str)] = &[
    (AdapterKind::OpenAI, "OpenAI"),
    (AdapterKind::Anthropic, "Anthropic"),
    (AdapterKind::Gemini, "Gemini"),
    (AdapterKind::Groq, "Groq"),
    (AdapterKind::Ollama, "Ollama"),
    (AdapterKind::DeepSeek, "DeepSeek"),
    (AdapterKind::Xai, "xAI"),
];

/// Every provider id the settings accept: the adapter kinds plus the
/// synthetic OpenAI-compatible `custom` provider.
pub fn provider_ids() -> Vec<&'static str> {
    PROVIDER_KINDS
        .iter()
        .map(|(kind, _)| kind.as_lower_str())
        .chain(std::iter::once(PROVIDER_CUSTOM))
        .collect()
}
```

In `list_providers()`, change `let kinds: &[(AdapterKind, &str)] = &[...]` to `let kinds = PROVIDER_KINDS;` and the appended custom provider to use `PROVIDER_CUSTOM.to_string()` for `id`. Keep everything else identical.

- [ ] **Step 9: Run the settings tests**

Run: `cargo test --quiet settings_tests`
Expected: PASS (7 tests).

- [ ] **Step 10: Write the failing route test**

Append to `src/routes_tests.rs` (after the existing version-route tests):

```rust
// -----------------------------------------------------------------------
// Settings routes
// -----------------------------------------------------------------------

#[tokio::test]
async fn given_empty_settings_when_get_settings_then_blank_snapshot() {
    let ctx = setup().await;
    let response = ctx
        .app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/settings")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), 8192).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["ai"]["provider"], "");
    assert_eq!(json["ai"]["model"], "");
    assert_eq!(json["ai"]["customBaseUrl"], "");
    assert_eq!(json["ai"]["apiKey"]["set"], false);
    assert_eq!(json["ai"]["apiKey"]["source"], "none");
    assert_eq!(json["bring"]["email"], "");
    assert_eq!(json["bring"]["emailSource"], "none");
    assert_eq!(json["bring"]["password"]["set"], false);
}

#[tokio::test]
async fn given_stored_secrets_when_get_settings_then_values_absent_from_body() {
    let ctx = setup().await;
    ctx.seed_setting("llm.provider", "openai").await;
    ctx.seed_setting("llm.model", "gpt-4o-mini").await;
    ctx.seed_setting("llm.api_key", "sk-super-secret").await;
    ctx.seed_setting("bring.email", "cook@example.com").await;
    ctx.seed_setting("bring.password", "super-secret-pass").await;

    let response = ctx
        .app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/settings")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), 8192).await.unwrap();
    let text = String::from_utf8(body.to_vec()).unwrap();

    assert!(!text.contains("sk-super-secret"), "api key leaked: {text}");
    assert!(!text.contains("super-secret-pass"), "password leaked: {text}");

    let json: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(json["ai"]["apiKey"]["set"], true);
    assert_eq!(json["ai"]["apiKey"]["source"], "settings");
    assert_eq!(json["bring"]["email"], "cook@example.com");
    assert_eq!(json["bring"]["emailSource"], "settings");
    assert_eq!(json["bring"]["password"]["set"], true);
}
```

Extend `TestCtx` and `setup()` so tests can reach the database, and add the seeding helper:

```rust
struct TestCtx {
    app: Router,
    pool: sqlx::SqlitePool,
    _dir: tempfile::TempDir,
}
```

In `setup()`, clone the pool before it moves into `AppState`:

```rust
    let state = Arc::new(AppState { pool: pool.clone() });
```

Return `TestCtx { app, pool, _dir: dir }`, and add the router line (both here and in `src/main.rs`):

```rust
        .route("/settings", get(get_settings))
```

Import `get_settings` in the `use crate::routes::{...}` list, and add the helper:

```rust
impl TestCtx {
    /// Write a stored setting directly, bypassing the HTTP layer.
    async fn seed_setting(&self, key: &str, value: &str) {
        sqlx::query("INSERT INTO settings (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = excluded.value")
            .bind(key)
            .bind(value)
            .execute(&self.pool)
            .await
            .expect("seed setting");
    }
}
```

- [ ] **Step 11: Run the route tests to verify they fail**

Run: `cargo test --quiet settings_routes -- --nocapture` (or the two test names)
Expected: FAIL — `unresolved import crate::routes::get_settings`.

- [ ] **Step 12: Add the handler**

Append to `src/routes.rs`:

```rust
// ---------------------------------------------------------------------------
// Settings handler
// ---------------------------------------------------------------------------

/// The stored AI and Bring! settings, with secrets reduced to their set state.
#[instrument(skip(state))]
pub async fn get_settings(
    State(state): State<Arc<AppState>>,
) -> Result<Json<crate::settings::SettingsSnapshot>, AppError> {
    let stored = crate::settings::load(&state.pool).await?;
    Ok(Json(crate::settings::snapshot(
        &stored,
        &crate::settings::env_lookup,
    )))
}
```

`patch_settings` is added in Task 2; for this task route `get(get_settings)` only, and add `.patch(patch_settings)` in Task 2.

- [ ] **Step 13: Declare the modules and route**

In `src/main.rs`, add next to the other module declarations:

```rust
mod settings;
#[cfg(test)]
mod settings_tests;
```

In the API router in `src/main.rs`, add after the `/bring/status` route:

```rust
        .route("/settings", get(routes::get_settings))
```

- [ ] **Step 14: Run the tests**

Run: `cargo test --quiet settings && cargo test --quiet given_stored_secrets_when_get_settings_then_values_absent_from_body`
Expected: PASS.

- [ ] **Step 15: Commit**

```bash
git add migrations/006_settings.sql src/settings.rs src/settings_tests.rs src/db.rs src/main.rs src/routes.rs src/routes_tests.rs src/llm_import.rs src/bring.rs
git commit -m "feat(settings): store AI and Bring! settings server-side with a safe snapshot API"
```

---

## Task 2: Committing settings (`PATCH /api/settings`)

**Files:**
- Modify: `src/settings.rs` (append the patch types, `plan_writes`, `apply`)
- Modify: `src/settings_tests.rs` (append patch tests)
- Modify: `src/db.rs` (append `apply_setting_writes`)
- Modify: `src/routes.rs` (add `patch_settings`)
- Modify: `src/main.rs` (add `.patch(...)` to the `/settings` route)
- Modify: `src/routes_tests.rs` (append PATCH route tests)

**Interfaces:**
- Consumes: everything from Task 1.
- Produces:
  - `settings::FieldUpdate = Option<Option<String>>` (absent = untouched, `null` = clear, string = set)
  - `settings::AiPatch { provider, model, custom_base_url, api_key: FieldUpdate }`
  - `settings::BringPatch { email, password: FieldUpdate }`
  - `settings::SettingsPatch { ai: Option<AiPatch>, bring: Option<BringPatch> }`
  - `settings::SettingWrites { set: Vec<(&'static str, String)>, delete: Vec<&'static str> }`
  - `settings::plan_writes(&SettingsPatch) -> Result<SettingWrites, AppError>`
  - `settings::apply(&SqlitePool, &SettingsPatch) -> Result<(), AppError>`
  - `db::apply_setting_writes(&SqlitePool, &[(&str, String)], &[&str]) -> Result<(), AppError>`
  - `routes::patch_settings` (`PATCH /api/settings`)

- [ ] **Step 1: Write the failing validation tests**

Append to `src/settings_tests.rs`:

```rust
use crate::settings::{
    AiPatch, BringPatch, SettingsPatch, SettingWrites, apply, plan_writes,
};

fn ai_patch(provider: Option<Option<&str>>, model: Option<Option<&str>>) -> SettingsPatch {
    SettingsPatch {
        ai: Some(AiPatch {
            provider: provider.map(|value| value.map(str::to_string)),
            model: model.map(|value| value.map(str::to_string)),
            ..AiPatch::default()
        }),
        bring: None,
    }
}

#[test]
fn given_provider_and_model_when_plan_writes_then_both_set() {
    let patch = ai_patch(Some(Some("openai")), Some(Some("gpt-4o-mini")));
    let writes = plan_writes(&patch).expect("valid patch");
    assert_eq!(
        writes,
        SettingWrites {
            set: vec![
                ("llm.provider", "openai".to_string()),
                ("llm.model", "gpt-4o-mini".to_string()),
            ],
            delete: vec![],
        }
    );
}

#[test]
fn given_null_field_when_plan_writes_then_key_deleted() {
    let patch = ai_patch(None, Some(None));
    let writes = plan_writes(&patch).expect("valid patch");
    assert_eq!(writes.set, vec![]);
    assert_eq!(writes.delete, vec!["llm.model"]);
}

#[test]
fn given_absent_field_when_plan_writes_then_untouched() {
    let patch = ai_patch(Some(Some("openai")), None);
    let writes = plan_writes(&patch).expect("valid patch");
    assert_eq!(writes.set.len(), 1);
    assert!(writes.delete.is_empty());
}

#[test]
fn given_blank_string_when_plan_writes_then_treated_as_clear() {
    let patch = ai_patch(Some(Some("   ")), Some(Some("")));
    let writes = plan_writes(&patch).expect("valid patch");
    assert_eq!(writes.set, vec![]);
    assert_eq!(writes.delete, vec!["llm.provider", "llm.model"]);
}

#[test]
fn given_unknown_provider_when_plan_writes_then_rejected_naming_field() {
    let patch = ai_patch(Some(Some("gpt-5-legacy")), None);
    let err = plan_writes(&patch).expect_err("must reject");
    let message = err.to_string();
    assert!(message.contains("provider must be one of"), "{message}");
    assert!(message.contains("gpt-5-legacy"), "{message}");
}

#[test]
fn given_overlong_model_when_plan_writes_then_rejected_naming_field() {
    let long = "m".repeat(201);
    let patch = ai_patch(None, Some(Some(&long)));
    let err = plan_writes(&patch).expect_err("must reject");
    let message = err.to_string();
    assert!(message.contains("model must be at most 200 characters"), "{message}");
}

#[test]
fn given_base_url_without_scheme_when_plan_writes_then_rejected() {
    let patch = SettingsPatch {
        ai: Some(AiPatch {
            custom_base_url: Some(Some("localhost:8080/v1/".to_string())),
            ..AiPatch::default()
        }),
        bring: None,
    };
    let err = plan_writes(&patch).expect_err("must reject");
    assert!(
        err.to_string()
            .contains("customBaseUrl must start with http:// or https://"),
        "{err}"
    );
}

#[test]
fn given_base_url_with_whitespace_when_plan_writes_then_rejected() {
    let patch = SettingsPatch {
        ai: Some(AiPatch {
            custom_base_url: Some(Some("http://local host/v1/".to_string())),
            ..AiPatch::default()
        }),
        bring: None,
    };
    let err = plan_writes(&patch).expect_err("must reject");
    assert!(
        err.to_string().contains("customBaseUrl must not contain whitespace"),
        "{err}"
    );
}

#[tokio::test]
async fn given_valid_patch_when_apply_then_stored_and_snapshot_updated() {
    let (pool, _dir) = setup_db().await;
    let patch = SettingsPatch {
        ai: Some(AiPatch {
            provider: Some(Some("custom".to_string())),
            model: Some(Some("llama3".to_string())),
            custom_base_url: Some(Some("http://localhost:8080/v1/".to_string())),
            api_key: Some(Some("local-key".to_string())),
        }),
        bring: None,
    };
    apply(&pool, &patch).await.expect("apply");

    let snapshot = snapshot(&load(&pool).await.expect("load"), &empty_env());
    assert_eq!(snapshot.ai.provider, "custom");
    assert_eq!(snapshot.ai.model, "llama3");
    assert_eq!(snapshot.ai.custom_base_url, "http://localhost:8080/v1/");
    assert_eq!(snapshot.ai.api_key.source, ValueSource::Settings);
}

#[tokio::test]
async fn given_stored_secret_when_cleared_then_environment_value_is_effective_again() {
    let (pool, _dir) = setup_db().await;
    apply(
        &pool,
        &SettingsPatch {
            ai: Some(AiPatch {
                provider: Some(Some("openai".to_string())),
                model: Some(Some("gpt-4o-mini".to_string())),
                api_key: Some(Some("sk-stored".to_string())),
                ..AiPatch::default()
            }),
            bring: Some(BringPatch {
                email: Some(Some("stored@example.com".to_string())),
                password: Some(Some("stored-pass".to_string())),
            }),
        },
    )
    .await
    .expect("apply");

    apply(
        &pool,
        &SettingsPatch {
            ai: Some(AiPatch {
                api_key: Some(None),
                ..AiPatch::default()
            }),
            bring: Some(BringPatch {
                email: Some(None),
                password: Some(None),
            }),
        },
    )
    .await
    .expect("clear");

    let env = |key: &str| match key {
        "OPENAI_API_KEY" => Some("sk-env".to_string()),
        "BRING_EMAIL" => Some("env@example.com".to_string()),
        "BRING_PASSWORD" => Some("env-pass".to_string()),
        _ => None,
    };
    let stored = load(&pool).await.expect("load");
    let snapshot = snapshot(&stored, &env);
    assert_eq!(snapshot.ai.api_key.source, ValueSource::Environment);
    assert_eq!(snapshot.bring.email, "env@example.com");
    assert_eq!(snapshot.bring.email_source, ValueSource::Environment);
    assert_eq!(snapshot.bring.password.source, ValueSource::Environment);
}

#[tokio::test]
async fn given_password_with_spaces_when_stored_then_preserved_verbatim() {
    let (pool, _dir) = setup_db().await;
    apply(
        &pool,
        &SettingsPatch {
            bring: Some(BringPatch {
                password: Some(Some(" pa ss ".to_string())),
                email: None,
            }),
            ai: None,
        },
    )
    .await
    .expect("apply");

    let stored = load(&pool).await.expect("load");
    assert_eq!(stored.bring_password.as_deref(), Some(" pa ss "));
}

#[test]
fn given_overlong_password_when_plan_writes_then_rejected_naming_field() {
    let long = "p".repeat(257);
    let patch = SettingsPatch {
        ai: None,
        bring: Some(BringPatch {
            email: None,
            password: Some(Some(long)),
        }),
    };
    let err = plan_writes(&patch).expect_err("must reject");
    assert!(
        err.to_string().contains("password must be at most 256 characters"),
        "{err}"
    );
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --quiet plan_writes`
Expected: FAIL — `crate::settings::AiPatch` does not exist.

- [ ] **Step 3: Implement the patch types and validation**

Append to `src/settings.rs`:

```rust
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
        plan_text(&mut writes, bring.email.as_ref(), KEY_BRING_EMAIL, |value| {
            validate_len("email", value, MAX_BRING_EMAIL_LEN)
        })?;
        plan_secret(
            &mut writes,
            bring.password.as_ref(),
            KEY_BRING_PASSWORD,
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
/// may be part of the value; only `null` or an empty string clears it.
fn plan_secret(
    writes: &mut SettingWrites,
    update: Option<&Option<String>>,
    key: &'static str,
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
    validate_len("password", value, max)?;
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
```

- [ ] **Step 4: Add the transactional writer**

Append to `src/db.rs`:

```rust
/// Apply a settings commit atomically: `set` pairs are upserted and `delete`
/// keys removed inside one transaction, so a failing commit changes nothing.
pub async fn apply_setting_writes(
    pool: &SqlitePool,
    set: &[(&str, String)],
    delete: &[&str],
) -> Result<(), AppError> {
    let mut tx = pool.begin().await?;
    for (key, value) in set {
        sqlx::query(
            "INSERT INTO settings (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        )
        .bind(key)
        .bind(value)
        .execute(&mut *tx)
        .await?;
    }
    for key in delete {
        sqlx::query("DELETE FROM settings WHERE key = ?1")
            .bind(key)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(())
}
```

- [ ] **Step 5: Run the settings tests**

Run: `cargo test --quiet settings_tests`
Expected: PASS.

- [ ] **Step 6: Write the failing route tests**

Append to `src/routes_tests.rs`:

```rust
async fn patch_settings_json(ctx: &TestCtx, body: serde_json::Value) -> (StatusCode, serde_json::Value) {
    let response = ctx
        .app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::PATCH)
                .uri("/settings")
                .header("content-type", "application/json")
                .body(axum::body::Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 8192).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    (status, json)
}

#[tokio::test]
async fn given_provider_and_model_when_patch_settings_then_snapshot_reports_them() {
    let ctx = setup().await;
    let (status, json) = patch_settings_json(
        &ctx,
        json!({ "ai": { "provider": "openai", "model": "gpt-4o-mini" } }),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["ai"]["provider"], "openai");
    assert_eq!(json["ai"]["model"], "gpt-4o-mini");
}

#[tokio::test]
async fn given_unknown_provider_when_patch_settings_then_400_names_field_and_keeps_values() {
    let ctx = setup().await;
    let (status, _) = patch_settings_json(&ctx, json!({ "ai": { "provider": "openai" } })).await;
    assert_eq!(status, StatusCode::OK);

    let (status, json) = patch_settings_json(
        &ctx,
        json!({ "ai": { "provider": "gpt-5-legacy" } }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(
        json["error"]
            .as_str()
            .unwrap()
            .contains("provider must be one of"),
        "{json}"
    );

    let response = ctx
        .app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/settings")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let bytes = to_bytes(response.into_body(), 8192).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(
        json["ai"]["provider"], "openai",
        "a rejected commit must not change stored values"
    );
}

#[tokio::test]
async fn given_unknown_field_when_patch_settings_then_400() {
    let ctx = setup().await;
    let (status, _) = patch_settings_json(&ctx, json!({ "ai": { "providerr": "openai" } })).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn given_stored_secret_when_patched_then_response_never_contains_it() {
    let ctx = setup().await;
    let (status, json) = patch_settings_json(
        &ctx,
        json!({ "ai": { "provider": "openai", "apiKey": "sk-super-secret" } }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let text = json.to_string();
    assert!(!text.contains("sk-super-secret"), "api key leaked: {text}");
    assert_eq!(json["ai"]["apiKey"]["set"], true);
    assert_eq!(json["ai"]["apiKey"]["source"], "settings");
}

#[tokio::test]
async fn given_credentials_when_cleared_then_settings_fall_back_to_environment() {
    let ctx = setup().await;
    ctx.seed_setting("bring.email", "stored@example.com").await;
    ctx.seed_setting("bring.password", "stored-pass").await;

    let (status, json) = patch_settings_json(
        &ctx,
        json!({ "bring": { "email": null, "password": null } }),
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["bring"]["emailSource"], "none");
    assert_eq!(json["bring"]["password"]["set"], false);
}

#[tokio::test]
async fn given_overlong_model_when_patch_settings_then_400_names_field() {
    let ctx = setup().await;
    let long = "m".repeat(201);
    let (status, json) = patch_settings_json(&ctx, json!({ "ai": { "model": long } })).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(
        json["error"]
            .as_str()
            .unwrap()
            .contains("model must be at most 200 characters"),
        "{json}"
    );
}
```

- [ ] **Step 7: Run the route tests to verify they fail**

Run: `cargo test --quiet when_patch_settings`
Expected: FAIL — no route matches PATCH `/settings` (the test router in `src/routes_tests.rs` is not nested under `/api`; only the real router in `src/main.rs` is).

- [ ] **Step 8: Add the handler and route**

In `src/routes.rs`, extend the settings section from Task 1:

```rust
#[instrument(skip(state, payload))]
pub async fn patch_settings(
    State(state): State<Arc<AppState>>,
    payload: Result<Json<crate::settings::SettingsPatch>, JsonRejection>,
) -> Result<Json<crate::settings::SettingsSnapshot>, AppError> {
    let Json(patch) = payload.map_err(|rejection| AppError::BadRequest(rejection.body_text()))?;
    crate::settings::apply(&state.pool, &patch).await?;
    let stored = crate::settings::load(&state.pool).await?;
    Ok(Json(crate::settings::snapshot(
        &stored,
        &crate::settings::env_lookup,
    )))
}
```

The extractor is taken as `Result<Json<_>, JsonRejection>` rather than `Json<_>` because axum answers a JSON data rejection with 422 and a plain-text body, while this project's error contract is a structured `{"error": "..."}` body with the field named (FR-019). `JsonRejection` comes from `axum::extract::rejection::JsonRejection`, and `body_text()` names the offending field for an unknown or mistyped key.

Use `.route("/settings", get(routes::get_settings).patch(routes::patch_settings))` in `src/main.rs`, and the same line (without the `routes::` prefix) in the test router in `src/routes_tests.rs`. Add `patch_settings` to the `use crate::routes::{...}` import list in `src/routes_tests.rs`; `axum::routing::get` is already imported there, and `MethodRouter::patch` needs no extra import.

- [ ] **Step 9: Run the tests**

Run: `cargo test --quiet settings`
Expected: PASS (all settings and settings-route tests).

- [ ] **Step 10: Run clippy and fmt**

Run: `cargo fmt && cargo clippy --all-targets --all-features -- -D warnings`
Expected: clean.

- [ ] **Step 11: Commit**

```bash
git add src/settings.rs src/settings_tests.rs src/db.rs src/routes.rs src/main.rs src/routes_tests.rs
git commit -m "feat(settings): add validated PATCH /api/settings with atomic commits"
```

---

## Task 3: Provider list reflects stored keys

**Files:**
- Modify: `src/llm_import.rs` (`LlmProviderInfo`, `list_providers`)
- Modify: `src/import.rs` (`llm_providers` handler)
- Modify: `src/routes_tests.rs` (provider-list tests)

**Interfaces:**
- Consumes: `settings::load`, `settings::StoredSettings`.
- Produces:
  - `llm_import::LlmProviderInfo { id, name, env_var, env_key_set, configured, supports_custom_endpoint }`
  - `llm_import::list_providers(stored_provider: Option<&str>, stored_key_set: bool) -> Vec<LlmProviderInfo>`

- [ ] **Step 1: Write the failing tests**

This task's assertions read the process environment, so they must control it: a developer or CI shell with `OPENAI_API_KEY` exported would otherwise decide the outcome. Rename the existing `BRING_ENV_LOCK` static in `src/routes_tests.rs` to `PROCESS_ENV_LOCK`, update its doc comment to "Serializes tests that mutate process-global environment variables; a concurrent restore in one test could otherwise land between another test's remove_var and its assertion. tokio Mutex: the guard is held across awaits.", update its three existing uses in the Bring! tests (`given_missing_bring_credentials_when_send_then_returns_400` and its sibling), and use it for the tests below.

Append to `src/routes_tests.rs`:

```rust
/// Read one provider entry from the `/llm/providers` response.
fn find_provider<'a>(json: &'a serde_json::Value, id: &str) -> &'a serde_json::Value {
    json["providers"]
        .as_array()
        .expect("providers array")
        .iter()
        .find(|p| p["id"].as_str() == Some(id))
        .unwrap_or_else(|| panic!("provider {id} missing"))
}

async fn get_providers(ctx: &TestCtx) -> serde_json::Value {
    let response = ctx
        .app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/llm/providers")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), 8192).await.unwrap();
    serde_json::from_slice(&body).unwrap()
}

#[tokio::test]
async fn given_stored_key_for_provider_when_list_providers_then_provider_configured() {
    let _guard = PROCESS_ENV_LOCK.lock().await;
    let had_key = std::env::var("OPENAI_API_KEY").ok();
    unsafe { std::env::remove_var("OPENAI_API_KEY") };

    let ctx = setup().await;
    ctx.seed_setting("llm.provider", "openai").await;
    ctx.seed_setting("llm.api_key", "sk-stored").await;

    let json = get_providers(&ctx).await;
    let openai = find_provider(&json, "openai");
    assert_eq!(openai["configured"], serde_json::Value::Bool(true));
    assert_eq!(openai["envKeySet"], serde_json::Value::Bool(false));
    assert_eq!(openai["envVar"], "OPENAI_API_KEY");

    // The stored key belongs to OpenAI only.
    assert_eq!(
        find_provider(&json, "anthropic")["configured"],
        serde_json::Value::Bool(false)
    );
    // A provider that needs no key stays selectable.
    assert_eq!(
        find_provider(&json, "custom")["configured"],
        serde_json::Value::Bool(true)
    );

    if let Some(value) = had_key {
        unsafe { std::env::set_var("OPENAI_API_KEY", value) };
    }
}

#[tokio::test]
async fn given_blank_env_key_when_list_providers_then_env_key_not_set() {
    let _guard = PROCESS_ENV_LOCK.lock().await;
    let had_key = std::env::var("OPENAI_API_KEY").ok();
    unsafe { std::env::set_var("OPENAI_API_KEY", "") };

    let ctx = setup().await;
    let json = get_providers(&ctx).await;
    let openai = find_provider(&json, "openai");

    // A present-but-empty variable is an unset key, exactly as
    // `settings::provider_env_key` treats it, so the two endpoints agree.
    assert_eq!(openai["envKeySet"], serde_json::Value::Bool(false));
    assert_eq!(openai["configured"], serde_json::Value::Bool(false));

    match had_key {
        Some(value) => unsafe { std::env::set_var("OPENAI_API_KEY", value) },
        None => unsafe { std::env::remove_var("OPENAI_API_KEY") },
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --quiet given_stored_key_for_provider_when_list_providers_then_provider_configured given_blank_env_key_when_list_providers_then_env_key_not_set`
Expected: FAIL — `openai["configured"]` is `false` (and `envKeySet` is missing).

- [ ] **Step 3: Rewrite `list_providers`**

In `src/llm_import.rs`, replace the whole `list_providers` function (and its doc comment) with:

```rust
/// Returns the list of LLM providers with their usability.
///
/// `configured` is true when the provider can be used right now: it needs no
/// API key, its API-key environment variable is set, or a key is stored in the
/// settings for exactly this provider. `env_key_set` reports the environment
/// variable alone, so the UI can distinguish a stored key from an inherited one.
pub fn list_providers(stored_provider: Option<&str>, stored_key_set: bool) -> Vec<LlmProviderInfo> {
    let mut providers: Vec<LlmProviderInfo> = PROVIDER_KINDS
        .iter()
        .map(|(kind, name)| {
            let id = kind.as_lower_str().to_string();
            let env_var = kind.default_key_env_name().unwrap_or("").to_string();
            // A present-but-empty variable counts as unset, matching
            // `settings::provider_env_key`, so both endpoints agree.
            let env_key_set = !env_var.is_empty()
                && std::env::var(&env_var).is_ok_and(|value| !value.is_empty());
            let stored_applies = stored_key_set && stored_provider == Some(id.as_str());
            LlmProviderInfo {
                configured: needs_no_api_key(&id) || env_key_set || stored_applies,
                id,
                name: (*name).to_string(),
                env_var,
                env_key_set,
                supports_custom_endpoint: false,
            }
        })
        .collect();

    // Append the synthetic "custom" OpenAI-compatible endpoint provider
    providers.push(LlmProviderInfo {
        id: PROVIDER_CUSTOM.to_string(),
        name: "Custom OpenAI-compatible".to_string(),
        env_var: String::new(),
        env_key_set: false,
        configured: true,
        supports_custom_endpoint: true,
    });

    providers
}

/// Providers that work without an API key.
fn needs_no_api_key(id: &str) -> bool {
    id == "ollama" || id == PROVIDER_CUSTOM
}
```

and the struct:

```rust
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LlmProviderInfo {
    pub id: String,
    pub name: String,
    pub env_var: String,
    /// Whether the provider's API-key environment variable holds a value.
    pub env_key_set: bool,
    /// Whether the provider can be used right now.
    pub configured: bool,
    pub supports_custom_endpoint: bool,
}
```

- [ ] **Step 4: Pass the stored key into the handler**

Replace `llm_providers` in `src/import.rs`:

```rust
#[instrument(skip(state))]
pub(crate) async fn llm_providers(
    State(state): State<Arc<AppState>>,
) -> Result<Json<crate::llm_import::LlmProvidersResponse>, AppError> {
    let stored = crate::settings::load(&state.pool).await?;
    Ok(Json(crate::llm_import::LlmProvidersResponse {
        providers: crate::llm_import::list_providers(
            stored.provider.as_deref(),
            stored.api_key.is_some(),
        ),
    }))
}
```

- [ ] **Step 5: Run the tests**

Run: `cargo test --quiet list_providers`
Expected: PASS — including the pre-existing `given_no_api_keys_when_list_providers_then_ollama_configured` and `list_providers_includes_custom`.

- [ ] **Step 6: Commit**

```bash
git add src/llm_import.rs src/import.rs src/routes_tests.rs
git commit -m "feat(settings): mark providers configured from the stored API key"
```

---

## Task 4: AI flows resolve the effective configuration

**Files:**
- Modify: `src/llm_import.rs` (`LlmTarget`, `build_model_spec` → `LlmTarget` methods, `import_via_llm`, `generate_meal_via_llm`, `polish_instructions`, `list_models`, unit tests)
- Modify: `src/error.rs` (status for `llm_not_configured`)
- Modify: `src/import.rs` (`import_from_llm`, `generate_meal`, `polish_instructions`, `llm_models`, `effective_ai` helper)
- Modify: `src/routes_tests.rs` (all `/import/llm`, `/import/generate`, `/llm/polish`, `/llm/models` tests)

**Interfaces:**
- Consumes: `settings::resolve_ai`, `settings::provider_credentials`, `settings::EffectiveAi::target()`, `settings::env_lookup`.
- Produces:
  - `llm_import::LlmTarget<'a> { provider_id: &'a str, base_url: Option<&'a str>, api_key: Option<&'a str> }`
  - `llm_import::list_models(&LlmTarget<'_>) -> Result<Vec<String>, AppError>`
  - `llm_import::import_via_llm(&LlmTarget<'_>, model: &str, hint: Option<&str>, images: Vec<LlmImage>, skip_image_download: bool) -> Result<ImportDraft, AppError>`
  - `llm_import::generate_meal_via_llm(&LlmTarget<'_>, model: &str, ingredients: Option<&str>, images: Vec<LlmImage>) -> Result<ImportDraft, AppError>`
  - `llm_import::polish_instructions(&LlmTarget<'_>, model: &str, meal_name: &str, ingredients: &[NewIngredientLine], instructions: &str) -> Result<String, AppError>`
  - Error code `llm_not_configured` → HTTP 400

- [ ] **Step 1: Write the failing unit test for the target**

Replace `given_valid_args_when_build_model_spec_then_succeeds` in `src/llm_import.rs` with:

```rust
    #[test]
    fn given_standard_provider_when_model_spec_then_adapter_from_provider() {
        let target = LlmTarget {
            provider_id: "anthropic",
            base_url: None,
            api_key: None,
        };
        let debug = format!(
            "{:?}",
            target.model_spec("claude-3-5-sonnet").expect("model spec")
        );
        assert!(debug.contains("claude-3-5-sonnet"));
        assert!(debug.contains("Anthropic"));
    }

    #[test]
    fn given_custom_provider_when_model_spec_then_custom_endpoint_with_key() {
        let target = LlmTarget {
            provider_id: PROVIDER_CUSTOM,
            base_url: Some("http://localhost:8080/v1"),
            api_key: Some("sk-123"),
        };
        let debug = format!("{:?}", target.model_spec("local-model").expect("model spec"));
        assert!(debug.contains("localhost:8080/v1/"));
        assert!(debug.contains("local-model"));
        assert!(debug.contains("sk-123"));
    }

    #[test]
    fn given_custom_provider_without_base_url_when_model_spec_then_rejected() {
        let target = LlmTarget {
            provider_id: PROVIDER_CUSTOM,
            base_url: None,
            api_key: None,
        };
        let err = target.model_spec("local-model").expect_err("must reject");
        assert!(err.to_string().contains("customBaseUrl must be set"), "{err}");
    }

    #[test]
    fn given_unknown_provider_when_model_spec_then_rejected() {
        let target = LlmTarget {
            provider_id: "gpt-5-legacy",
            base_url: None,
            api_key: None,
        };
        let err = target.model_spec("gpt-4o-mini").expect_err("must reject");
        assert!(err.to_string().contains("unknown provider"), "{err}");
    }

    #[test]
    fn given_no_key_when_target_then_no_auth_data() {
        let target = LlmTarget {
            provider_id: "openai",
            base_url: None,
            api_key: None,
        };
        assert!(target.auth().is_none());
    }

    #[test]
    fn given_key_when_target_then_auth_data_holds_it() {
        let target = LlmTarget {
            provider_id: "openai",
            base_url: None,
            api_key: Some("sk-stored"),
        };
        let debug = format!("{:?}", target.auth());
        assert!(debug.contains("sk-stored"));
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --quiet llm_import::tests`
Expected: FAIL — `LlmTarget` does not exist.

- [ ] **Step 3: Implement `LlmTarget`**

In `src/llm_import.rs`, add near the top (after the public types) and delete `build_model_spec`:

```rust
/// The provider endpoint and key a request runs against, resolved from the
/// stored settings (or their environment fallback) before the call.
#[derive(Debug, Clone, Copy)]
pub struct LlmTarget<'a> {
    pub provider_id: &'a str,
    pub base_url: Option<&'a str>,
    pub api_key: Option<&'a str>,
}

impl LlmTarget<'_> {
    /// The resolved key as auth data, `None` when no key is configured.
    fn auth(&self) -> Option<genai::resolver::AuthData> {
        self.api_key
            .map(str::trim)
            .filter(|key| !key.is_empty())
            .map(genai::resolver::AuthData::from_single)
    }

    /// Client with the resolved key installed as the auth resolver. Without a
    /// key the client falls back to genai's environment resolution.
    fn client(&self) -> genai::Client {
        match self.auth() {
            Some(auth) => genai::Client::builder()
                .with_auth_resolver_fn(move |_model_iden: genai::ModelIden| Ok(Some(auth.clone())))
                .build(),
            None => genai::Client::default(),
        }
    }

    /// Adapter kind for a standard provider.
    fn adapter_kind(&self) -> Result<AdapterKind, AppError> {
        AdapterKind::from_lower_str(self.provider_id).ok_or_else(|| {
            AppError::Validation(format!("unknown provider: {}", self.provider_id))
        })
    }

    /// Normalized endpoint for the custom OpenAI-compatible provider.
    fn custom_endpoint(&self) -> Result<genai::resolver::Endpoint, AppError> {
        let base_url = self
            .base_url
            .map(str::trim)
            .filter(|url| !url.is_empty())
            .ok_or_else(|| {
                AppError::Validation(
                    "customBaseUrl must be set when the provider is custom".to_string(),
                )
            })?;
        let base_url = if base_url.ends_with('/') {
            base_url.to_string()
        } else {
            format!("{base_url}/")
        };
        Ok(genai::resolver::Endpoint::from_owned(base_url))
    }

    /// Model spec for chat calls: a fully resolved service target for the
    /// custom endpoint, the provider's adapter otherwise.
    fn model_spec(&self, model: &str) -> Result<genai::ModelSpec, AppError> {
        if self.provider_id == PROVIDER_CUSTOM {
            return Ok(genai::ServiceTarget {
                endpoint: self.custom_endpoint()?,
                auth: self.auth().unwrap_or(genai::resolver::AuthData::None),
                model: genai::ModelIden::new(AdapterKind::OpenAI, model),
            }
            .into());
        }
        Ok(genai::ModelIden::new(self.adapter_kind()?, model).into())
    }

    /// Adapter and provider config for model listing.
    fn listing_config(
        &self,
    ) -> Result<(AdapterKind, genai::resolver::ProviderConfig), AppError> {
        if self.provider_id == PROVIDER_CUSTOM {
            let auth = self.auth().unwrap_or(genai::resolver::AuthData::None);
            return Ok((
                AdapterKind::OpenAI,
                genai::resolver::ProviderConfig::from((self.custom_endpoint()?, auth)),
            ));
        }
        let config = match self.auth() {
            Some(auth) => genai::resolver::ProviderConfig::from_auth(auth),
            None => genai::resolver::ProviderConfig::default(),
        };
        Ok((self.adapter_kind()?, config))
    }
}
```

Update the three call sites and `list_models`:

```rust
pub async fn list_models(target: &LlmTarget<'_>) -> Result<Vec<String>, AppError> {
    let client = genai::Client::default();
    let (adapter_kind, provider_config) = target.listing_config()?;
    let models_fut = client.all_model_names(adapter_kind, provider_config);
    // ... existing 15 s timeout block, unchanged ...
}

pub async fn import_via_llm(
    target: &LlmTarget<'_>,
    model: &str,
    hint: Option<&str>,
    images: Vec<LlmImage>,
    skip_image_download: bool,
) -> Result<recipe::ImportDraft, AppError> {
    let client = target.client();
    // ... unchanged until the call ...
    let model_spec = target.model_spec(model)?;
    // ... unchanged ...
}

pub async fn generate_meal_via_llm(
    target: &LlmTarget<'_>,
    model: &str,
    ingredients: Option<&str>,
    images: Vec<LlmImage>,
) -> Result<recipe::ImportDraft, AppError> { /* same shape */ }

pub async fn polish_instructions(
    target: &LlmTarget<'_>,
    model: &str,
    meal_name: &str,
    ingredients: &[NewIngredientLine],
    instructions: &str,
) -> Result<String, AppError> { /* same shape */ }
```

In `map_genai_error`, extend the missing-key message so it points at the settings page:

```rust
            AppError::Llm(
                format!(
                    "API key not configured for provider '{}': store one in Settings or set the {} environment variable",
                    model_iden.adapter_kind, env_var
                ),
                "llm_api_key_missing",
            )
```

The module-level `use genai::resolver::{AuthData, Endpoint, ProviderConfig};` becomes unused because `LlmTarget` refers to them through full paths; reduce it to `use genai::resolver::AuthData;` (or delete it entirely) so `cargo clippy -- -D warnings` stays clean. Run `cargo clippy` in Step 5 to confirm.

- [ ] **Step 4: Map the new error code to 400**

In `src/error.rs`, extend the `AppError::Llm` status match:

```rust
                let status = match *code {
                    "llm_api_key_missing" | "llm_not_configured" => StatusCode::BAD_REQUEST,
                    "llm_parse_failed" => StatusCode::UNPROCESSABLE_ENTITY,
                    _ => StatusCode::INTERNAL_SERVER_ERROR,
                };
```

- [ ] **Step 5: Run the unit tests**

Run: `cargo test --quiet llm_import::tests`
Expected: PASS.

- [ ] **Step 6: Add the AI resolvers to `src/settings.rs`**

These were deliberately held back from Task 1: only this task's handlers call them. Append to `src/settings.rs`, restore the `use crate::llm_import::PROVIDER_CUSTOM;` import at the top, and add the tests below to `src/settings_tests.rs`:

```rust
// ---------------------------------------------------------------------------
// Effective configuration
// ---------------------------------------------------------------------------

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
```

```rust
#[test]
fn given_no_provider_or_model_when_resolve_ai_then_not_configured() {
    let env = empty_env();
    assert!(resolve_ai(&StoredSettings::default(), &env).expect("ok").is_none());

    let model_only = StoredSettings {
        model: Some("gpt-4o-mini".to_string()),
        ..StoredSettings::default()
    };
    assert!(resolve_ai(&model_only, &env).expect("ok").is_none());
}

#[test]
fn given_stored_ai_when_resolved_then_stored_key_wins_over_environment() {
    let stored = StoredSettings {
        provider: Some("openai".to_string()),
        model: Some("gpt-4o-mini".to_string()),
        api_key: Some("sk-stored".to_string()),
        ..StoredSettings::default()
    };
    let env = |key: &str| (key == "OPENAI_API_KEY").then(|| "sk-env".to_string());

    let resolved = resolve_ai(&stored, &env).expect("ok").expect("configured");

    assert_eq!(resolved.provider, "openai");
    assert_eq!(resolved.model, "gpt-4o-mini");
    assert_eq!(resolved.base_url, None);
    assert_eq!(resolved.api_key.as_deref(), Some("sk-stored"));
}

#[test]
fn given_no_stored_key_when_resolved_then_provider_environment_key_applies() {
    let stored = StoredSettings {
        provider: Some("openai".to_string()),
        model: Some("gpt-4o-mini".to_string()),
        ..StoredSettings::default()
    };
    let env = |key: &str| (key == "OPENAI_API_KEY").then(|| "sk-env".to_string());

    let resolved = resolve_ai(&stored, &env).expect("ok").expect("configured");
    assert_eq!(resolved.api_key.as_deref(), Some("sk-env"));
}

#[test]
fn given_custom_provider_without_base_url_when_resolved_then_rejected() {
    let stored = StoredSettings {
        provider: Some("custom".to_string()),
        model: Some("llama3".to_string()),
        ..StoredSettings::default()
    };
    let err = resolve_ai(&stored, &empty_env()).expect_err("must reject");
    assert!(err.to_string().contains("customBaseUrl must be set"), "{err}");
}

#[test]
fn given_standard_provider_with_foreign_stored_key_when_resolved_then_key_not_applied() {
    let stored = StoredSettings {
        provider: Some("anthropic".to_string()),
        model: Some("claude-3-5-sonnet".to_string()),
        api_key: Some("sk-openai".to_string()),
        ..StoredSettings::default()
    };
    let env = |key: &str| (key == "ANTHROPIC_API_KEY").then(|| "sk-anthropic".to_string());

    let resolved = resolve_ai(&stored, &env).expect("ok").expect("configured");
    assert_eq!(resolved.api_key.as_deref(), Some("sk-anthropic"));
}
```

Add `resolve_ai` (and, for the assertions that inspect the resolved target, `ProviderCredentials`/`provider_credentials`) to the `use crate::settings::{...}` import list of `src/settings_tests.rs`.

- [ ] **Step 7: Rewrite the handlers**

In `src/import.rs`, add the resolver and drop the configuration fields from the three multipart handlers:

```rust
/// Resolve the effective AI configuration, or report that AI is not
/// configured and the settings page has to be visited first.
pub(crate) async fn effective_ai(
    pool: &sqlx::SqlitePool,
) -> Result<crate::settings::EffectiveAi, AppError> {
    let stored = crate::settings::load(pool).await?;
    crate::settings::resolve_ai(&stored, &crate::settings::env_lookup)?.ok_or_else(|| {
        AppError::Llm(
            "AI is not configured: choose a provider and a model in Settings".to_string(),
            "llm_not_configured",
        )
    })
}
```

`import_from_llm`: delete the `model`, `base_url` and `api_key` arms from the multipart loop and their post-loop extraction; rename the extractor parameter `State(_state)` to `State(state)` and the attribute `#[instrument(skip(_state))]` to `#[instrument(skip(state))]`; and replace the call, placing the resolution **after** the existing hint/image validation so those error messages keep their precedence:

```rust
    // (existing validation of MAX_GENERATE_IMAGES, MAX_IMAGE_BYTES,
    //  the all-empty guard and the hint length limit stays here, unchanged)

    let ai = effective_ai(&state.pool).await?;
    let skip_image_download = !llm_images.is_empty();

    let mut draft = crate::llm_import::import_via_llm(
        &ai.target(),
        &ai.model,
        hint.as_deref(),
        llm_images,
        skip_image_download,
    )
    .await?;
```

`generate_meal`: same treatment (rename the state extractor and its `skip` attribute); keep the existing image/ingredient validation where it is and insert the resolution directly before the call:

```rust
    let ai = effective_ai(&state.pool).await?;
    let draft = crate::llm_import::generate_meal_via_llm(
        &ai.target(),
        &ai.model,
        ingredients.as_deref(),
        images,
    )
    .await?;
```

`polish_instructions`: same treatment; the call becomes

```rust
    let ai = effective_ai(&state.pool).await?;
    let polished = crate::llm_import::polish_instructions(
        &ai.target(),
        &ai.model,
        &name,
        &ingredients,
        &instructions,
    )
    .await?;
```

`llm_models`: replace the query struct and handler:

```rust
#[derive(Debug, serde::Deserialize)]
pub(crate) struct ModelsQuery {
    pub(crate) provider: String,
}

#[instrument(skip(state))]
pub(crate) async fn llm_models(
    State(state): State<Arc<AppState>>,
    Query(q): Query<ModelsQuery>,
) -> Result<Json<crate::llm_import::LlmModelsResponse>, AppError> {
    if !crate::llm_import::provider_ids().contains(&q.provider.as_str()) {
        return Err(AppError::BadRequest(format!(
            "unknown provider: {}",
            q.provider
        )));
    }
    let stored = crate::settings::load(&state.pool).await?;
    let credentials =
        crate::settings::provider_credentials(&stored, &q.provider, &crate::settings::env_lookup);
    let target = crate::llm_import::LlmTarget {
        provider_id: &q.provider,
        base_url: credentials.base_url.as_deref(),
        api_key: credentials.api_key.as_deref(),
    };
    let models = crate::llm_import::list_models(&target).await?;
    Ok(Json(crate::llm_import::LlmModelsResponse { models }))
}
```

- [ ] **Step 8: Migrate the route tests**

The AI configuration no longer travels in the request, so both multipart builders shrink to their remaining fields:

- `build_llm_multipart(model, hint, images, base_url, api_key)` becomes `build_llm_multipart(hint: Option<&str>, images: &[&[u8]])`
- `build_generate_multipart(model, ingredients, images)` becomes `build_generate_multipart(ingredients: Option<&str>, images: &[&[u8]])`

Update every call site in the file accordingly (the compiler finds them all), and keep each builder's existing boundary/field-writing code otherwise untouched. Add this helper next to them:

```rust
/// Point the stored AI configuration at a local mock provider so a route test
/// can reach the LLM without env vars.
async fn seed_custom_ai(ctx: &TestCtx, base_url: &str, model: &str) {
    ctx.seed_setting("llm.provider", "custom").await;
    ctx.seed_setting("llm.base_url", base_url).await;
    ctx.seed_setting("llm.model", model).await;
    ctx.seed_setting("llm.api_key", "test-key").await;
}
```

Apply these rules to every test that targets `/import/llm`, `/import/generate` or `/llm/polish`:

| Test group | Change |
|---|---|
| `given_empty_body_when_import_llm_then_400` | The body now fails the existing hint/image guard before the AI resolution: keep the test and the status assertion, and replace the `contains("missing 'model' field")` assertion with `contains("at least one of image or hint is required")` |
| `given_no_fields_when_generate_meal_then_400_missing_model` | Rename to `given_no_fields_when_generate_meal_then_400` and replace the `contains("model")` assertion with `assert_eq!(response.status(), StatusCode::BAD_REQUEST)` — the existing ingredients/image guard rejects the request before the AI resolution |
| `given_missing_model_when_polish_instructions_then_returns_400` (its body now carries only a `name` field) | Rename to `given_missing_ingredients_when_polish_instructions_then_400`; the request is still rejected with 400, now by the existing `missing 'ingredients' field` guard, before the AI resolution |
| `given_body_over_50mb_when_polish_instructions_then_413` | Unchanged |
| Tests that only assert 400/413 on size or count limits (hint over 20000 chars, 6 images, oversized image, empty image field, image over 20 MB) | Drop the removed builder arguments; the request is rejected before the AI call, so no seeding is needed |
| Tests that reach the mock LLM (`given_empty_image_field_with_hint_when_import_llm_then_draft_returned`, the multi-image ordering test, and every `/import/generate` test that expects a draft) | Call `seed_custom_ai(&ctx, &format!("http://127.0.0.1:{port}/v1/"), "test-model").await` right after `setup()` and before sending the request |
| `given_unknown_provider_when_list_models_then_400` | Unchanged |
| `given_custom_provider_no_base_url_when_list_models_then_400` | Unchanged (no stored base URL ⇒ 400) |

Add this test for the not-configured path (it is the only route test that reaches the AI resolution with no stored provider):

```rust
#[tokio::test]
async fn given_no_stored_ai_config_when_generate_meal_then_400_not_configured() {
    let ctx = setup().await;
    let (body, content_type) = build_generate_multipart(Some("flour\neggs"), &[]);
    let response = ctx
        .app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/import/generate")
                .header("content-type", content_type)
                .body(axum::body::Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let bytes = to_bytes(response.into_body(), 4096).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(json["code"], "llm_not_configured");
}
```

And this one, which proves the flows read the stored settings rather than the request:

```rust
#[tokio::test]
async fn given_stored_custom_provider_when_list_models_then_uses_stored_endpoint() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let ctx = setup().await;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut buf = Vec::new();
        let mut byte = [0u8; 1];
        while !buf.ends_with(b"\r\n\r\n") {
            stream.read_exact(&mut byte).await.unwrap();
            buf.push(byte[0]);
        }
        let body = r#"{"object":"list","data":[{"id":"stored-model"}]}"#;
        let response = format!(
            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(response.as_bytes()).await.unwrap();
        stream.flush().await.unwrap();
    });

    seed_custom_ai(&ctx, &format!("http://127.0.0.1:{port}/v1/"), "stored-model").await;

    let response = ctx
        .app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/llm/models?provider=custom")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = to_bytes(response.into_body(), 4096).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(json["models"][0], "stored-model");
}
```

- [ ] **Step 9: Run the Rust suite**

Run: `cargo test --quiet`
Expected: PASS.

- [ ] **Step 10: Run clippy and fmt**

Run: `cargo fmt && cargo clippy --all-targets --all-features -- -D warnings`
Expected: clean.

- [ ] **Step 11: Commit**

```bash
git add src/llm_import.rs src/import.rs src/error.rs src/routes_tests.rs
git commit -m "feat(settings): resolve AI provider, model, endpoint and key server-side"
```

---

## Task 5: Bring! uses the effective credentials

**Files:**
- Modify: `src/bring.rs` (add `BringCredentials`; rewrite `push_item_to_bring`, `check_bring_status`, the login error message, and the tests)
- Modify: `src/settings.rs` (add `resolve_bring`)
- Modify: `src/settings_tests.rs` (add the resolution tests)
- Modify: `src/routes.rs` (`add_bring_item`, `get_bring_status`)
- Modify: `src/routes_tests.rs` (Bring! tests)

**Interfaces:**
- Consumes: `settings::load`, `settings::env_lookup`, `StoredSettings`.
- Produces:
  - `bring::BringCredentials { email: String, password: String }`
  - `bring::push_item_to_bring(&BringCredentials, name: &str, spec: Option<&str>) -> Result<(), AppError>`
  - `bring::check_bring_status(creds: Option<&BringCredentials>) -> BringStatus`
  - `settings::resolve_bring(&StoredSettings, Env) -> Option<BringCredentials>`
  - `routes::BRING_NOT_CONFIGURED: &str`

- [ ] **Step 1: Write the failing tests**

Add the credentials type to `src/bring.rs`, next to `BringStatus`:

```rust
/// Bring! account credentials for one request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BringCredentials {
    pub email: String,
    pub password: String,
}
```

Add `resolve_bring` to `src/settings.rs` (its only consumer is this task's handlers, which is why Task 1 left it out):

```rust
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
```

with `use crate::bring::BringCredentials;` restored at the top of `src/settings.rs`, and replace `given_no_bring_env_vars_when_check_status_then_not_configured` in `src/bring.rs` with:

```rust
    #[tokio::test]
    async fn given_no_credentials_when_check_status_then_not_configured() {
        match check_bring_status(None).await {
            BringStatus::NotConfigured => {}
            other => panic!("expected NotConfigured, got {other:?}"),
        }
    }
```

`check_bring_status(None)` returns before any network call, so this test needs neither credentials nor connectivity.

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test --quiet bring::tests`
Expected: FAIL — `check_bring_status` takes no arguments.

- [ ] **Step 3: Rewrite the bring functions**

In `src/bring.rs`, replace the head of `push_item_to_bring` and `check_bring_status`:

```rust
/// Push a single ingredient to the user's first Bring! shopping list.
/// Truncates `name` to 100 characters if longer.
pub async fn push_item_to_bring(
    creds: &BringCredentials,
    name: &str,
    spec: Option<&str>,
) -> Result<(), AppError> {
    let name = truncate_name(name);

    let client = Client::builder()
        .timeout(Duration::from_secs(10))
        .connect_timeout(Duration::from_secs(5))
        .build()
        .map_err(|e| AppError::Internal(format!("failed to build HTTP client: {e}")))?;

    // Step 1: authenticate
    let auth = bring_login(&client, &creds.email, &creds.password).await?;
    // ... steps 2 and 3 unchanged ...
}

/// Probe the given credentials. `None` means no credentials are configured in
/// either the settings or the environment, and no network call is made.
pub async fn check_bring_status(creds: Option<&BringCredentials>) -> BringStatus {
    let Some(creds) = creds else {
        return BringStatus::NotConfigured;
    };

    let client = match Client::builder()
        .timeout(Duration::from_secs(10))
        .connect_timeout(Duration::from_secs(5))
        .build()
    {
        Ok(c) => c,
        Err(e) => return BringStatus::Error(format!("failed to build HTTP client: {e}")),
    };

    let auth = match bring_login(&client, &creds.email, &creds.password).await {
        Ok(a) => a,
        Err(e) => return BringStatus::Error(e.to_string()),
    };
    // ... unchanged ...
}
```

Change the failed-login message in `bring_login` to:

```rust
            AppError::BringAuthFailed(
                "Bring! login failed — check your Bring! credentials in Settings".to_string(),
            )
```

(Keep the em-dash out of *UI* strings; this is a server-side message that is surfaced verbatim, so replace the em-dash with a comma: `"Bring! login failed, check your Bring! credentials in Settings"`.)

- [ ] **Step 4: Wire the handlers**

In `src/routes.rs`, add above `add_bring_item`:

```rust
/// Reported when neither the settings nor the environment hold credentials.
pub(crate) const BRING_NOT_CONFIGURED: &str =
    "Bring! credentials not configured: set them in Settings or via the BRING_EMAIL and BRING_PASSWORD environment variables";
```

and rewrite both handlers:

```rust
#[instrument(skip(state))]
pub async fn add_bring_item(
    State(state): State<Arc<AppState>>,
    Json(req): Json<BringItemRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    let stored = crate::settings::load(&state.pool).await?;
    let creds = crate::settings::resolve_bring(&stored, &crate::settings::env_lookup)
        .ok_or_else(|| AppError::BadRequest(BRING_NOT_CONFIGURED.to_string()))?;
    bring::push_item_to_bring(&creds, &req.name, req.spec.as_deref()).await?;
    Ok(Json(serde_json::json!({"sent": true})))
}

#[instrument(skip(state))]
pub async fn get_bring_status(
    State(state): State<Arc<AppState>>,
) -> Result<Json<BringStatusResponse>, AppError> {
    let stored = crate::settings::load(&state.pool).await?;
    let creds = crate::settings::resolve_bring(&stored, &crate::settings::env_lookup);
    Ok(Json(BringStatusResponse::from(
        bring::check_bring_status(creds.as_ref()).await,
    )))
}
```

- [ ] **Step 5: Cover the stored and cleared rules without a network call**

The route deliberately makes no test-visible decision of its own: it resolves credentials through `settings::resolve_bring` and hands them to `check_bring_status`. The Rust suite stays offline, so this task pins the resolution rule in `src/settings_tests.rs` (append it there) and leaves the probe itself to the existing `check_bring_status(None)` test plus the mocked E2E coverage in Task 10:

```rust
#[tokio::test]
async fn given_stored_credentials_when_resolved_then_stored_wins_over_environment() {
    let (pool, _dir) = setup_db().await;
    apply(
        &pool,
        &SettingsPatch {
            ai: None,
            bring: Some(BringPatch {
                email: Some(Some("stored@example.com".to_string())),
                password: Some(Some("stored-pass".to_string())),
            }),
        },
    )
    .await
    .expect("apply");

    let env = |key: &str| match key {
        "BRING_EMAIL" => Some("env@example.com".to_string()),
        "BRING_PASSWORD" => Some("env-pass".to_string()),
        _ => None,
    };
    let stored = load(&pool).await.expect("load");
    let credentials = resolve_bring(&stored, &env).expect("credentials");

    assert_eq!(credentials.email, "stored@example.com");
    assert_eq!(credentials.password, "stored-pass");
}
```

and this one, which pins the fall-back-after-clear rule (FR-008) for Bring!:

```rust
#[tokio::test]
async fn given_cleared_value_without_environment_when_cleared_then_not_configured() {
    let (pool, _dir) = setup_db().await;
    apply(
        &pool,
        &SettingsPatch {
            ai: None,
            bring: Some(BringPatch {
                email: Some(Some("stored@example.com".to_string())),
                password: Some(Some("stored-pass".to_string())),
            }),
        },
    )
    .await
    .expect("apply");
    apply(
        &pool,
        &SettingsPatch {
            ai: None,
            bring: Some(BringPatch {
                email: Some(None),
                password: Some(None),
            }),
        },
    )
    .await
    .expect("clear");

    let stored = load(&pool).await.expect("load");
    assert!(resolve_bring(&stored, &empty_env()).is_none());
}
```

Also verify by hand that a failed login still reports the existing no-lists message: `bring_first_list` failures are mapped by the unchanged `Err(e) => BringStatus::Error(e.to_string())` arm, which turns `AppError::BringNoLists` into `"no Bring! lists found in your account"` (the spec's "credentials authenticate but expose no shopping list" edge case).

Both tests need `resolve_bring` in the `use crate::settings::{...}` import list of `src/settings_tests.rs`, and `SettingsPatch`/`BringPatch`/`apply`/`load` are already imported there from Task 2.

- [ ] **Step 6: Run the Rust suite**

Run: `cargo test --quiet`
Expected: PASS, including the pre-existing `given_bring_not_configured_...` tests (which assert the message contains `BRING_EMAIL and BRING_PASSWORD`).

- [ ] **Step 7: Commit**

```bash
git add src/bring.rs src/routes.rs src/routes_tests.rs src/settings.rs src/settings_tests.rs
git commit -m "feat(settings): resolve Bring! credentials from settings with env fallback"
```

---

## Task 6: Frontend API client, settings module, i18n keys, gear icon

**Files:**
- Create: `web/src/lib/settings.svelte.ts`
- Create: `web/src/lib/settings.test.ts`
- Delete: `web/src/lib/llm-config.svelte.ts`, `web/src/lib/llm-config.test.ts`
- Modify: `web/src/lib/api.ts`
- Modify: `web/src/lib/api.test.ts`
- Modify: `web/src/lib/i18n/types.ts`, `web/src/lib/i18n/en.ts`, `web/src/lib/i18n/de.ts`
- Modify: `web/src/lib/Icon.svelte` (add a `settings` icon)

**Interfaces:**
- Produces:
  - `settings.svelte.ts`: `ValueSource`, `SecretState`, `AiSettings`, `BringSettings`, `SettingsSnapshot`, `AiPatch`, `BringPatch`, `SettingsPatch`, `isAiConfigured(snapshot)`, `secretSourceLabelKey(source)`, `commitStatusLabelKey(status)`, `SettingsCommitter`
  - `api.ts`: `getSettings()`, `updateSettings(patch)`, `listLlmModels(provider)`, `importFromLlm(hint, images)`, `generateMeal(ingredients, images)`, `polishInstructions(name, ingredients, instructions)`

- [ ] **Step 1: Write the failing module test**

Create `web/src/lib/settings.test.ts`:

```ts
import { describe, it, expect, vi } from 'vitest';
import {
	SettingsCommitter,
	isAiConfigured,
	type SettingsSnapshot,
} from './settings.svelte';

function snapshot(overrides: Partial<SettingsSnapshot['ai']> = {}): SettingsSnapshot {
	return {
		ai: {
			provider: 'openai',
			model: 'gpt-4o-mini',
			customBaseUrl: '',
			apiKey: { set: true, source: 'settings' },
			...overrides,
		},
		bring: {
			email: '',
			emailSource: 'none',
			password: { set: false, source: 'none' },
		},
	};
}

describe('isAiConfigured', () => {
	it('reports an unloaded snapshot as not configured', () => {
		expect(isAiConfigured(null)).toBe(false);
	});

	it('requires a provider and a model', () => {
		expect(isAiConfigured(snapshot({ provider: '', model: 'gpt-4o-mini' }))).toBe(false);
		expect(isAiConfigured(snapshot({ model: '' }))).toBe(false);
	});

	it('requires a key for providers that need one', () => {
		expect(isAiConfigured(snapshot({ apiKey: { set: false, source: 'none' } }))).toBe(false);
		expect(isAiConfigured(snapshot({ apiKey: { set: true, source: 'environment' } }))).toBe(true);
	});

	it('accepts ollama without a key', () => {
		expect(
			isAiConfigured(snapshot({ provider: 'ollama', apiKey: { set: false, source: 'none' } })),
		).toBe(true);
	});

	it('requires a base URL for the custom provider', () => {
		const base = { provider: 'custom', apiKey: { set: false, source: 'none' as const } };
		expect(isAiConfigured(snapshot({ ...base, customBaseUrl: '' }))).toBe(false);
		expect(isAiConfigured(snapshot({ ...base, customBaseUrl: 'http://localhost:8080/v1/' }))).toBe(true);
	});
});

describe('SettingsCommitter', () => {
	it('keeps commits in order so a slow response cannot overwrite a newer value', async () => {
		const order: string[] = [];
		const resolvers: Array<(value: SettingsSnapshot) => void> = [];
		const send = vi.fn(
			() => new Promise<SettingsSnapshot>((resolve) => resolvers.push(resolve)),
		);
		const applied: string[] = [];
		const committer = new SettingsCommitter(send, (s) => applied.push(s.ai.model));

		const first = committer.commit({ ai: { model: 'a' } });
		const second = committer.commit({ ai: { model: 'b' } });
		order.push(`requests:${send.mock.calls.length}`);
		expect(order).toEqual(['requests:1']);

		resolvers[0](snapshot({ model: 'a' }));
		await first;
		expect(send).toHaveBeenCalledTimes(2);

		resolvers[1](snapshot({ model: 'b' }));
		await second;

		expect(applied).toEqual(['a', 'b']);
		expect(committer.state.status).toBe('saved');
		expect(committer.state.error).toBeNull();
	});

	it('surfaces a failed commit and recovers on the next success', async () => {
		const send = vi
			.fn()
			.mockRejectedValueOnce(new Error('provider must be one of openai'))
			.mockResolvedValueOnce(snapshot());
		const committer = new SettingsCommitter(send, () => {});

		await committer.commit({ ai: { provider: 'nope' } });
		expect(committer.state.status).toBe('error');
		expect(committer.state.error).toContain('provider must be one of');

		await committer.commit({ ai: { provider: 'openai' } });
		expect(committer.state.status).toBe('saved');
		expect(committer.state.error).toBeNull();
	});

	it('reports a saving state while the request is in flight', async () => {
		let resolve!: (value: SettingsSnapshot) => void;
		const send = vi.fn(() => new Promise<SettingsSnapshot>((r) => (resolve = r)));
		const committer = new SettingsCommitter(send, () => {});

		const pending = committer.commit({ ai: { model: 'a' } });
		expect(committer.state.status).toBe('saving');

		resolve(snapshot());
		await pending;
		expect(committer.state.status).toBe('saved');
	});
});
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cd web && npm test -- -t "isAiConfigured"`
Expected: FAIL — `./settings.svelte` does not exist.

- [ ] **Step 3: Implement the settings module**

Create `web/src/lib/settings.svelte.ts`:

```ts
/// <reference types="svelte" />

import type { TranslationKey } from './i18n/types';

export type ValueSource = 'settings' | 'environment' | 'none';

/** A secret as the API reports it: never the value itself. */
export interface SecretState {
	set: boolean;
	source: ValueSource;
}

export interface AiSettings {
	provider: string;
	model: string;
	customBaseUrl: string;
	apiKey: SecretState;
}

export interface BringSettings {
	email: string;
	emailSource: ValueSource;
	password: SecretState;
}

export interface SettingsSnapshot {
	ai: AiSettings;
	bring: BringSettings;
}

/** `null` clears a stored value, a string replaces it, an absent key leaves it untouched. */
export interface AiPatch {
	provider?: string | null;
	model?: string | null;
	customBaseUrl?: string | null;
	apiKey?: string | null;
}

export interface BringPatch {
	email?: string | null;
	password?: string | null;
}

export interface SettingsPatch {
	ai?: AiPatch;
	bring?: BringPatch;
}

/** Providers that work without an API key, mirroring `llm_import::needs_no_api_key`. */
const KEY_OPTIONAL_PROVIDERS = new Set(['ollama', 'custom']);

/** Whether every AI flow has the configuration it needs to run. */
export function isAiConfigured(settings: SettingsSnapshot | null): boolean {
	if (!settings) return false;
	const { provider, model, customBaseUrl, apiKey } = settings.ai;
	if (!provider || !model) return false;
	if (provider === 'custom') return customBaseUrl.trim().length > 0;
	if (KEY_OPTIONAL_PROVIDERS.has(provider)) return true;
	return apiKey.set;
}

/** i18n key describing where a value comes from. */
export function secretSourceLabelKey(source: ValueSource): TranslationKey {
	if (source === 'settings') return 'settingsSecretStored';
	if (source === 'environment') return 'settingsSecretInherited';
	return 'settingsSecretAbsent';
}

export type CommitStatus = 'idle' | 'saving' | 'saved' | 'error';

/** i18n key describing a commit state, or `null` while idle. */
export function commitStatusLabelKey(status: CommitStatus): TranslationKey | null {
	if (status === 'saving') return 'settingsSaving';
	if (status === 'saved') return 'settingsSaved';
	if (status === 'error') return 'settingsSaveFailed';
	return null;
}

/**
 * Serializes settings commits so a slow earlier request can never land after a
 * newer one and overwrite it, and exposes the shared saving/saved/failed state
 * that every commit must show.
 */
export class SettingsCommitter {
	state = $state<{ status: CommitStatus; error: string | null }>({
		status: 'idle',
		error: null,
	});
	#queue: Promise<void> = Promise.resolve();
	#send: (patch: SettingsPatch) => Promise<SettingsSnapshot>;
	#apply: (snapshot: SettingsSnapshot) => void;

	constructor(
		send: (patch: SettingsPatch) => Promise<SettingsSnapshot>,
		apply: (snapshot: SettingsSnapshot) => void,
	) {
		this.#send = send;
		this.#apply = apply;
	}

	commit(patch: SettingsPatch): Promise<void> {
		const run = async () => {
			this.state = { status: 'saving', error: null };
			try {
				const snapshot = await this.#send(patch);
				this.#apply(snapshot);
				this.state = { status: 'saved', error: null };
			} catch (err) {
				this.state = {
					status: 'error',
					error: err instanceof Error ? err.message : String(err),
				};
			}
		};
		this.#queue = this.#queue.then(run, run);
		return this.#queue;
	}
}
```

- [ ] **Step 4: Run the module tests**

Run: `cd web && npm test -- -t "settings"` and `cd web && npm test -- -t "SettingsCommitter"`
Expected: PASS.

- [ ] **Step 5: Write the failing API tests**

Append to `web/src/lib/api.test.ts`:

```ts
import { getSettings, updateSettings, listLlmModels } from './api';

describe('getSettings', () => {
	it('GETs /api/settings and returns the snapshot', async () => {
		mockResponse(200, {
			ai: { provider: 'openai', model: 'gpt-4o-mini', customBaseUrl: '', apiKey: { set: true, source: 'settings' } },
			bring: { email: '', emailSource: 'none', password: { set: false, source: 'none' } },
		});

		const settings = await getSettings();

		expect(mockFetch).toHaveBeenCalledWith('/api/settings', expect.objectContaining({ signal: expect.any(AbortSignal) }));
		expect(settings.ai.provider).toBe('openai');
		expect(settings.ai.apiKey.source).toBe('settings');
	});
});

describe('updateSettings', () => {
	it('PATCHes the given patch as JSON and returns the snapshot', async () => {
		mockResponse(200, {
			ai: { provider: 'custom', model: 'llama3', customBaseUrl: 'http://localhost:8080/v1/', apiKey: { set: false, source: 'none' } },
			bring: { email: '', emailSource: 'none', password: { set: false, source: 'none' } },
		});

		const settings = await updateSettings({ ai: { provider: 'custom', apiKey: null } });

		expect(mockFetch).toHaveBeenCalledWith('/api/settings', expect.objectContaining({
			method: 'PATCH',
			headers: { 'Content-Type': 'application/json' },
			body: JSON.stringify({ ai: { provider: 'custom', apiKey: null } }),
		}));
		expect(settings.ai.provider).toBe('custom');
	});

	it('surfaces the field error from a rejected commit', async () => {
		mockResponse(400, { error: "provider must be one of openai, anthropic, custom, got 'nope'" });

		await expect(updateSettings({ ai: { provider: 'nope' } })).rejects.toThrow(
			'provider must be one of',
		);
	});
});

describe('listLlmModels', () => {
	it('sends only the provider, never a key', async () => {
		mockResponse(200, { models: ['gpt-4o-mini'] });

		const result = await listLlmModels('openai');

		expect(mockFetch).toHaveBeenCalledWith(
			'/api/llm/models?provider=openai',
			expect.objectContaining({ signal: expect.any(AbortSignal) }),
		);
		expect(result.models).toEqual(['gpt-4o-mini']);
	});
});
```

- [ ] **Step 6: Run the API tests to verify they fail**

Run: `cd web && npm test -- -t "getSettings"`
Expected: FAIL — `getSettings` is not exported.

- [ ] **Step 7: Implement the API client changes**

In `web/src/lib/api.ts`:

1. Add to the import at the top: `import type { SettingsPatch, SettingsSnapshot } from './settings.svelte';`
2. Replace the three LLM flow functions:

```ts
export async function importFromLlm(hint: string | null, images: File[]): Promise<ImportDraft> {
    const form = new FormData();
    if (hint && hint.trim()) form.set('hint', hint.trim());
    for (const img of images) form.append('image', img);
    return request<ImportDraft>('/api/import/llm', { method: 'POST', body: form }, 90_000);
}

export async function generateMeal(ingredients: string, images: File[]): Promise<ImportDraft> {
    const form = new FormData();
    if (ingredients.trim()) form.set('ingredients', ingredients);
    for (const img of images) form.append('image', img);
    return request<ImportDraft>('/api/import/generate', { method: 'POST', body: form }, 90_000);
}

export async function polishInstructions(
    name: string,
    ingredients: NewIngredientLine[],
    instructions: string,
): Promise<string> {
    const form = new FormData();
    form.set('name', name);
    form.set('ingredients', JSON.stringify(ingredients));
    form.set('instructions', instructions);
    const data = await request<{ instructions: string }>('/api/llm/polish', { method: 'POST', body: form }, 90_000);
    return data.instructions;
}
```

3. Replace `listLlmModels`:

```ts
export async function listLlmModels(provider: string): Promise<LlmModelsResponse> {
    const params = new URLSearchParams({ provider });
    return request<LlmModelsResponse>(`/api/llm/models?${params}`, undefined, 20_000);
}
```

4. Add the settings API next to the Bring! section:

```ts
// Settings API

export async function getSettings(): Promise<SettingsSnapshot> {
	return request<SettingsSnapshot>('/api/settings');
}

export async function updateSettings(patch: SettingsPatch): Promise<SettingsSnapshot> {
	return request<SettingsSnapshot>('/api/settings', {
		method: 'PATCH',
		headers: { 'Content-Type': 'application/json' },
		body: JSON.stringify(patch),
	});
}
```

- [ ] **Step 8: Delete the legacy localStorage module**

```bash
git rm web/src/lib/llm-config.svelte.ts web/src/lib/llm-config.test.ts
```

Every remaining import of `$lib/llm-config.svelte` is removed in Tasks 7 and 9; until then `npm run check` reports those as errors. Do not commit a half-migrated state: run Tasks 7 to 9 before the final `npm run check`, and commit this task together with the imports removed in Task 7 (see Task 7 Step 9).

- [ ] **Step 9: Add the i18n keys**

Append to the `TranslationKey` union in `web/src/lib/i18n/types.ts`:

```ts
	| 'navSettings'
	| 'settingsTitle'
	| 'settingsIntro'
	| 'settingsAiTitle'
	| 'settingsAiIntro'
	| 'settingsBringTitle'
	| 'settingsBringIntro'
	| 'settingsBringEmailLabel'
	| 'settingsBringPasswordLabel'
	| 'settingsApiKeyLabel'
	| 'settingsApiKeyPlaceholder'
	| 'settingsSecretStored'
	| 'settingsSecretInherited'
	| 'settingsSecretAbsent'
	| 'settingsSecretClear'
	| 'settingsSecretReplacePlaceholder'
	| 'settingsSaving'
	| 'settingsSaved'
	| 'settingsSaveFailed'
	| 'settingsBringChecking'
	| 'settingsBringConnected'
	| 'settingsSecurityNote'
	| 'settingsLinkLabel'
	| 'aiConfigNotice'
	| 'llmErrorNotConfigured';
```

Append to `web/src/lib/i18n/en.ts`:

```ts
	navSettings: 'Settings',
	settingsTitle: 'Settings',
	settingsIntro: 'Configure the AI provider and your Bring! account once, here.',
	settingsAiTitle: 'AI',
	settingsAiIntro: 'Used by AI recipe import, meal generation and instruction polish.',
	settingsBringTitle: 'Bring!',
	settingsBringIntro: 'Used to send planner ingredients to your shopping list.',
	settingsBringEmailLabel: 'Bring! email',
	settingsBringPasswordLabel: 'Bring! password',
	settingsApiKeyLabel: 'API key',
	settingsApiKeyPlaceholder: 'Paste the API key',
	settingsSecretStored: 'Stored in settings',
	settingsSecretInherited: 'Inherited from the environment',
	settingsSecretAbsent: 'Not set',
	settingsSecretClear: 'Clear',
	settingsSecretReplacePlaceholder: 'Enter a new value to replace the stored one',
	settingsSaving: 'Saving…',
	settingsSaved: 'Saved',
	settingsSaveFailed: 'Could not save',
	settingsBringChecking: 'Checking the connection…',
	settingsBringConnected: 'Bring! connection works',
	settingsSecurityNote: 'Secrets are stored in plaintext in the YummyBox database. Protect the data directory and do not expose this app to the internet.',
	settingsLinkLabel: 'Open settings',
	aiConfigNotice: 'No AI provider is configured yet.',
	llmErrorNotConfigured: 'AI is not configured. Choose a provider and a model in the settings.',
```

Append the matching German strings to `web/src/lib/i18n/de.ts`:

```ts
	navSettings: 'Einstellungen',
	settingsTitle: 'Einstellungen',
	settingsIntro: 'KI-Anbieter und Bring!-Konto einmalig hier einrichten.',
	settingsAiTitle: 'KI',
	settingsAiIntro: 'Wird für KI-Rezeptimport, Rezept-Generierung und Textüberarbeitung verwendet.',
	settingsBringTitle: 'Bring!',
	settingsBringIntro: 'Wird verwendet, um Zutaten aus dem Planer an die Einkaufsliste zu senden.',
	settingsBringEmailLabel: 'Bring!-E-Mail',
	settingsBringPasswordLabel: 'Bring!-Passwort',
	settingsApiKeyLabel: 'API-Schlüssel',
	settingsApiKeyPlaceholder: 'API-Schlüssel einfügen',
	settingsSecretStored: 'In den Einstellungen gespeichert',
	settingsSecretInherited: 'Aus der Umgebung übernommen',
	settingsSecretAbsent: 'Nicht gesetzt',
	settingsSecretClear: 'Löschen',
	settingsSecretReplacePlaceholder: 'Neuen Wert eingeben, um den gespeicherten zu ersetzen',
	settingsSaving: 'Wird gespeichert…',
	settingsSaved: 'Gespeichert',
	settingsSaveFailed: 'Speichern fehlgeschlagen',
	settingsBringChecking: 'Verbindung wird geprüft…',
	settingsBringConnected: 'Bring!-Verbindung funktioniert',
	settingsSecurityNote: 'Geheimnisse werden im Klartext in der YummyBox-Datenbank gespeichert. Das Datenverzeichnis schützen und die App nicht im Internet betreiben.',
	settingsLinkLabel: 'Einstellungen öffnen',
	aiConfigNotice: 'Es ist noch kein KI-Anbieter konfiguriert.',
	llmErrorNotConfigured: 'KI ist nicht konfiguriert. Anbieter und Modell in den Einstellungen wählen.',
```

- [ ] **Step 10: Run the i18n parity test**

Run: `cd web && npm test -- -t "dictionary"`
Expected: PASS (identical key sets in `en.ts` and `de.ts`).

- [ ] **Step 11: Add the gear icon**

In `web/src/lib/Icon.svelte`, extend the `IconName` union with `| 'settings'` and insert before the closing `{/if}` (after the `image-down` branch):

```svelte
{:else if name === 'settings'}
	<svg xmlns="http://www.w3.org/2000/svg" width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class={iconClass} aria-hidden="true">
		<path d="M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.1a2 2 0 0 1 1 1.72v.51a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.39a2 2 0 0 0-.73-2.73l-.15-.08a2 2 0 0 1-1-1.74v-.5a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.38a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2z" />
		<circle cx="12" cy="12" r="3" />
	</svg>
```

- [ ] **Step 12: Run the frontend tests and type check for the new modules only**

Run: `cd web && npm test -- -t "settings" && npx svelte-check --threshold error 2>&1 | tail -20`
Expected: the settings module and API tests pass. Type errors for the still-unmigrated `llm-config` imports are expected and are resolved in Tasks 7 and 9.

- [ ] **Step 13: Do not commit yet**

Tasks 6, 7 and 9 plus Task 8 Step 1 are one atomic unit: the TypeScript and Svelte type graph makes every intermediate state uncompilable (the picker and the API client drop parameters and props the pages still pass), so the unit is committed once, at the end of Task 9.

---

## Task 7: The AI picker is settings-backed

**Files:**
- Modify: `web/src/lib/components/LlmConfigPicker.svelte` (rewrite the script, adjust the template's API-key field)
- Modify: `tests/e2e/generate-meal.spec.ts` (`configureMockProvider`)
- Modify: `tests/e2e/_helpers.ts` (add `resetSettings`)

**Interfaces:**
- Consumes: `getSettings`, `updateSettings`, `listLlmProviders`, `listLlmModels`, `settings.svelte.ts`.
- Produces: `LlmConfigPicker` props `{ provider, providerName, model, disabled, providersReady, configured }` — all bindable except `disabled`; `customBaseUrl`/`customApiKey`/`autorestore`/`onrestored` are gone.

- [ ] **Step 1: Add the settings reset helper and call it from the AI specs**

Append to `tests/e2e/_helpers.ts`:

```ts
export async function resetSettings(request: APIRequestContext): Promise<void> {
	const res = await request.patch('/api/settings', {
		data: {
			ai: { provider: null, model: null, customBaseUrl: null, apiKey: null },
			bring: { email: null, password: null },
		},
	});
	expect(res.ok()).toBe(true);
}
```

Stored settings now outlive a browser context (they live in the workflow suite's shared `.e2e-db`), so every spec that drives an AI flow must clear them per test — otherwise a provider, model or key committed by an earlier test makes the next test's "not configured" assertions fail. Add `resetSettings` to the import list of `tests/e2e/generate-meal.spec.ts`, `tests/e2e/import-llm.spec.ts` and `tests/e2e/llm-image.spec.ts`, and call `await resetSettings(request);` in each of their `beforeEach` blocks next to the existing `resetMeals(request)` call.

- [ ] **Step 2: Update the mock-provider helper**

In `tests/e2e/generate-meal.spec.ts`, replace `configureMockProvider` (the settings now commit on blur, and the model list is loaded from the stored configuration):

```ts
async function configureMockProvider(page: import('@playwright/test').Page) {
	// Provider select is the first select in the picker.
	await page.locator('select').first().selectOption('custom');
	await page.getByLabel('Base URL').fill('http://127.0.0.1:18999/v1/');
	// genai's OpenAI adapter requires a key value even for keyless endpoints;
	// the mock ignores the Authorization header.
	await page.getByLabel('API Key (optional)').fill('mock-key');
	// Blurring commits the base URL and the key, which is what triggers the
	// model list request against the now stored configuration.
	await page.getByLabel('API Key (optional)').blur();
	await expect(page.locator('select').nth(1)).toBeVisible({ timeout: 10_000 });
	await page.locator('select').nth(1).selectOption('mock-model');
}
```

- [ ] **Step 3: Write the picker script**

Replace the `<script>` block of `web/src/lib/components/LlmConfigPicker.svelte` entirely:

```svelte
<script lang="ts">
	import { listLlmProviders, listLlmModels, getSettings, updateSettings, ApiError } from '$lib/api';
	import {
		SettingsCommitter,
		isAiConfigured,
		commitStatusLabelKey,
		secretSourceLabelKey,
		type SecretState,
		type SettingsPatch,
	} from '$lib/settings.svelte';
	import { t } from '$lib/i18n';
	import type { LlmProviderInfo } from '$lib/types';

	let {
		provider = $bindable(''),
		providerName = $bindable(''),
		model = $bindable(''),
		disabled = false,
		providersReady = $bindable(true),
		configured = $bindable(false),
	}: {
		provider?: string;
		providerName?: string;
		model?: string;
		disabled?: boolean;
		providersReady?: boolean;
		configured?: boolean;
	} = $props();

	let llmProviders = $state<LlmProviderInfo[]>([]);
	let llmProvidersLoading = $state(false);
	let llmProvidersLoaded = $state(false);
	let llmModels: string[] = $state([]);
	let llmModelsLoading = $state(false);
	let llmModelsError = $state<string | null>(null);
	let customBaseUrl = $state('');
	let apiKeyInput = $state('');
	let apiKeyState = $state<SecretState>({ set: false, source: 'none' });
	let restored = $state(false);
	// Provider whose models were already loaded this mount; ensures a fresh
	// model load after the component remounts (collapse/expand, tab switch).
	let modelsLoadedFor: string | null = null;
	// Monotonic sequence for model-list requests: a slow earlier response must
	// not overwrite the models of a newer provider switch.
	let modelsRequestSeq = 0;

	// Commits are serialized: a settings change may only be followed by the
	// next one once the previous request has been answered, so a slow
	// provider commit can never land after a newer model commit.
	const committer = new SettingsCommitter(updateSettings, (snapshot) => {
		apiKeyState = snapshot.ai.apiKey;
		configured = isAiConfigured(snapshot);
	});

	function commit(patch: SettingsPatch): Promise<void> {
		return committer.commit(patch);
	}

	async function loadModels() {
		const seq = ++modelsRequestSeq;
		if (!provider) {
			llmModelsLoading = false;
			llmModelsError = null;
			return;
		}
		if (provider === 'custom' && !customBaseUrl.trim()) {
			llmModels = [];
			llmModelsLoading = false;
			llmModelsError = null;
			return;
		}
		llmModelsLoading = true;
		llmModelsError = null;
		try {
			const resp = await listLlmModels(provider);
			if (seq !== modelsRequestSeq) return;
			llmModels = resp.models;
			if (model && !resp.models.includes(model)) {
				llmModelsError = t('llmModelsLoadError');
			}
		} catch (err) {
			if (seq !== modelsRequestSeq) return;
			llmModels = [];
			llmModelsError = err instanceof ApiError
				? (err.code === 'REQUEST_FAILED' ? t('llmModelsLoadError') : `${t('llmModelsLoadError')} (${err.message})`)
				: t('llmModelsLoadError');
		} finally {
			if (seq === modelsRequestSeq) {
				llmModelsLoading = false;
			}
		}
	}

	function onProviderChange() {
		// Invalidate any in-flight model-list request: a stale response must
		// not repopulate the model select after a provider switch.
		modelsRequestSeq++;
		model = '';
		customBaseUrl = '';
		apiKeyInput = '';
		llmModels = [];
		llmModelsError = null;
		providerName = llmProviders.find((p) => p.id === provider)?.name ?? provider;
		// A provider switch invalidates the previous provider's model,
		// endpoint and key: clear them in the same commit.
		commit({
			ai: { provider, model: null, customBaseUrl: null, apiKey: null },
		}).then(() => {
			modelsLoadedFor = provider;
			if (provider && provider !== 'custom') loadModels();
		});
	}

	function onModelChange() {
		commit({ ai: { model: model.trim() ? model : null } });
	}

	function onBaseUrlChange() {
		commit({ ai: { customBaseUrl: customBaseUrl.trim() ? customBaseUrl : null } }).then(() => {
			if (provider === 'custom' && customBaseUrl.trim()) loadModels();
		});
	}

	function onApiKeyChange() {
		const value = apiKeyInput;
		if (!value.trim()) return;
		commit({ ai: { apiKey: value } }).then(() => {
			// The value is stored now; never keep it in the DOM. A value typed
			// while the request was in flight stays untouched.
			if (apiKeyInput === value) apiKeyInput = '';
			if (provider === 'custom' && customBaseUrl.trim()) loadModels();
		});
	}

	function onClearApiKey() {
		apiKeyInput = '';
		commit({ ai: { apiKey: null } });
	}

	function onEnter(event: KeyboardEvent, commitField: () => void) {
		if (event.key !== 'Enter') return;
		event.preventDefault();
		commitField();
	}

	// Re-run the providers load after a failure: resetting llmProvidersLoaded
	// re-triggers the load effect below (the catch keeps the loop from
	// retrying on its own, so this is strictly user-initiated).
	function retryLoadProviders() {
		llmProvidersLoaded = false;
	}

	// Load providers once, then reconcile the stored provider.
	$effect(() => {
		if (!llmProvidersLoaded && !llmProvidersLoading) {
			llmProvidersLoading = true;
			providersReady = true;
			listLlmProviders()
				.then((p) => {
					llmProviders = p;
					llmProvidersLoaded = true;
					llmProvidersLoading = false;
					providersReady = p.length > 0;
					providerName = p.find((pp) => pp.id === provider)?.name ?? provider;
					if (provider && !p.some((pp) => pp.id === provider)) {
						provider = '';
						model = '';
					}
				})
				.catch(() => {
					llmProvidersLoaded = true;
					llmProvidersLoading = false;
					providersReady = false;
				});
		}
	});

	// Load the stored configuration once per mount; never overwrite user edits.
	$effect(() => {
		if (restored) return;
		restored = true;
		getSettings()
			.then((snapshot) => {
				provider = snapshot.ai.provider;
				model = snapshot.ai.model;
				customBaseUrl = snapshot.ai.customBaseUrl;
				apiKeyState = snapshot.ai.apiKey;
				configured = isAiConfigured(snapshot);
				providerName = llmProviders.find((p) => p.id === provider)?.name ?? provider;
				// Standard providers list their models straight away; the custom
				// provider waits for a base URL to be stored.
				if (provider && provider !== 'custom') {
					modelsLoadedFor = provider;
					loadModels();
				}
			})
			.catch(() => {
				// A settings read failure leaves the picker empty; the settings
				// page surfaces the error again on its own load.
			});
	});

	// Reload models when the picker remounts with a provider already selected.
	$effect(() => {
		if (provider && provider !== 'custom' && modelsLoadedFor !== provider) {
			modelsLoadedFor = provider;
			loadModels();
		}
	});
</script>
```

- [ ] **Step 4: Update the picker template**

In the same file, replace the model row's `select`/`input` handlers and the custom-provider block:

```svelte
			{#if provider}
				{#if llmModelsLoading}
					<span class="import-loading">{t('llmModelLoading')}</span>
				{:else if llmModelsError}
					<input type="text" bind:value={model} placeholder={t('importLlmModelPlaceholder')}
						disabled={disabled} onchange={onModelChange}
						onkeydown={(e) => onEnter(e, onModelChange)} />
				{:else}
					<select bind:value={model} aria-label={t('llmModelLabel')} disabled={disabled}
						onchange={onModelChange}>
						<option value="">{t('llmModelPlaceholder')}</option>
						{#each llmModels as m}
							<option value={m}>{m}</option>
						{/each}
					</select>
				{/if}
			{/if}
		</div>

		{#if provider === 'custom'}
			<p class="import-info">{t('llmCustomHint')}</p>
			<label class="import-field">
				<span>{t('llmCustomBaseUrlLabel')}</span>
				<input type="url" bind:value={customBaseUrl} placeholder={t('llmCustomBaseUrlPlaceholder')}
					disabled={disabled} onchange={onBaseUrlChange}
					onkeydown={(e) => onEnter(e, onBaseUrlChange)} />
			</label>
		{/if}

		{#if provider}
			<label class="import-field">
				<span>{provider === 'custom' ? t('llmCustomApiKeyLabel') : t('settingsApiKeyLabel')}</span>
				<input type="password" bind:value={apiKeyInput}
					placeholder={provider === 'custom' ? t('llmCustomApiKeyPlaceholder') : t('settingsApiKeyPlaceholder')}
					disabled={disabled} onchange={onApiKeyChange}
					onkeydown={(e) => onEnter(e, onApiKeyChange)} />
			</label>
			<p class="llm-secret-state">
				{t(secretSourceLabelKey(apiKeyState.source))}
				{#if apiKeyState.source === 'settings'}
					<button type="button" class="btn btn--ghost" onclick={onClearApiKey} disabled={disabled}>
						{t('settingsSecretClear')}
					</button>
				{/if}
			</p>
		{/if}

		{#if llmModelsError}
			<p class="form-error">{llmModelsError}</p>
		{/if}
		{#if provider === 'ollama' && llmModelsError}
			<p class="import-info">{t('llmOllamaHint')}</p>
		{/if}

		<p class="llm-commit-state" role="status">
			{commitStatusLabelKey(committer.state.status) ? t(commitStatusLabelKey(committer.state.status)!) : ''}
		</p>
		{#if committer.state.status === 'error'}
			<p class="form-error" role="alert">{committer.state.error}</p>
		{/if}
```

Add the two small style rules at the end of the component's `<style>` block:

```css
	.llm-secret-state {
		display: flex;
		align-items: center;
		gap: var(--space-2);
		font-size: var(--text-sm);
		color: var(--color-text-secondary);
	}
	.llm-commit-state {
		min-height: 1.2em;
		font-size: var(--text-sm);
		color: var(--color-text-muted);
	}
```

- [ ] **Step 5: Run the frontend tests and type check**

Run: `cd web && npm test && npm run check`
Expected: PASS, except for the still-unmigrated call sites in `spontaneous/+page.svelte` and `meals/+page.svelte`, which pass the removed props `customBaseUrl`, `customApiKey` and `onrestored`. Those are fixed in Task 9.

- [ ] **Step 6: Do not commit yet**

The atomic unit is Tasks 6, 7 and 9 plus Task 8 Step 1; it is committed at the end of Task 9. Committing here would leave `npm run check` failing on the pages that still pass the removed props and still import `llm-config.svelte`.

---

## Task 8: The settings page, the gear control and the flow notice

**Files:**
- Create: `web/src/routes/settings/+page.svelte`
- Create: `web/src/lib/components/AiConfigNotice.svelte`
- Modify: `web/src/routes/+layout.svelte` (gear control in `.app-bar__actions`)
- Modify: `web/src/app.css` (`.app-bar__settings` sharing the theme-control styles)

**Interfaces:**
- Consumes: `getSettings`, `updateSettings`, `checkBringStatus`, `SettingsCommitter`, `secretSourceLabelKey`, `commitStatusLabelKey`, `LlmConfigPicker`, `SettingsPatch`.
- Produces: the `/settings` route; `AiConfigNotice.svelte`.
- Note: Step 1 (the notice component) belongs to the preceding unit — the flow gates in Task 9 import it — and is committed with Task 9. This task's own commit covers Steps 2 to 4.

- [ ] **Step 1: Add the shared notice component**

Create `web/src/lib/components/AiConfigNotice.svelte`:

```svelte
<script lang="ts">
	import { t } from '$lib/i18n';
</script>

<p class="ai-config-notice" role="status">
	{t('aiConfigNotice')}
	<a href="/settings">{t('settingsLinkLabel')}</a>
</p>

<style>
	.ai-config-notice {
		display: flex;
		flex-wrap: wrap;
		gap: var(--space-2);
		align-items: baseline;
		padding: var(--space-3) var(--space-4);
		background: var(--color-surface-2);
		border: 1px solid var(--color-border);
		border-radius: var(--radius-md);
		font-size: var(--text-sm);
		color: var(--color-text-secondary);
	}
	.ai-config-notice a {
		color: var(--color-primary);
		font-weight: var(--weight-medium);
	}
</style>
```

- [ ] **Step 2: Write the settings page**

Create `web/src/routes/settings/+page.svelte`:

```svelte
<script lang="ts">
	import LlmConfigPicker from '$lib/components/LlmConfigPicker.svelte';
	import { checkBringStatus, updateSettings } from '$lib/api';
	import {
		SettingsCommitter,
		commitStatusLabelKey,
		secretSourceLabelKey,
		type SecretState,
		type ValueSource,
	} from '$lib/settings.svelte';
	import { t } from '$lib/i18n';

	let provider = $state('');
	let providerName = $state('');
	let model = $state('');
	let providersReady = $state(true);
	let aiConfigured = $state(false);

	let bringEmail = $state('');
	let bringEmailSource = $state<ValueSource>('none');
	let bringPasswordInput = $state('');
	let bringPasswordState = $state<SecretState>({ set: false, source: 'none' });
	let bringStatus = $state<'idle' | 'checking' | 'connected' | 'error'>('idle');
	let bringStatusError = $state<string | null>(null);

	// Commits are serialized, exactly like in the AI picker: the newest value
	// always wins, regardless of how long an earlier request takes.
	const committer = new SettingsCommitter(updateSettings, (snapshot) => {
		bringEmailSource = snapshot.bring.emailSource;
		bringPasswordState = snapshot.bring.password;
		// Only refill the email when the field is empty: that is the case after
		// a clear, where the environment value becomes effective again. A value
		// the user is typing is never overwritten by an older response.
		if (!bringEmail.trim()) bringEmail = snapshot.bring.email;
	});

	async function refreshBringStatus() {
		bringStatus = 'checking';
		bringStatusError = null;
		try {
			const res = await checkBringStatus();
			if (!res.configured) {
				bringStatus = 'idle';
			} else if (res.connected) {
				bringStatus = 'connected';
			} else {
				bringStatus = 'error';
				bringStatusError = res.error;
			}
		} catch (err) {
			bringStatus = 'error';
			bringStatusError = err instanceof Error ? err.message : String(err);
		}
	}

	function onBringEmailChange() {
		committer
			.commit({ bring: { email: bringEmail.trim() ? bringEmail : null } })
			.then(refreshBringStatus);
	}

	function onBringPasswordChange() {
		const value = bringPasswordInput;
		if (!value) return;
		committer.commit({ bring: { password: value } }).then(() => {
			if (bringPasswordInput === value) bringPasswordInput = '';
			refreshBringStatus();
		});
	}

	function onClearBringPassword() {
		bringPasswordInput = '';
		committer.commit({ bring: { password: null } }).then(refreshBringStatus);
	}

	function onEnter(event: KeyboardEvent, commitField: () => void) {
		if (event.key !== 'Enter') return;
		event.preventDefault();
		commitField();
	}

	$effect(() => {
		refreshBringStatus();
	});
</script>

<main class="settings-page">
	<header class="settings-hero glass">
		<h1 class="settings-hero__title">{t('settingsTitle')}</h1>
		<p class="settings-hero__sub">{t('settingsIntro')}</p>
	</header>

	<section class="settings-card glass">
		<h2 class="settings-card__title">{t('settingsAiTitle')}</h2>
		<p class="settings-card__intro">{t('settingsAiIntro')}</p>
		<LlmConfigPicker bind:provider bind:providerName bind:model bind:providersReady bind:configured={aiConfigured} />
	</section>

	<section class="settings-card glass">
		<h2 class="settings-card__title">{t('settingsBringTitle')}</h2>
		<p class="settings-card__intro">{t('settingsBringIntro')}</p>

		<label class="import-field">
			<span>{t('settingsBringEmailLabel')}</span>
			<input type="email" bind:value={bringEmail} onchange={onBringEmailChange}
				onkeydown={(e) => onEnter(e, onBringEmailChange)} />
		</label>
		<p class="settings-state">{t(secretSourceLabelKey(bringEmailSource))}</p>

		<label class="import-field">
			<span>{t('settingsBringPasswordLabel')}</span>
			<input type="password" bind:value={bringPasswordInput}
				placeholder={t('settingsSecretReplacePlaceholder')}
				onchange={onBringPasswordChange}
				onkeydown={(e) => onEnter(e, onBringPasswordChange)} />
		</label>
		<p class="settings-state">
			{t(secretSourceLabelKey(bringPasswordState.source))}
			{#if bringPasswordState.source === 'settings'}
				<button type="button" class="btn btn--ghost" onclick={onClearBringPassword}>
					{t('settingsSecretClear')}
				</button>
			{/if}
		</p>

		<p class="settings-state settings-state--status" role="status">
			{commitStatusLabelKey(committer.state.status) ? t(commitStatusLabelKey(committer.state.status)!) : ''}
		</p>
		{#if committer.state.status === 'error'}
			<p class="form-error" role="alert">{committer.state.error}</p>
		{/if}

		{#if bringStatus === 'checking'}
			<p class="settings-state" role="status">{t('settingsBringChecking')}</p>
		{:else if bringStatus === 'connected'}
			<p class="settings-state" role="status">{t('settingsBringConnected')}</p>
		{/if}
		{#if bringStatusError}
			<p class="form-error" role="alert">{bringStatusError}</p>
		{/if}
	</section>

	<p class="settings-note">{t('settingsSecurityNote')}</p>
</main>

<style>
	.settings-page {
		display: flex;
		flex-direction: column;
		gap: var(--space-4);
		padding: var(--space-4) var(--space-3) var(--space-6);
	}
	.settings-hero {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
		padding: var(--space-5) var(--space-4);
	}
	.settings-hero__title {
		margin: 0;
		font-family: var(--font-display);
		font-size: var(--text-2xl);
		color: var(--color-primary);
	}
	.settings-hero__sub {
		margin: 0;
		color: var(--color-text-secondary);
	}
	.settings-card {
		display: flex;
		flex-direction: column;
		gap: var(--space-3);
		padding: var(--space-4);
	}
	.settings-card__title {
		margin: 0;
		font-family: var(--font-display);
		font-size: var(--text-lg);
	}
	.settings-card__intro {
		margin: 0;
		font-size: var(--text-sm);
		color: var(--color-text-secondary);
	}
	.settings-state {
		display: flex;
		align-items: center;
		gap: var(--space-2);
		margin: 0;
		font-size: var(--text-sm);
		color: var(--color-text-muted);
	}
	.settings-state--status {
		min-height: 1.2em;
	}
	.settings-note {
		margin: 0;
		font-size: var(--text-sm);
		color: var(--color-text-muted);
	}
</style>
```

Every design token used above exists in `web/src/app.css` (`--text-2xl`, `--text-lg`, `--font-display`, `--space-*`, `--radius-md`, `--color-primary`, `--color-text-secondary`, `--color-text-muted`, `--color-surface-2`, `--color-border`, and the shared `.glass`, `.import-field`, `.form-error` and `.btn--ghost` classes) — verified against the stylesheet; no substitution needed.

- [ ] **Step 3: Add the gear control**

In `web/src/routes/+layout.svelte`, insert as the first child of `.app-bar__actions`:

```svelte
		<a href="/settings" class="app-bar__settings"
			aria-label={t('navSettings')}
			title={t('navSettings')}
			aria-current={pathname.startsWith('/settings') ? 'page' : undefined}>
			<Icon name="settings" size={16} />
		</a>
```

- [ ] **Step 4: Style the gear control**

In `web/src/app.css`, extend the four `.app-bar__theme` rules (lines around 759-779) to cover the new control, adding `text-decoration: none;` to the shared block:

```css
.app-bar__theme,
.app-bar__settings {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 32px;
  height: 32px;
  padding: 0;
  border: none;
  background: transparent;
  color: var(--color-text-secondary);
  border-radius: var(--radius-full);
  cursor: pointer;
  text-decoration: none;
  transition: color var(--transition-fast), background var(--transition-fast);
}
.app-bar__theme:hover,
.app-bar__settings:hover {
  color: var(--color-primary);
  background: var(--glass-bg-strong);
}
.app-bar__theme:focus-visible,
.app-bar__settings:focus-visible {
  outline: 2px solid var(--color-primary);
  outline-offset: 2px;
}
```

- [ ] **Step 5: Verify in the browser**

Run: `cd web && npm run dev` and open `http://localhost:5173/settings` (the dev server proxies `/api` only when the backend runs; start the backend with `cargo run` in a second shell so `/api/settings` answers).
Expected: the settings page renders both sections, the gear appears beside the theme toggle and the language switcher, and the theme toggle and language switcher keep working.

- [ ] **Step 6: Commit this task's steps 2 to 4**

Step 1 (the notice component) belongs to the previous unit and is committed with it. This commit covers the settings page, the gear control and the styles:

```bash
git add web/src/routes/settings web/src/routes/+layout.svelte web/src/app.css
git commit -m "feat(settings): add the settings page and the top-bar gear control"
```

---

## Task 9: Flows gate on the stored configuration

**Files:**
- Modify: `web/src/routes/meals/+page.svelte` (replace the inline picker with `LlmConfigPicker`, drop the `llm-config` import, use the gate)
- Modify: `web/src/routes/spontaneous/+page.svelte` (new picker props, gate from `configured`)
- Modify: `web/src/routes/meals/[id]/+page.svelte` (gate from the settings API)
- Modify: `web/src/lib/llm-error.ts` (`llm_not_configured`)

**Interfaces:**
- Consumes: `getSettings`, `isAiConfigured`, `importFromLlm(hint, images)`, `generateMeal(ingredients, images)`, `polishInstructions(name, ingredients, instructions)`, `AiConfigNotice`.
- Produces: no new exports.

- [ ] **Step 1: Map the not-configured error**

In `web/src/lib/llm-error.ts`, add before the generic branch:

```ts
		if (err.code === 'llm_not_configured') return t('llmErrorNotConfigured');
```

- [ ] **Step 2: Gate the meal detail page**

In `web/src/routes/meals/[id]/+page.svelte`:

1. Replace the import `import { readStoredLlmConfig } from '$lib/llm-config.svelte';` with `import { getSettings } from '$lib/api';` (merge into the existing `$lib/api` import) and `import { isAiConfigured } from '$lib/settings.svelte';`.
2. Replace the `hasLlmConfig` derivation and `doPolish`'s config lookup:

```ts
	let settings = $state<import('$lib/settings.svelte').SettingsSnapshot | null>(null);
	let hasLlmConfig = $derived(isAiConfigured(settings));

	$effect(() => {
		let cancelled = false;
		getSettings()
			.then((snapshot) => { if (!cancelled) settings = snapshot; })
			.catch(() => { if (!cancelled) settings = null; });
		return () => { cancelled = true; };
	});
```

```ts
	async function doPolish() {
		if (!meal || polishing || !hasLlmConfig) return;
		polishing = true;
		polishError = null;
		try {
			const polished = await polishInstructions(meal.name, meal.ingredients, meal.instructions);
			await updateMeal(meal.id, {
				name: meal.name,
				ingredients: meal.ingredients,
				instructions: polished,
				portions: meal.portions ?? null,
				source_url: meal.source_url ?? null,
			});
			await loadMeal();
		} catch (err) {
			if (err instanceof ApiError) {
				if (err.code === 'llm_not_configured') polishError = t('llmErrorNotConfigured');
				else if (err.code === 'llm_timeout') polishError = t('llmErrorTimeout');
				else if (err.code === 'llm_parse_failed') polishError = t('llmErrorParseFailed');
				else if (err.code === 'llm_api_key_missing') polishError = t('llmErrorApiKey', { envVar: '' });
				else polishError = t('polishErrorFailed');
			} else {
				polishError = t('polishErrorFailed');
			}
		} finally {
			polishing = false;
		}
	}
```

3. In the markup, render `<AiConfigNotice />` (imported from `$lib/components/AiConfigNotice.svelte`) next to the polish button whenever `!hasLlmConfig`.

- [ ] **Step 3: Gate the generate page**

In `web/src/routes/spontaneous/+page.svelte`:

1. Remove `import { persistLlmConfig } from '$lib/llm-config.svelte';`; remove the `customBaseUrl` and `customApiKey` state and every `bind:customBaseUrl`/`bind:customApiKey`/`onrestored` usage.
2. Add `let configured = $state(false);` and change the generate gate to `let canGenerate = $derived(configured && hasInput && !generating);`.
3. Change `generateMeal(...)` to the two-argument form:

```ts
			const d = await generateMeal(ingredients, images);
```

4. Delete the `persistLlmConfig({...})` call in `onGenerate`.
5. Replace the picker usage with:

```svelte
			<LlmConfigPicker
				bind:provider
				bind:providerName
				bind:model
				bind:providersReady
				bind:configured
				disabled={generating}
			/>
```

6. Collapse the settings block once a stored configuration is loaded:

```ts
	// Collapse the AI settings block once the stored configuration is usable,
	// so the ingredients input is the focus of the page.
	let collapsedOnce = false;
	$effect(() => {
		if (configured && !collapsedOnce) {
			collapsedOnce = true;
			settingsCollapsed = true;
		}
	});
```

7. Render `<AiConfigNotice />` inside the `.spontan-config` section when `!configured`.

- [ ] **Step 4: Migrate the add-meal dialog to the shared picker**

In `web/src/routes/meals/+page.svelte`:

1. Remove `import { readStoredLlmConfig, persistLlmConfig } from '$lib/llm-config.svelte';` and the LLM state that the picker now owns: `importLlmProvider`, `importLlmModel`, `importLlmCustomBaseUrl`, `importLlmCustomApiKey`, `llmConfigRestored`, `llmProviders`, `llmProvidersLoading`, `llmProvidersLoaded`, `llmModels`, `llmModelsLoading`, `llmModelsError`, and the `loadLlmModels`, `onProviderChange`, the restore `$effect`, the providers `$effect` and the debounce `$effect`, plus `_customDebounceTimer`.
2. Keep `llmSettingsCollapsed` and add:

```ts
    let importLlmProvider = $state('');
    let importLlmProviderName = $state('');
    let importLlmModel = $state('');
    let importLlmProvidersReady = $state(true);
    let importLlmConfigured = $state(false);
```

3. Replace the whole `{#if !llmSettingsCollapsed || !importLlmProvider} <div class="import-subsection"> ... </div> {/if}` block with the shared component:

```svelte
									{#if !llmSettingsCollapsed || !importLlmProvider}
										<div class="import-subsection">
											<LlmConfigPicker
												bind:provider={importLlmProvider}
												bind:providerName={importLlmProviderName}
												bind:model={importLlmModel}
												bind:providersReady={importLlmProvidersReady}
												bind:configured={importLlmConfigured}
												disabled={importing}
											/>
										</div>
									{/if}
```

and add `import LlmConfigPicker from '$lib/components/LlmConfigPicker.svelte';` plus `import AiConfigNotice from '$lib/components/AiConfigNotice.svelte';`.

4. The `{#if llmProviders.length === 0 && !llmProvidersLoading}` "no providers" branch is now inside the picker; delete it from the page along with the collapsed summary's `llmProviders.find(...)` lookup, replacing it with `{importLlmProviderName}`.
5. Change the LLM import call and drop the persistence:

```ts
            const draft = await importFromLlm(importLlmHint || null, importLlmImages);
```

6. Change the parse-button gate from `!importLlmModel.trim()` to `!importLlmConfigured`, and render `<AiConfigNotice />` above the picker when `!importLlmConfigured`.
7. In `openAdd()`, reset the new state (`importLlmProvider = ''; importLlmProviderName = ''; importLlmModel = ''; importLlmConfigured = false;`) and drop the removed variables.

- [ ] **Step 5: Run the frontend checks**

Run: `cd web && npm test && npm run check && npx prettier --check src 2>/dev/null || true`
Expected: `npm test` PASS, `npm run check` 0 errors and no new warnings, and no reference to `llm-config` anywhere:

```bash
grep -rn "llm-config" web/src || echo "no legacy llm-config references"
```

- [ ] **Step 6: Run the workflow E2E suite**

Run: `cd tests && npx playwright test e2e/generate-meal.spec.ts e2e/import-llm.spec.ts e2e/llm-image.spec.ts`
Expected: PASS. If `configureMockProvider` times out waiting for the model select, check that the API-key input is blurred (that is what commits it) and that `/api/settings` returns the stored base URL and key.

- [ ] **Step 7: Commit the atomic frontend unit**

This commit carries Tasks 6, 7 and 9 plus Task 8 Step 1 — the API client, the settings module, the i18n keys, the icon, the settings-backed picker, the notice component, the three migrated call sites, and the removal of the legacy localStorage module:

```bash
git add web/src/lib web/src/routes web/src/lib/i18n web/src/lib/components tests/e2e/_helpers.ts tests/e2e/generate-meal.spec.ts
git rm web/src/lib/llm-config.svelte.ts web/src/lib/llm-config.test.ts
git commit -m "feat(settings): back every AI flow with the stored server-side configuration"
```

---

## Task 10: E2E coverage and documentation

**Files:**
- Create: `tests/e2e/settings.spec.ts`
- Modify: `tests/e2e/generate-meal.spec.ts`, `tests/e2e/import-llm.spec.ts`, `tests/e2e/llm-image.spec.ts` (reset stored settings per test)
- Modify: `README.md` (Configuration section)
- Modify: `.env.example` (point at the settings page)
- Modify: `AGENTS.md` (file tables)

**Interfaces:**
- Consumes: `resetSettings`, `setLocale`, `resetMeals` from `./_helpers`.
- Produces: `tests/e2e/settings.spec.ts`.

- [ ] **Step 1: Write the E2E spec**

Create `tests/e2e/settings.spec.ts`:

```ts
import { test, expect } from '@playwright/test';
import { resetMeals, resetSettings, setLocale } from './_helpers';

test.describe('Settings page', () => {
	test.beforeEach(async ({ request, page }) => {
		await setLocale(page, 'en');
		await resetMeals(request);
		await resetSettings(request);
	});

	test('given_app_bar_when_gear_clicked_then_settings_page_opens', async ({ page }) => {
		await page.goto('/meals');
		await page.getByRole('link', { name: 'Settings' }).click();
		await expect(page).toHaveURL(/\/settings$/);
		await expect(page.getByRole('heading', { name: 'Settings' })).toBeVisible();
	});

	test('given_provider_and_model_when_committed_then_saved_state_and_persisted', async ({ page }) => {
		await page.goto('/settings');

		await page.locator('select').first().selectOption('custom');
		await page.getByLabel('Base URL').fill('http://127.0.0.1:1/v1/');
		await page.getByLabel('API Key (optional)').blur();
		await page.getByLabel('Base URL').blur();

		// The model listing against the dead endpoint fails, so the model
		// becomes a free-text field; committing it must still be reported.
		const modelInput = page.getByPlaceholder('Model name (e.g. gpt-4o-mini)');
		await expect(modelInput).toBeVisible();
		await modelInput.fill('test-model');
		await modelInput.blur();

		await expect(page.getByText('Saved')).toBeVisible();

		// Reload: the commit survives because it is stored server-side.
		await page.reload();
		await expect(page.locator('select').first()).toHaveValue('custom');
		await expect(page.getByLabel('Base URL')).toHaveValue('http://127.0.0.1:1/v1/');
	});

	test('given_stored_api_key_when_page_loads_then_state_shown_and_value_never_rendered', async ({ page, request }) => {
		const patch = await request.patch('/api/settings', {
			data: { ai: { provider: 'openai', model: 'gpt-4o-mini', apiKey: 'super-secret-key' } },
		});
		expect(patch.ok()).toBe(true);

		await page.goto('/settings');

		await expect(page.getByText('Stored in settings').first()).toBeVisible();
		await expect(page.locator('body')).not.toContainText('super-secret-key');

		const res = await request.get('/api/settings');
		expect(await res.text()).not.toContain('super-secret-key');

		// Clearing falls back to "not set" when no environment value exists.
		await page.getByRole('button', { name: 'Clear' }).first().click();
		await expect(page.getByText('Not set').first()).toBeVisible();
	});

	test('given_stored_bring_password_when_page_loads_then_value_never_rendered', async ({ page, request }) => {
		await request.patch('/api/settings', {
			data: { bring: { email: 'cook@example.com', password: 'bring-secret-pass' } },
		});

		await page.goto('/settings');

		await expect(page.getByLabel('Bring! email')).toHaveValue('cook@example.com');
		await expect(page.getByText('Stored in settings').first()).toBeVisible();
		await expect(page.locator('body')).not.toContainText('bring-secret-pass');
	});

	test('given_rejected_connection_when_credentials_committed_then_error_shown_inline', async ({ page }) => {
		await page.route('**/api/bring/status', async (route) => {
			await route.fulfill({
				status: 200,
				contentType: 'application/json',
				body: JSON.stringify({ configured: true, connected: false, error: 'Bring! login failed, check your Bring! credentials in Settings' }),
			});
		});

		await page.goto('/settings');
		const email = page.getByLabel('Bring! email');
		await email.fill('cook@example.com');
		await email.blur();
		const password = page.getByLabel('Bring! password');
		await password.fill('wrong-password');
		await password.blur();

		await expect(page.getByText('Bring! login failed')).toBeVisible();
		await expect(page.getByLabel('Bring! password')).toHaveValue('');
	});

	test('given_no_ai_configuration_when_generate_page_opened_then_notice_links_to_settings', async ({ page }) => {
		await page.goto('/spontaneous');
		const notice = page.locator('.ai-config-notice');
		await expect(notice).toBeVisible();
		await notice.getByRole('link', { name: 'Open settings' }).click();
		await expect(page).toHaveURL(/\/settings$/);
	});
});
```

- [ ] **Step 2: Confirm the AI specs reset stored settings**

`tests/e2e/generate-meal.spec.ts`, `tests/e2e/import-llm.spec.ts` and `tests/e2e/llm-image.spec.ts` must each call `await resetSettings(request);` in their `beforeEach` — that was added in Task 7, because stored settings outlive a browser context in the workflow suite's shared `.e2e-db`. Verify with `grep -n resetSettings tests/e2e/*.spec.ts` and add the call to any spec that drives an AI flow without it.

- [ ] **Step 3: Run the workflow suite**

Run: `cd tests && npx playwright test`
Expected: PASS. Note that `llm-image.spec.ts` selects the `custom` provider and relies on a *failed* model listing; the settings page commits are now persisted between tests, so `resetSettings` in `beforeEach` is mandatory.

- [ ] **Step 4: Run the visual/styling suite**

Run: `cd web && npx playwright test --config=playwright.config.ts`
Expected: PASS (6 tests). The gear control must not disturb the ambient background assertions.

- [ ] **Step 5: Document the settings page and the security boundary**

In `README.md`, under `## Configuration`, insert before `### LLM providers`:

```markdown
### Settings page

Open `/settings` (top-bar gear) to configure the AI provider and your Bring! account. Values entered there are stored in the SQLite database and take precedence over environment variables; a value cleared on the page falls back to the environment variable, or is reported as not configured when the environment provides none.

API keys and the Bring! password are never sent back to the browser: the page only shows whether a value is set and whether it comes from the settings or from the environment.

> **Security:** settings are stored in plaintext inside the database file. YummyBox has no authentication, so the database file permissions and the network exposure of the port are the security boundary. Do not expose the app to the internet.
```

Also update the two provider/Bring! tables so their rows read `Environment variable (fallback; prefer the settings page)`.

- [ ] **Step 6: Point `.env.example` at the settings page**

Extend the header comment in `.env.example`:

```bash
# YummyBox — runtime configuration
# All variables are optional. Set only what you need.
# AI provider keys and Bring! credentials can also be configured in the
# app's settings page (/settings), which takes precedence over these values.
```

and append the two Bring! variables after the provider keys:

```bash
# Bring! shopping list (fallback — the settings page takes precedence)
BRING_EMAIL=
BRING_PASSWORD=
```

- [ ] **Step 7: Keep `AGENTS.md` accurate**

In `AGENTS.md`, add to the Rust directory table: `|src/settings.rs|Stored AI and Bring! settings, effective-configuration resolution, validation|` and to the frontend table: `|web/src/routes/settings/|Settings page: AI provider/model/endpoint/key and Bring! credentials|`.

- [ ] **Step 8: Run every gate**

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
cd web && npm test && npm run check
cd ../tests && npx playwright test
```

Expected: all green.

- [ ] **Step 9: Commit**

```bash
git add tests/e2e README.md .env.example AGENTS.md
git commit -m "test(settings): cover the settings page end to end and document the security boundary"
```

---

## Self-Review Notes

- **Spec coverage:** FR-001 (Task 8), FR-002/FR-003 (Tasks 7, 8), FR-004 (Task 1), FR-005/FR-006 (Tasks 7, 8 via `SettingsCommitter`), FR-007 (Tasks 1, 2), FR-008 (Tasks 2, 7, 8), FR-009 (Tasks 1, 4, 5), FR-010 (Tasks 1, 2), FR-011 (Task 8), FR-012 (Tasks 4, 7), FR-013 (Task 4), FR-014 (Tasks 7, 9), FR-015 (Tasks 4, 8, 9), FR-016 (Task 3), FR-017 (Tasks 6, 8), FR-018 (Tasks 6, 9), FR-019 (Task 2). Success criteria SC-001/SC-002 (Tasks 4, 5, 7, 8), SC-003 (Tasks 1, 2, 10), SC-004 (Tasks 1, 3, 5), SC-005 (Task 2), SC-006 (Tasks 6, 7, 8), SC-007 (Tasks 4, 5, 9, 10).
- **Type consistency:** `SecretState`/`ValueSource` are identical in Rust (`src/settings.rs`) and TypeScript (`web/src/lib/settings.svelte.ts`); the API uses `camelCase` end to end, so `custom_base_url` serializes to `customBaseUrl` and the TS fields match. `SettingsPatch` field names match `AiPatch`/`BringPatch` on both sides. `LlmTarget` is created in `settings::EffectiveAi::target()` and consumed only by `llm_import`.
- **Known follow-up (out of scope):** the planner's send action and the layout's footer probe keep their current behavior; only their credential source changes.
