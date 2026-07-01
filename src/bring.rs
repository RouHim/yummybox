use std::time::Duration;

use reqwest::Client;
use serde::{Deserialize, Serialize};
use tracing::info;

use crate::error::AppError;

const BRING_BASE_URL: &str = "https://api.getbring.com/rest/v2";
const BRING_API_KEY: &str = "cof4Nc6D8saplXjE3h3HXqHH8m7VU2i1Gs0g85Sp";
const BRING_CLIENT: &str = "android";
const BRING_APPLICATION: &str = "bring";
const MAX_ITEM_NAME_LEN: usize = 100;

#[derive(Debug, Deserialize)]
struct BringAuthResponse {
    uuid: String,
    access_token: String,
}

#[derive(Debug, Deserialize)]
struct BringList {
    #[serde(rename = "listUuid")]
    list_uuid: String,
}

#[derive(Debug, Deserialize)]
struct BringListsResponse {
    lists: Vec<BringList>,
}

/// Status of the Bring! API connection, probed at startup.
#[derive(Debug, Serialize)]
pub enum BringStatus {
    #[serde(rename = "notConfigured")]
    NotConfigured,
    Connected {
        list_uuid: String,
    },
    Error(String),
}

/// Push a single ingredient to the user's first Bring! shopping list.
///
/// Reads `BRING_EMAIL` and `BRING_PASSWORD` from the environment at call time.
/// Truncates `name` to 100 characters if longer.
pub async fn push_item_to_bring(name: &str, spec: Option<&str>) -> Result<(), AppError> {
    let email = std::env::var("BRING_EMAIL").map_err(|_| {
        AppError::BadRequest(
            "Bring! credentials not configured: set BRING_EMAIL and BRING_PASSWORD".into(),
        )
    })?;
    let password = std::env::var("BRING_PASSWORD").map_err(|_| {
        AppError::BadRequest(
            "Bring! credentials not configured: set BRING_EMAIL and BRING_PASSWORD".into(),
        )
    })?;

    let name = truncate_name(name);

    let client = Client::builder()
        .timeout(Duration::from_secs(10))
        .connect_timeout(Duration::from_secs(5))
        .build()
        .map_err(|e| AppError::Internal(format!("failed to build HTTP client: {e}")))?;

    // Step 1: authenticate
    let auth = bring_login(&client, &email, &password).await?;
    info!(uuid = %auth.uuid, "authenticated with Bring! API");

    // Step 2: list shopping lists and pick the first one
    let first_list = bring_first_list(&client, &auth.uuid, &auth.access_token).await?;
    info!(list_uuid = %first_list.list_uuid, "selected first Bring! list");

    // Step 3: save the item
    bring_save_item(
        &client,
        &first_list.list_uuid,
        &auth.uuid,
        &auth.access_token,
        &name,
        spec.unwrap_or(""),
    )
    .await?;

    info!(name = %name, spec = ?spec, "pushed item to Bring! list");
    Ok(())
}

/// Probe Bring! credentials at startup. Returns the connection status without
/// making a network call when env vars are missing.
pub async fn check_bring_status() -> BringStatus {
    let email = match std::env::var("BRING_EMAIL") {
        Ok(v) => v,
        Err(_) => return BringStatus::NotConfigured,
    };
    let password = match std::env::var("BRING_PASSWORD") {
        Ok(v) => v,
        Err(_) => return BringStatus::NotConfigured,
    };

    let client = match Client::builder()
        .timeout(Duration::from_secs(10))
        .connect_timeout(Duration::from_secs(5))
        .build()
    {
        Ok(c) => c,
        Err(e) => return BringStatus::Error(format!("failed to build HTTP client: {e}")),
    };

    let auth = match bring_login(&client, &email, &password).await {
        Ok(a) => a,
        Err(e) => return BringStatus::Error(e.to_string()),
    };
    info!(uuid = %auth.uuid, "authenticated with Bring! API (status probe)");

    match bring_first_list(&client, &auth.uuid, &auth.access_token).await {
        Ok(list) => {
            info!(list_uuid = %list.list_uuid, "Bring! status probe successful");
            BringStatus::Connected {
                list_uuid: list.list_uuid,
            }
        }
        Err(e) => BringStatus::Error(e.to_string()),
    }
}

fn truncate_name(name: &str) -> String {
    if name.len() <= MAX_ITEM_NAME_LEN {
        name.to_string()
    } else {
        name[..MAX_ITEM_NAME_LEN].to_string()
    }
}

async fn bring_login(
    client: &Client,
    email: &str,
    password: &str,
) -> Result<BringAuthResponse, AppError> {
    let resp = client
        .post(format!("{BRING_BASE_URL}/bringauth"))
        .header("X-BRING-API-KEY", BRING_API_KEY)
        .header("X-BRING-CLIENT", BRING_CLIENT)
        .header("X-BRING-APPLICATION", BRING_APPLICATION)
        .form(&[("email", email), ("password", password)])
        .send()
        .await
        .map_err(|e| {
            if e.is_connect() || e.is_timeout() {
                AppError::BringNetworkError(
                    "Could not reach Bring! — check your network connection".into(),
                )
            } else {
                AppError::BringNetworkError(format!("Bring! request failed: {e}"))
            }
        })?;

    if resp.status().is_client_error() {
        return Err(AppError::BringAuthFailed(
            "Bring! login failed — check BRING_EMAIL and BRING_PASSWORD".into(),
        ));
    }

    if !resp.status().is_success() {
        return Err(AppError::BringNetworkError(format!(
            "Bring! returned unexpected status {}",
            resp.status()
        )));
    }

    resp.json::<BringAuthResponse>().await.map_err(|e| {
        AppError::BringNetworkError(format!("failed to parse Bring! auth response: {e}"))
    })
}

