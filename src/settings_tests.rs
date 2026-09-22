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
        SecretState {
            set: true,
            source: ValueSource::Settings
        }
    );
    assert_eq!(snapshot.bring.email, "cook@example.com");
    assert_eq!(snapshot.bring.email_source, ValueSource::Settings);
    assert_eq!(
        snapshot.bring.password,
        SecretState {
            set: true,
            source: ValueSource::Settings
        }
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
fn given_stored_key_for_another_provider_when_snapshot_then_key_is_not_borrowed_from_environment() {
    let stored = StoredSettings {
        provider: Some("anthropic".to_string()),
        api_key: Some("sk-openai".to_string()),
        ..StoredSettings::default()
    };
    let env = |key: &str| (key == "OPENAI_API_KEY").then(|| "sk-env".to_string());

    let snapshot = snapshot(&stored, &env);

    // A stored key belongs to the provider it was stored for: the environment
    // key of a provider that is not selected is never attributed to it.
    assert_eq!(
        snapshot.ai.api_key,
        SecretState {
            set: true,
            source: ValueSource::Settings
        }
    );
}

#[test]
fn given_stored_key_without_provider_when_snapshot_then_api_key_not_set() {
    let stored = StoredSettings {
        api_key: Some("sk-openai".to_string()),
        ..StoredSettings::default()
    };

    let snapshot = snapshot(&stored, &empty_env());

    // A key with no provider stored alongside it belongs to no provider, so
    // there is nothing to authenticate against.
    assert_eq!(
        snapshot.ai.api_key,
        SecretState {
            set: false,
            source: ValueSource::None
        }
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
