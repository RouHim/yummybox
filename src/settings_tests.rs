// Tests for the settings domain module. Kept in a separate flat module so
// src/settings.rs stays focused on production code.

use crate::db;
use crate::settings::{
    KEY_BRING_EMAIL, KEY_BRING_PASSWORD, KEY_LLM_API_KEY, KEY_LLM_BASE_URL, KEY_LLM_MODEL,
    KEY_LLM_PROVIDER, SecretState, StoredSettings, ValueSource, load, provider_credentials,
    resolve_ai, resolve_bring, snapshot,
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
fn given_a_stored_provider_and_no_stored_key_when_snapshot_then_inherited_from_environment() {
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
fn given_whitespace_only_provider_key_env_when_snapshot_then_reported_unset() {
    // A variable holding only whitespace is the same deployment accident as an
    // empty one: `LlmTarget::auth` trims and drops blanks, so counting it as a
    // configured key would report the provider usable and let the request go
    // out with a blank bearer token instead of the actionable
    // `llm_api_key_missing`.
    let stored = StoredSettings {
        provider: Some("openai".to_string()),
        ..StoredSettings::default()
    };
    let env = |key: &str| (key == "OPENAI_API_KEY").then(|| "   ".to_string());

    let snapshot = snapshot(&stored, &env);

    assert_eq!(
        snapshot.ai.api_key,
        SecretState {
            set: false,
            source: ValueSource::None
        }
    );
    assert!(
        provider_credentials(&stored, "openai", &env)
            .api_key
            .is_none(),
        "a whitespace-only environment key must not resolve into credentials"
    );
}

#[test]
fn given_stored_key_for_the_selected_provider_when_snapshot_then_the_stored_key_wins() {
    // A stored key for the selected provider is reported as coming from the
    // settings; the stored-key branch never consults the environment, so the
    // environment key set here is ignored. The companion test below covers the
    // borrow rule when no key is stored at all.
    let stored = StoredSettings {
        provider: Some("anthropic".to_string()),
        api_key: Some("sk-anthropic-stored".to_string()),
        ..StoredSettings::default()
    };
    let env = |key: &str| (key == "ANTHROPIC_API_KEY").then(|| "sk-anthropic-env".to_string());

    let snapshot = snapshot(&stored, &env);

    assert_eq!(
        snapshot.ai.api_key,
        SecretState {
            set: true,
            source: ValueSource::Settings
        }
    );
}

#[test]
fn given_no_stored_key_when_snapshot_then_another_providers_environment_key_is_not_borrowed() {
    // The stored-key-wins companion above never reaches the environment
    // lookup; here the install has no stored key at all, so the only candidate
    // would be the API-key variable of a provider that is not the stored one.
    let stored = StoredSettings {
        provider: Some("anthropic".to_string()),
        ..StoredSettings::default()
    };
    let env = |key: &str| (key == "OPENAI_API_KEY").then(|| "sk-openai-env".to_string());

    let snapshot = snapshot(&stored, &env);

    assert_eq!(
        snapshot.ai.api_key,
        SecretState {
            set: false,
            source: ValueSource::None
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
        (
            KEY_LLM_BASE_URL.to_string(),
            format!("http://host/{}", "x".repeat(3000)),
        ),
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
fn given_malformed_stored_base_url_when_read_then_treated_as_unset() {
    let stored = StoredSettings::from_pairs(vec![
        (KEY_LLM_PROVIDER.to_string(), "custom".to_string()),
        (KEY_LLM_MODEL.to_string(), "llama3".to_string()),
        (
            KEY_LLM_BASE_URL.to_string(),
            "localhost:8080/v1/".to_string(),
        ),
        (KEY_LLM_API_KEY.to_string(), "sk-stored".to_string()),
    ]);

    assert_eq!(
        stored.base_url, None,
        "a stored base URL that fails the write-path validation must be dropped"
    );
    // The malformed endpoint can therefore not reach the provider call: the
    // custom provider reports itself as not configured instead of failing with
    // an opaque provider error.
    let err = resolve_ai(&stored, &empty_env()).expect_err("custom without base url");
    assert!(
        err.to_string().contains("customBaseUrl must be set"),
        "{err}"
    );
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
fn given_present_but_empty_environment_values_when_snapshot_then_reported_unset() {
    // An exported-but-empty variable is a common deployment accident. Every
    // reader treats it as unset, so no flow ever runs with an empty secret: the
    // provider key lookup, the Bring! email and the Bring! password alike.
    let stored = StoredSettings {
        provider: Some("openai".to_string()),
        ..StoredSettings::default()
    };
    let env = |key: &str| match key {
        "OPENAI_API_KEY" | "BRING_EMAIL" | "BRING_PASSWORD" => Some(String::new()),
        _ => None,
    };

    let snapshot = snapshot(&stored, &env);

    assert_eq!(
        snapshot.ai.api_key,
        SecretState {
            set: false,
            source: ValueSource::None
        }
    );
    assert_eq!(snapshot.bring.email_source, ValueSource::None);
    assert_eq!(
        snapshot.bring.password,
        SecretState {
            set: false,
            source: ValueSource::None
        }
    );
    assert!(
        resolve_bring(&StoredSettings::default(), &env).is_none(),
        "empty environment values must not resolve into credentials"
    );
}

#[test]
fn given_whitespace_only_environment_values_when_snapshot_then_reported_unset() {
    // A variable holding only blanks is as unusable as an empty one: resolving
    // it would report the Bring! credentials as set and send a blank login that
    // cannot succeed, instead of the not-configured state the flows expect.
    let stored = StoredSettings {
        provider: Some("openai".to_string()),
        ..StoredSettings::default()
    };
    let env = |key: &str| match key {
        "OPENAI_API_KEY" | "BRING_EMAIL" | "BRING_PASSWORD" => Some("   ".to_string()),
        _ => None,
    };

    let snapshot = snapshot(&stored, &env);

    assert_eq!(
        snapshot.ai.api_key,
        SecretState {
            set: false,
            source: ValueSource::None
        }
    );
    assert_eq!(snapshot.bring.email_source, ValueSource::None);
    assert!(snapshot.bring.email.is_empty());
    assert_eq!(
        snapshot.bring.password,
        SecretState {
            set: false,
            source: ValueSource::None
        }
    );
    assert!(
        resolve_bring(&StoredSettings::default(), &env).is_none(),
        "whitespace-only environment values must not resolve into credentials"
    );
    // A stored value is still used verbatim, blanks and all.
    let stored_bring = StoredSettings {
        bring_email: Some(" spaced@example.com ".to_string()),
        bring_password: Some(" spaced-pass ".to_string()),
        ..StoredSettings::default()
    };
    let credentials = resolve_bring(&stored_bring, &env).expect("stored credentials");
    assert_eq!(credentials.email, " spaced@example.com ");
    assert_eq!(credentials.password, " spaced-pass ");
}

// ---------------------------------------------------------------------------
// Commits
// ---------------------------------------------------------------------------

use crate::settings::{AiPatch, BringPatch, SettingWrites, SettingsPatch, apply, plan_writes};

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
    assert!(
        message.contains("model must be at most 200 characters"),
        "{message}"
    );
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
        err.to_string()
            .contains("customBaseUrl must not contain whitespace"),
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
async fn given_stored_key_when_provider_changes_without_a_key_then_stored_key_is_dropped() {
    let (pool, _dir) = setup_db().await;
    apply(
        &pool,
        &SettingsPatch {
            ai: Some(AiPatch {
                provider: Some(Some("openai".to_string())),
                model: Some(Some("gpt-4o-mini".to_string())),
                api_key: Some(Some("sk-openai".to_string())),
                ..AiPatch::default()
            }),
            bring: None,
        },
    )
    .await
    .expect("store key");

    apply(
        &pool,
        &SettingsPatch {
            ai: Some(AiPatch {
                provider: Some(Some("anthropic".to_string())),
                ..AiPatch::default()
            }),
            bring: None,
        },
    )
    .await
    .expect("switch provider");

    let stored = load(&pool).await.expect("load");
    assert_eq!(stored.provider.as_deref(), Some("anthropic"));
    assert_eq!(
        stored.api_key, None,
        "a key must not follow a provider switch to another endpoint"
    );
}

#[tokio::test]
async fn given_stored_key_when_custom_base_url_changes_without_a_key_then_stored_key_is_dropped() {
    let (pool, _dir) = setup_db().await;
    apply(
        &pool,
        &SettingsPatch {
            ai: Some(AiPatch {
                provider: Some(Some("custom".to_string())),
                model: Some(Some("llama3".to_string())),
                custom_base_url: Some(Some("http://localhost:8080/v1/".to_string())),
                api_key: Some(Some("local-key".to_string())),
            }),
            bring: None,
        },
    )
    .await
    .expect("store key");

    // The stored key was valid for the localhost endpoint; pointing the custom
    // provider somewhere else must not deliver it to the new host.
    apply(
        &pool,
        &SettingsPatch {
            ai: Some(AiPatch {
                custom_base_url: Some(Some("http://elsewhere.example/v1/".to_string())),
                ..AiPatch::default()
            }),
            bring: None,
        },
    )
    .await
    .expect("move endpoint");

    let stored = load(&pool).await.expect("load");
    assert_eq!(
        stored.base_url.as_deref(),
        Some("http://elsewhere.example/v1/")
    );
    assert_eq!(
        stored.api_key, None,
        "a key must not survive a base URL change"
    );
}

#[tokio::test]
async fn given_stored_key_when_the_provider_is_cleared_without_a_key_then_stored_key_is_dropped() {
    let (pool, _dir) = setup_db().await;
    apply(
        &pool,
        &SettingsPatch {
            ai: Some(AiPatch {
                provider: Some(Some("openai".to_string())),
                model: Some(Some("gpt-4o-mini".to_string())),
                api_key: Some(Some("sk-openai".to_string())),
                ..AiPatch::default()
            }),
            bring: None,
        },
    )
    .await
    .expect("store key");

    // Clearing the provider detaches the key from any endpoint it could
    // authenticate against.
    apply(
        &pool,
        &SettingsPatch {
            ai: Some(AiPatch {
                provider: Some(None),
                ..AiPatch::default()
            }),
            bring: None,
        },
    )
    .await
    .expect("clear provider");

    let stored = load(&pool).await.expect("load");
    assert_eq!(stored.provider, None);
    assert_eq!(
        stored.api_key, None,
        "a key must not outlive the provider it was stored for"
    );
}

#[tokio::test]
async fn given_stored_key_when_the_custom_base_url_is_cleared_then_stored_key_is_dropped() {
    let (pool, _dir) = setup_db().await;
    apply(
        &pool,
        &SettingsPatch {
            ai: Some(AiPatch {
                provider: Some(Some("custom".to_string())),
                model: Some(Some("llama3".to_string())),
                custom_base_url: Some(Some("http://localhost:8080/v1/".to_string())),
                api_key: Some(Some("local-key".to_string())),
            }),
            bring: None,
        },
    )
    .await
    .expect("store key");

    // Clearing the custom endpoint leaves the provider `custom` but removes the
    // endpoint the key was bound to.
    apply(
        &pool,
        &SettingsPatch {
            ai: Some(AiPatch {
                custom_base_url: Some(None),
                ..AiPatch::default()
            }),
            bring: None,
        },
    )
    .await
    .expect("clear base url");

    let stored = load(&pool).await.expect("load");
    assert_eq!(stored.base_url, None);
    assert_eq!(
        stored.api_key, None,
        "a key must not survive the removal of its custom endpoint"
    );
}

#[tokio::test]
async fn given_a_key_without_a_provider_when_applied_then_rejected_naming_provider() {
    // The API-only state { provider: null, apiKey: "k" } has no endpoint the
    // key could be bound to, so storing it would only produce a key the next
    // commit drops. The rejection names the field the form has to point at.
    let (pool, _dir) = setup_db().await;
    let patch = SettingsPatch {
        ai: Some(AiPatch {
            provider: Some(None),
            api_key: Some(Some("sk-orphan".to_string())),
            ..AiPatch::default()
        }),
        bring: None,
    };
    let err = apply(&pool, &patch).await.expect_err("must reject");
    assert!(err.to_string().contains("provider must be set"), "{err}");
    let stored = load(&pool).await.expect("load");
    assert_eq!(stored.api_key, None, "a rejected commit writes nothing");
}

#[tokio::test]
async fn given_a_key_and_no_provider_field_when_applied_then_rejected_naming_provider() {
    // The same state reached with the provider field omitted: nothing is
    // stored, so the effective provider is absent and the key has no endpoint.
    let (pool, _dir) = setup_db().await;
    let patch = SettingsPatch {
        ai: Some(AiPatch {
            api_key: Some(Some("sk-orphan".to_string())),
            ..AiPatch::default()
        }),
        bring: None,
    };
    let err = apply(&pool, &patch).await.expect_err("must reject");
    assert!(err.to_string().contains("provider must be set"), "{err}");
    assert_eq!(
        load(&pool).await.expect("load").api_key,
        None,
        "a rejected commit writes nothing"
    );
}

#[tokio::test]
async fn given_a_whitespace_only_key_when_applied_then_clears_instead_of_being_rejected() {
    // A blank key is a clear for `plan_text`, so this commit stores no key at
    // all: rejecting it for a missing endpoint would name a constraint the
    // commit never violates, and the clear the caller asked for would not
    // happen. The stored key is dropped by the same commit that clears the
    // provider, whose dependent values go with it.
    let (pool, _dir) = setup_db().await;
    apply(
        &pool,
        &SettingsPatch {
            ai: Some(AiPatch {
                provider: Some(Some("openai".to_string())),
                api_key: Some(Some("sk-stored".to_string())),
                ..AiPatch::default()
            }),
            bring: None,
        },
    )
    .await
    .expect("store key");

    apply(
        &pool,
        &SettingsPatch {
            ai: Some(AiPatch {
                provider: Some(None),
                api_key: Some(Some("   ".to_string())),
                ..AiPatch::default()
            }),
            bring: None,
        },
    )
    .await
    .expect("a blank key clears without needing an endpoint");

    let stored = load(&pool).await.expect("load");
    assert_eq!(stored.api_key, None);
    assert_eq!(stored.provider, None);
}

#[tokio::test]
async fn given_a_key_for_custom_without_a_base_url_when_applied_then_rejected_naming_the_endpoint()
{
    // The reachable order from the settings page: select `custom`, paste the
    // key, then enter the endpoint. Accepting the key here would report it as
    // stored and let the endpoint commit revoke it silently.
    let (pool, _dir) = setup_db().await;
    let patch = SettingsPatch {
        ai: Some(AiPatch {
            provider: Some(Some("custom".to_string())),
            api_key: Some(Some("sk-local".to_string())),
            ..AiPatch::default()
        }),
        bring: None,
    };
    let err = apply(&pool, &patch).await.expect_err("must reject");
    assert!(
        err.to_string().contains("customBaseUrl must be set"),
        "{err}"
    );
    let stored = load(&pool).await.expect("load");
    assert_eq!(stored.api_key, None, "a rejected commit writes nothing");
    assert_eq!(stored.provider, None, "a rejected commit writes nothing");
}

#[tokio::test]
async fn given_a_key_for_a_stored_custom_provider_with_a_stored_endpoint_when_applied_then_stored()
{
    // The other order: the endpoint is committed first, so the key that
    // follows has an endpoint to bind to and must be accepted.
    let (pool, _dir) = setup_db().await;
    apply(
        &pool,
        &SettingsPatch {
            ai: Some(AiPatch {
                provider: Some(Some("custom".to_string())),
                custom_base_url: Some(Some("http://localhost:8080/v1/".to_string())),
                ..AiPatch::default()
            }),
            bring: None,
        },
    )
    .await
    .expect("store endpoint");

    apply(
        &pool,
        &SettingsPatch {
            ai: Some(AiPatch {
                api_key: Some(Some("local-key".to_string())),
                ..AiPatch::default()
            }),
            bring: None,
        },
    )
    .await
    .expect("store key");

    let stored = load(&pool).await.expect("load");
    assert_eq!(stored.api_key.as_deref(), Some("local-key"));
}

#[tokio::test]
async fn given_a_key_for_a_stored_endpoint_less_provider_when_applied_then_rejected() {
    // A stored `custom` provider with no base URL is endpoint-less too, so a
    // key sent without one is rejected on the stored state alone.
    let (pool, _dir) = setup_db().await;
    apply(
        &pool,
        &SettingsPatch {
            ai: Some(AiPatch {
                provider: Some(Some("custom".to_string())),
                ..AiPatch::default()
            }),
            bring: None,
        },
    )
    .await
    .expect("store provider");

    let patch = SettingsPatch {
        ai: Some(AiPatch {
            api_key: Some(Some("sk-local".to_string())),
            ..AiPatch::default()
        }),
        bring: None,
    };
    let err = apply(&pool, &patch).await.expect_err("must reject");
    assert!(
        err.to_string().contains("customBaseUrl must be set"),
        "{err}"
    );
    assert_eq!(
        load(&pool).await.expect("load").api_key,
        None,
        "a rejected commit writes nothing"
    );
}

#[tokio::test]
async fn given_a_key_for_a_stored_endpoint_less_provider_when_the_endpoint_lands_in_the_same_commit_then_stored()
 {
    // A stored `custom` provider without an endpoint accepts a key as soon as
    // the same commit supplies the endpoint, so the rejection never forces two
    // round trips for a state the commit itself completes.
    let (pool, _dir) = setup_db().await;
    apply(
        &pool,
        &SettingsPatch {
            ai: Some(AiPatch {
                provider: Some(Some("custom".to_string())),
                ..AiPatch::default()
            }),
            bring: None,
        },
    )
    .await
    .expect("store provider");

    apply(
        &pool,
        &SettingsPatch {
            ai: Some(AiPatch {
                custom_base_url: Some(Some("http://localhost:8080/v1/".to_string())),
                api_key: Some(Some("local-key".to_string())),
                ..AiPatch::default()
            }),
            bring: None,
        },
    )
    .await
    .expect("store endpoint and key");

    let stored = load(&pool).await.expect("load");
    assert_eq!(stored.api_key.as_deref(), Some("local-key"));
}

#[tokio::test]
async fn given_stored_key_for_openai_when_the_custom_base_url_changes_then_stored_key_is_kept() {
    let (pool, _dir) = setup_db().await;
    apply(
        &pool,
        &SettingsPatch {
            ai: Some(AiPatch {
                provider: Some(Some("openai".to_string())),
                model: Some(Some("gpt-4o-mini".to_string())),
                api_key: Some(Some("sk-openai".to_string())),
                ..AiPatch::default()
            }),
            bring: None,
        },
    )
    .await
    .expect("store key");

    // The stored base URL never reaches a request while the provider is not
    // `custom`, so editing it must not discard the provider's key.
    apply(
        &pool,
        &SettingsPatch {
            ai: Some(AiPatch {
                custom_base_url: Some(Some("http://other.example/v1/".to_string())),
                ..AiPatch::default()
            }),
            bring: None,
        },
    )
    .await
    .expect("change custom base url");

    let stored = load(&pool).await.expect("load");
    assert_eq!(
        stored.api_key.as_deref(),
        Some("sk-openai"),
        "a non-custom provider keeps its key when the unused custom URL changes"
    );

    // Clearing the URL with an explicit null is likewise not an endpoint move
    // for a key whose provider is not `custom`.
    apply(
        &pool,
        &SettingsPatch {
            ai: Some(AiPatch {
                custom_base_url: Some(None),
                ..AiPatch::default()
            }),
            bring: None,
        },
    )
    .await
    .expect("clear custom base url");

    let stored = load(&pool).await.expect("load");
    assert_eq!(stored.base_url, None);
    assert_eq!(
        stored.api_key.as_deref(),
        Some("sk-openai"),
        "clearing the unused custom URL keeps a non-custom provider's key"
    );
}

#[tokio::test]
async fn given_endpoint_moves_when_the_same_commit_supplies_a_key_then_the_supplied_key_is_kept() {
    let (pool, _dir) = setup_db().await;
    apply(
        &pool,
        &SettingsPatch {
            ai: Some(AiPatch {
                provider: Some(Some("openai".to_string())),
                model: Some(Some("gpt-4o-mini".to_string())),
                api_key: Some(Some("sk-openai".to_string())),
                ..AiPatch::default()
            }),
            bring: None,
        },
    )
    .await
    .expect("store key");

    apply(
        &pool,
        &SettingsPatch {
            ai: Some(AiPatch {
                provider: Some(Some("custom".to_string())),
                model: Some(Some("llama3".to_string())),
                custom_base_url: Some(Some("http://localhost:8080/v1/".to_string())),
                api_key: Some(Some("local-key".to_string())),
            }),
            bring: None,
        },
    )
    .await
    .expect("move endpoint with a key");

    let stored = load(&pool).await.expect("load");
    assert_eq!(stored.provider.as_deref(), Some("custom"));
    assert_eq!(
        stored.api_key.as_deref(),
        Some("local-key"),
        "a key supplied with the new endpoint rebinds to it"
    );
}

#[tokio::test]
async fn given_unchanged_endpoint_when_committing_another_field_then_stored_key_survives() {
    let (pool, _dir) = setup_db().await;
    apply(
        &pool,
        &SettingsPatch {
            ai: Some(AiPatch {
                provider: Some(Some("openai".to_string())),
                model: Some(Some("gpt-4o-mini".to_string())),
                api_key: Some(Some("sk-openai".to_string())),
                ..AiPatch::default()
            }),
            bring: None,
        },
    )
    .await
    .expect("store key");

    // The same provider is committed again alongside a model change: that is
    // not an endpoint move, so the key stays.
    apply(
        &pool,
        &SettingsPatch {
            ai: Some(AiPatch {
                provider: Some(Some("openai".to_string())),
                model: Some(Some("gpt-4o".to_string())),
                ..AiPatch::default()
            }),
            bring: None,
        },
    )
    .await
    .expect("change model");

    let stored = load(&pool).await.expect("load");
    assert_eq!(stored.model.as_deref(), Some("gpt-4o"));
    assert_eq!(
        stored.api_key.as_deref(),
        Some("sk-openai"),
        "an unchanged endpoint keeps its key"
    );
}

#[tokio::test]
async fn given_stored_model_when_provider_changes_without_a_model_then_stored_model_is_dropped() {
    let (pool, _dir) = setup_db().await;
    apply(
        &pool,
        &SettingsPatch {
            ai: Some(AiPatch {
                provider: Some(Some("openai".to_string())),
                model: Some(Some("gpt-4o-mini".to_string())),
                ..AiPatch::default()
            }),
            bring: None,
        },
    )
    .await
    .expect("store model");

    // The commit names only the new provider: a model chosen for OpenAI does
    // not exist at Anthropic, so it must not be stored as Anthropic's model.
    apply(
        &pool,
        &SettingsPatch {
            ai: Some(AiPatch {
                provider: Some(Some("anthropic".to_string())),
                ..AiPatch::default()
            }),
            bring: None,
        },
    )
    .await
    .expect("switch provider");

    let stored = load(&pool).await.expect("load");
    assert_eq!(stored.provider.as_deref(), Some("anthropic"));
    assert_eq!(
        stored.model, None,
        "a model must not follow a provider switch to another listing"
    );
}

#[tokio::test]
async fn given_provider_moves_when_the_same_commit_supplies_a_model_then_the_supplied_model_is_kept()
 {
    let (pool, _dir) = setup_db().await;
    apply(
        &pool,
        &SettingsPatch {
            ai: Some(AiPatch {
                provider: Some(Some("openai".to_string())),
                model: Some(Some("gpt-4o-mini".to_string())),
                ..AiPatch::default()
            }),
            bring: None,
        },
    )
    .await
    .expect("store model");

    apply(
        &pool,
        &SettingsPatch {
            ai: Some(AiPatch {
                provider: Some(Some("anthropic".to_string())),
                model: Some(Some("claude-3-5-haiku".to_string())),
                ..AiPatch::default()
            }),
            bring: None,
        },
    )
    .await
    .expect("switch provider with a model");

    let stored = load(&pool).await.expect("load");
    assert_eq!(stored.provider.as_deref(), Some("anthropic"));
    assert_eq!(
        stored.model.as_deref(),
        Some("claude-3-5-haiku"),
        "a model supplied with the new provider belongs to it"
    );
}

#[tokio::test]
async fn given_stored_model_when_the_same_provider_is_committed_without_a_model_then_it_survives() {
    let (pool, _dir) = setup_db().await;
    apply(
        &pool,
        &SettingsPatch {
            ai: Some(AiPatch {
                provider: Some(Some("openai".to_string())),
                model: Some(Some("gpt-4o-mini".to_string())),
                ..AiPatch::default()
            }),
            bring: None,
        },
    )
    .await
    .expect("store model");

    // A selection that changed nothing sends the provider alone: re-sending it
    // is not a move, so the stored model must stay.
    apply(
        &pool,
        &SettingsPatch {
            ai: Some(AiPatch {
                provider: Some(Some("openai".to_string())),
                ..AiPatch::default()
            }),
            bring: None,
        },
    )
    .await
    .expect("resend provider");

    let stored = load(&pool).await.expect("load");
    assert_eq!(stored.provider.as_deref(), Some("openai"));
    assert_eq!(
        stored.model.as_deref(),
        Some("gpt-4o-mini"),
        "re-sending the stored provider keeps its model"
    );
}

#[tokio::test]
async fn given_stored_model_when_the_provider_is_cleared_then_stored_model_is_dropped() {
    let (pool, _dir) = setup_db().await;
    apply(
        &pool,
        &SettingsPatch {
            ai: Some(AiPatch {
                provider: Some(Some("openai".to_string())),
                model: Some(Some("gpt-4o-mini".to_string())),
                ..AiPatch::default()
            }),
            bring: None,
        },
    )
    .await
    .expect("store model");

    apply(
        &pool,
        &SettingsPatch {
            ai: Some(AiPatch {
                provider: Some(None),
                ..AiPatch::default()
            }),
            bring: None,
        },
    )
    .await
    .expect("clear provider");

    let stored = load(&pool).await.expect("load");
    assert_eq!(stored.provider, None);
    assert_eq!(
        stored.model, None,
        "a model must not outlive the provider it was chosen for"
    );
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
        err.to_string()
            .contains("password must be at most 256 characters"),
        "{err}"
    );
}

#[tokio::test]
async fn given_existing_values_when_committed_again_then_stored_values_are_replaced() {
    let (pool, _dir) = setup_db().await;
    apply(
        &pool,
        &SettingsPatch {
            ai: Some(AiPatch {
                provider: Some(Some("openai".to_string())),
                model: Some(Some("gpt-4o-mini".to_string())),
                ..AiPatch::default()
            }),
            bring: Some(BringPatch {
                email: Some(Some("first@example.com".to_string())),
                password: Some(Some("first-pass".to_string())),
            }),
        },
    )
    .await
    .expect("first commit");

    apply(
        &pool,
        &SettingsPatch {
            ai: Some(AiPatch {
                model: Some(Some("gpt-4o".to_string())),
                ..AiPatch::default()
            }),
            bring: Some(BringPatch {
                email: Some(Some("second@example.com".to_string())),
                password: Some(Some("second-pass".to_string())),
            }),
        },
    )
    .await
    .expect("replacement commit");

    let stored = load(&pool).await.expect("load");
    assert_eq!(
        stored.provider.as_deref(),
        Some("openai"),
        "untouched fields survive"
    );
    assert_eq!(stored.model.as_deref(), Some("gpt-4o"));
    assert_eq!(stored.bring_email.as_deref(), Some("second@example.com"));
    assert_eq!(stored.bring_password.as_deref(), Some("second-pass"));
}

#[test]
fn given_no_provider_or_model_when_resolve_ai_then_not_configured() {
    let env = empty_env();
    assert!(
        resolve_ai(&StoredSettings::default(), &env)
            .expect("ok")
            .is_none()
    );

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
    assert!(
        err.to_string().contains("customBaseUrl must be set"),
        "{err}"
    );
}

#[test]
fn given_foreign_stored_key_when_credentials_for_another_provider_then_not_applied() {
    // The stored key was saved for anthropic; openai must fall back to its own
    // environment key instead of inheriting a key that belongs to another
    // provider.
    let stored = StoredSettings {
        provider: Some("anthropic".to_string()),
        model: Some("claude-3-5-sonnet".to_string()),
        api_key: Some("sk-anthropic-stored".to_string()),
        ..StoredSettings::default()
    };
    let env = |key: &str| (key == "OPENAI_API_KEY").then(|| "sk-openai-env".to_string());

    let foreign = provider_credentials(&stored, "openai", &env);
    assert_eq!(foreign.api_key.as_deref(), Some("sk-openai-env"));

    // …and the key is still used for the provider it was stored for.
    let own = provider_credentials(&stored, "anthropic", &env);
    assert_eq!(own.api_key.as_deref(), Some("sk-anthropic-stored"));
}

#[test]
fn given_environment_openai_key_when_credentials_for_custom_provider_then_not_inherited() {
    // The custom provider has no API-key environment variable of its own: a
    // key stored for openai (or its environment variable) must never leak into
    // a custom endpoint, which the user may not own.
    let stored = StoredSettings {
        provider: Some("custom".to_string()),
        model: Some("llama3".to_string()),
        base_url: Some("http://localhost:8080/v1/".to_string()),
        ..StoredSettings::default()
    };
    let env = |key: &str| (key == "OPENAI_API_KEY").then(|| "sk-openai-env".to_string());

    let credentials = provider_credentials(&stored, "custom", &env);
    assert!(
        credentials.api_key.is_none(),
        "a custom provider must not inherit OPENAI_API_KEY"
    );

    let snap = snapshot(&stored, &env);
    assert!(
        !snap.ai.api_key.set,
        "the snapshot must not report a configured key for a keyless custom provider"
    );
}

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

#[tokio::test]
async fn given_stored_credentials_without_environment_when_cleared_then_not_configured() {
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

#[test]
fn given_no_stored_credentials_when_resolved_then_environment_values_apply() {
    let env = |key: &str| match key {
        "BRING_EMAIL" => Some("env@example.com".to_string()),
        "BRING_PASSWORD" => Some("env-pass".to_string()),
        _ => None,
    };

    let credentials = resolve_bring(&StoredSettings::default(), &env).expect("credentials");

    assert_eq!(credentials.email, "env@example.com");
    assert_eq!(credentials.password, "env-pass");
}

#[test]
fn given_only_one_stored_field_when_resolved_then_the_other_falls_back_to_environment() {
    let stored = StoredSettings {
        bring_email: Some("stored@example.com".to_string()),
        ..StoredSettings::default()
    };
    let env = |key: &str| match key {
        "BRING_EMAIL" => Some("env@example.com".to_string()),
        "BRING_PASSWORD" => Some("env-pass".to_string()),
        _ => None,
    };

    let credentials = resolve_bring(&stored, &env).expect("credentials");

    // Each field resolves on its own: stored email, inherited password.
    assert_eq!(credentials.email, "stored@example.com");
    assert_eq!(credentials.password, "env-pass");
}

#[tokio::test]
async fn given_cleared_credentials_with_environment_when_resolved_then_environment_values_apply() {
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
    .expect("store credentials");
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
    .expect("clear credentials");

    let env = |key: &str| match key {
        "BRING_EMAIL" => Some("env@example.com".to_string()),
        "BRING_PASSWORD" => Some("env-pass".to_string()),
        _ => None,
    };
    let stored = load(&pool).await.expect("load");
    let credentials = resolve_bring(&stored, &env).expect("credentials");

    assert_eq!(credentials.email, "env@example.com");
    assert_eq!(credentials.password, "env-pass");
}