async fn bring_first_list(
    client: &Client,
    user_uuid: &str,
    access_token: &str,
) -> Result<BringList, AppError> {
    let resp = client
        .get(format!("{BRING_BASE_URL}/bringusers/{user_uuid}/lists"))
        .header("X-BRING-API-KEY", BRING_API_KEY)
        .header("X-BRING-CLIENT", BRING_CLIENT)
        .header("X-BRING-APPLICATION", BRING_APPLICATION)
        .bearer_auth(access_token)
        .send()
        .await
        .map_err(|e| AppError::BringNetworkError(format!("failed to fetch Bring! lists: {e}")))?;

    if !resp.status().is_success() {
        return Err(AppError::BringNetworkError(format!(
            "Bring! lists request returned status {}",
            resp.status()
        )));
    }

    let wrapper: BringListsResponse = resp.json().await.map_err(|e| {
        AppError::BringNetworkError(format!("failed to parse Bring! lists response: {e}"))
    })?;

    wrapper
        .lists
        .into_iter()
        .next()
        .ok_or(AppError::BringNoLists)
}
async fn bring_save_item(
    client: &Client,
    list_uuid: &str,
    user_uuid: &str,
    access_token: &str,
    name: &str,
    spec: &str,
) -> Result<(), AppError> {
    let item_uuid = uuid_v4();

    let resp = client
        .put(format!("{BRING_BASE_URL}/bringlists/{list_uuid}/items"))
        .header("X-BRING-API-KEY", BRING_API_KEY)
        .header("X-BRING-CLIENT", BRING_CLIENT)
        .header("X-BRING-APPLICATION", BRING_APPLICATION)
        .header("X-BRING-USER-UUID", user_uuid)
        .bearer_auth(access_token)
        .json(&serde_json::json!({
            "changes": [{
                "accuracy": "0.0",
                "altitude": "0.0",
                "latitude": "0.0",
                "longitude": "0.0",
                "itemId": name,
                "spec": spec,
                "uuid": item_uuid,
                "operation": "TO_PURCHASE"
            }],
            "sender": ""
        }))
        .send()
        .await
        .map_err(|e| AppError::BringNetworkError(format!("failed to save Bring! item: {e}")))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(AppError::BringNetworkError(format!(
            "Bring! save item returned status {status}: {body}"
        )));
    }

    Ok(())
}

/// Generate a UUID v4 string without pulling in the `uuid` crate.
fn uuid_v4() -> String {
    let mut buf = [0u8; 16];
    getrandom_fill(&mut buf);
    // Set version to 4
    buf[6] = (buf[6] & 0x0f) | 0x40;
    // Set variant to 10xx
    buf[8] = (buf[8] & 0x3f) | 0x80;
    format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        buf[0],
        buf[1],
        buf[2],
        buf[3],
        buf[4],
        buf[5],
        buf[6],
        buf[7],
        buf[8],
        buf[9],
        buf[10],
        buf[11],
        buf[12],
        buf[13],
        buf[14],
        buf[15],
    )
}
#[cfg(not(test))]
fn getrandom_fill(buf: &mut [u8]) {
    use rand::Rng;
    rand::rng().fill_bytes(buf);
}

#[cfg(test)]
fn getrandom_fill(buf: &mut [u8]) {
    // Deterministic for tests — just zero-fill; tests don't assert on UUID values.
    buf.fill(0xAB);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn given_short_name_when_truncated_then_unchanged() {
        assert_eq!(truncate_name("Tomatoes"), "Tomatoes");
    }

    #[test]
    fn given_name_exactly_100_chars_when_truncated_then_unchanged() {
        let name = "a".repeat(100);
        assert_eq!(truncate_name(&name), name);
    }

    #[test]
    fn given_name_longer_than_100_chars_when_truncated_then_cut_to_100() {
        let name = "a".repeat(150);
        assert_eq!(truncate_name(&name).len(), 100);
    }

    #[test]
    fn given_empty_name_when_truncated_then_unchanged() {
        assert_eq!(truncate_name(""), "");
    }

    #[test]
    fn uuid_v4_has_correct_format() {
        let uuid = uuid_v4();
        assert_eq!(uuid.len(), 36);
        assert_eq!(uuid.chars().filter(|&c| c == '-').count(), 4);
        // Version nibble should be 4
        assert_eq!(uuid.as_bytes()[14] as char, '4');
    }

    #[tokio::test]
    async fn given_no_bring_env_vars_when_check_status_then_not_configured() {
        // Ensure env vars are unset for this test
        let had_email = std::env::var("BRING_EMAIL").ok();
        let had_password = std::env::var("BRING_PASSWORD").ok();
        unsafe {
            std::env::remove_var("BRING_EMAIL");
            std::env::remove_var("BRING_PASSWORD");
        }

        let status = check_bring_status().await;
        match status {
            BringStatus::NotConfigured => {}
            other => panic!("expected NotConfigured, got {other:?}"),
        }

        // Restore env vars
        if let Some(v) = had_email {
            unsafe {
                std::env::set_var("BRING_EMAIL", v);
            }
        }
        if let Some(v) = had_password {
            unsafe {
                std::env::set_var("BRING_PASSWORD", v);
            }
        }
    }
}
