use std::time::Duration;

use reqwest::Client;
use serde::{Deserialize, Serialize};
use tracing::info;

use crate::error::AppError;

const BRING_BASE_URL: &str = "https://api.getbring.com/rest/v2";
/// Bring! ships one public app key to every client, so this is not a user
/// secret. `BRING_API_KEY` overrides it, which lets a rotation ship without
/// rebuilding the binary.
const BRING_API_KEY: &str = "cof4Nc6D8saplXjE3h3HXqHH8m7VU2i1Gs0g85Sp";
const BRING_API_KEY_ENV: &str = "BRING_API_KEY";
const BRING_CLIENT: &str = "android";
const BRING_APPLICATION: &str = "bring";
const MAX_ITEM_NAME_LEN: usize = 100;

/// API key for the `X-BRING-API-KEY` header: the bundled public key unless
/// `BRING_API_KEY` supplies a non-blank override.
fn api_key() -> String {
    api_key_from(&crate::settings::env_lookup)
}

/// Testable form of [`api_key`] with the environment lookup injected.
fn api_key_from(env: crate::settings::Env<'_>) -> String {
    env(BRING_API_KEY_ENV)
        .map(|key| key.trim().to_string())
        .filter(|key| !key.is_empty())
        .unwrap_or_else(|| BRING_API_KEY.to_string())
}

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

/// Bring! account credentials for one request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BringCredentials {
    pub email: String,
    pub password: String,
}

/// Push a single ingredient to the user's first Bring! shopping list.
/// Truncates `name` to 100 characters if longer.
pub async fn push_item_to_bring(
    creds: &BringCredentials,
    name: &str,
    spec: Option<&str>,
) -> Result<(), AppError> {
    push_item_to_bring_at(creds, name, spec, BRING_BASE_URL).await
}

/// Same as [`push_item_to_bring`], but against an explicit API origin.
/// `base_url` is a seam so tests can drive the flow against a local server;
/// production callers pass [`BRING_BASE_URL`].
async fn push_item_to_bring_at(
    creds: &BringCredentials,
    name: &str,
    spec: Option<&str>,
    base_url: &str,
) -> Result<(), AppError> {
    let name = truncate_name(name);

    let client = Client::builder()
        .timeout(Duration::from_secs(10))
        .connect_timeout(Duration::from_secs(5))
        .build()
        .map_err(|e| AppError::Internal(format!("failed to build HTTP client: {e}")))?;

    // Step 1: authenticate
    let auth = bring_login(&client, &creds.email, &creds.password, base_url).await?;
    info!(uuid = %auth.uuid, "authenticated with Bring! API");

    // Step 2: list shopping lists and pick the first one
    let first_list = bring_first_list(&client, &auth.uuid, &auth.access_token, base_url).await?;
    info!(list_uuid = %first_list.list_uuid, "selected first Bring! list");

    // Step 3: save the item
    bring_save_item(
        &client,
        &first_list.list_uuid,
        &auth.uuid,
        &auth.access_token,
        &name,
        spec.unwrap_or(""),
        base_url,
    )
    .await?;

    info!(name = %name, spec = ?spec, "pushed item to Bring! list");
    Ok(())
}

/// Probe the given credentials. `None` means no credentials are configured in
/// either the settings or the environment, and no network call is made.
pub async fn check_bring_status(creds: Option<&BringCredentials>) -> BringStatus {
    check_bring_status_at(creds, BRING_BASE_URL).await
}

/// Same as [`check_bring_status`], but against an explicit API origin.
async fn check_bring_status_at(creds: Option<&BringCredentials>, base_url: &str) -> BringStatus {
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

    let auth = match bring_login(&client, &creds.email, &creds.password, base_url).await {
        Ok(a) => a,
        Err(e) => return BringStatus::Error(e.to_string()),
    };
    info!(uuid = %auth.uuid, "authenticated with Bring! API (status probe)");

    match bring_first_list(&client, &auth.uuid, &auth.access_token, base_url).await {
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
    base_url: &str,
) -> Result<BringAuthResponse, AppError> {
    let resp = client
        .post(format!("{base_url}/bringauth"))
        .header("X-BRING-API-KEY", api_key())
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
            "Bring! login failed, check your Bring! credentials in Settings".to_string(),
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
    base_url: &str,
) -> Result<BringList, AppError> {
    let resp = client
        .get(format!("{base_url}/bringusers/{user_uuid}/lists"))
        .header("X-BRING-API-KEY", api_key())
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
    base_url: &str,
) -> Result<(), AppError> {
    let item_uuid = uuid_v4();

    let resp = client
        .put(format!("{base_url}/bringlists/{list_uuid}/items"))
        .header("X-BRING-API-KEY", api_key())
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
    fn given_no_env_override_when_api_key_then_bundled_key_used() {
        let env = |_: &str| -> Option<String> { None };
        assert_eq!(api_key_from(&env), BRING_API_KEY);
    }

    #[test]
    fn given_env_override_when_api_key_then_trimmed_override_used() {
        let env = |key: &str| -> Option<String> {
            (key == "BRING_API_KEY").then(|| "  rotated-key \n".to_string())
        };
        assert_eq!(api_key_from(&env), "rotated-key");
    }

    #[test]
    fn given_blank_env_override_when_api_key_then_bundled_key_used() {
        let env = |_: &str| -> Option<String> { Some("   ".to_string()) };
        assert_eq!(api_key_from(&env), BRING_API_KEY);
    }

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
    async fn given_no_credentials_when_check_status_then_not_configured() {
        match check_bring_status(None).await {
            BringStatus::NotConfigured => {}
            other => panic!("expected NotConfigured, got {other:?}"),
        }
    }

    // -------------------------------------------------------------------
    // Remote client: login -> list lookup -> item save
    //
    // The Bring! API is a real host, so these tests drive the client against
    // an ephemeral loopback server that plays back one canned HTTP/1.1
    // response per request and records what the client actually sent.
    // -------------------------------------------------------------------

    #[derive(Debug)]
    struct RecordedRequest {
        method: String,
        path: String,
        head: String,
        body: String,
    }

    impl RecordedRequest {
        fn header(&self, name: &str) -> Option<String> {
            let wanted = name.to_ascii_lowercase();
            self.head.lines().find_map(|line| {
                let (key, value) = line.split_once(':')?;
                (key.trim().to_ascii_lowercase() == wanted).then(|| value.trim().to_string())
            })
        }
    }

    async fn read_request(stream: &mut tokio::net::TcpStream) -> RecordedRequest {
        use tokio::io::AsyncReadExt;

        let mut raw = Vec::new();
        let mut chunk = [0u8; 1024];
        let head_end = loop {
            if let Some(pos) = raw.windows(4).position(|w| w == b"\r\n\r\n") {
                break pos + 4;
            }
            match stream.read(&mut chunk).await.unwrap_or(0) {
                0 => break raw.len(),
                n => raw.extend_from_slice(&chunk[..n]),
            }
        };
        let head = String::from_utf8_lossy(&raw[..head_end]).to_string();
        let content_length = head
            .lines()
            .find_map(|line| {
                let lower = line.to_ascii_lowercase();
                lower
                    .strip_prefix("content-length:")
                    .map(|value| value.trim().parse::<usize>().unwrap_or(0))
            })
            .unwrap_or(0);
        let mut body = raw[head_end..].to_vec();
        while body.len() < content_length {
            match stream.read(&mut chunk).await.unwrap_or(0) {
                0 => break,
                n => body.extend_from_slice(&chunk[..n]),
            }
        }
        let request_line = head.lines().next().unwrap_or("").to_string();
        let mut parts = request_line.split_whitespace();
        RecordedRequest {
            method: parts.next().unwrap_or("").to_string(),
            path: parts.next().unwrap_or("").to_string(),
            head,
            body: String::from_utf8_lossy(&body).to_string(),
        }
    }

    /// Serve `responses` in order, one connection each, and return the origin
    /// plus the requests the client sent.
    async fn serve(
        responses: Vec<(u16, &'static str, String)>,
    ) -> (
        String,
        std::sync::Arc<std::sync::Mutex<Vec<RecordedRequest>>>,
    ) {
        use tokio::io::AsyncWriteExt;

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().expect("local_addr");
        let recorded = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let sink = std::sync::Arc::clone(&recorded);
        tokio::spawn(async move {
            for (status, content_type, body) in responses {
                let Ok((mut stream, _)) = listener.accept().await else {
                    return;
                };
                let request = read_request(&mut stream).await;
                sink.lock().unwrap().push(request);
                let reason = if status < 400 { "OK" } else { "Error" };
                let head = format!(
                    "HTTP/1.1 {status} {reason}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = stream.write_all(head.as_bytes()).await;
                let _ = stream.write_all(body.as_bytes()).await;
                let _ = stream.shutdown().await;
            }
        });
        (format!("http://{addr}"), recorded)
    }

    fn creds() -> BringCredentials {
        BringCredentials {
            email: "cook@example.com".into(),
            password: "secret".into(),
        }
    }

    #[test]
    fn given_recorded_request_when_header_read_then_case_insensitive_lookup() {
        let request = RecordedRequest {
            method: "PUT".into(),
            path: "/x".into(),
            head: "PUT /x HTTP/1.1\r\nX-BRING-USER-UUID: user-1\r\n\r\n".into(),
            body: String::new(),
        };
        assert_eq!(
            request.header("x-bring-user-uuid").as_deref(),
            Some("user-1")
        );
        assert_eq!(request.header("absent"), None);
    }

    #[tokio::test]
    async fn given_working_api_when_push_item_then_login_lists_and_save_are_called() {
        let (origin, recorded) = serve(vec![
            (
                200,
                "application/json",
                r#"{"uuid":"user-1","access_token":"token-1"}"#.to_string(),
            ),
            (
                200,
                "application/json",
                r#"{"lists":[{"listUuid":"list-1"}]}"#.to_string(),
            ),
            (200, "application/json", "{}".to_string()),
        ])
        .await;
        let base_url = format!("{origin}/rest/v2");

        push_item_to_bring_at(&creds(), "Tomatoes", Some("2 kg"), &base_url)
            .await
            .expect("push succeeds");

        let requests = recorded.lock().unwrap();
        assert_eq!(requests.len(), 3, "requests: {requests:?}");

        assert_eq!(requests[0].method, "POST");
        assert_eq!(requests[0].path, "/rest/v2/bringauth");
        let expected_key = api_key();
        assert_eq!(
            requests[0].header("x-bring-api-key").as_deref(),
            Some(expected_key.as_str())
        );
        assert_eq!(
            requests[0].header("x-bring-client").as_deref(),
            Some("android")
        );
        assert!(
            requests[0].body.contains("email=cook%40example.com"),
            "body: {}",
            requests[0].body
        );
        assert!(
            requests[0].body.contains("password=secret"),
            "body: {}",
            requests[0].body
        );

        assert_eq!(requests[1].method, "GET");
        assert_eq!(requests[1].path, "/rest/v2/bringusers/user-1/lists");
        assert_eq!(
            requests[1].header("authorization").as_deref(),
            Some("Bearer token-1")
        );

        assert_eq!(requests[2].method, "PUT");
        assert_eq!(requests[2].path, "/rest/v2/bringlists/list-1/items");
        assert_eq!(
            requests[2].header("x-bring-user-uuid").as_deref(),
            Some("user-1")
        );
        let payload: serde_json::Value = serde_json::from_str(&requests[2].body).unwrap();
        let change = &payload["changes"][0];
        assert_eq!(change["itemId"], "Tomatoes");
        assert_eq!(change["spec"], "2 kg");
        assert_eq!(change["operation"], "TO_PURCHASE");
        assert_eq!(change["uuid"].as_str().map(str::len), Some(36));
    }

    #[tokio::test]
    async fn given_working_api_when_push_item_without_spec_then_empty_spec_sent() {
        let (origin, recorded) = serve(vec![
            (
                200,
                "application/json",
                r#"{"uuid":"user-1","access_token":"token-1"}"#.to_string(),
            ),
            (
                200,
                "application/json",
                r#"{"lists":[{"listUuid":"list-1"}]}"#.to_string(),
            ),
            (200, "application/json", "{}".to_string()),
        ])
        .await;
        let base_url = format!("{origin}/rest/v2");

        push_item_to_bring_at(&creds(), "Milk", None, &base_url)
            .await
            .expect("push succeeds");

        let requests = recorded.lock().unwrap();
        let payload: serde_json::Value = serde_json::from_str(&requests[2].body).unwrap();
        assert_eq!(payload["changes"][0]["spec"], "");
    }

    #[tokio::test]
    async fn given_login_returns_401_when_push_item_then_bring_auth_failed() {
        let (origin, recorded) = serve(vec![(401, "application/json", "{}".to_string())]).await;
        let base_url = format!("{origin}/rest/v2");

        let err = push_item_to_bring_at(&creds(), "Tomatoes", None, &base_url)
            .await
            .expect_err("login must fail");

        match err {
            AppError::BringAuthFailed(msg) => assert!(msg.contains("Bring! login failed")),
            other => panic!("expected BringAuthFailed, got {other:?}"),
        }
        assert_eq!(
            recorded.lock().unwrap().len(),
            1,
            "a rejected login must not proceed to the list lookup"
        );
    }

    #[tokio::test]
    async fn given_lists_response_without_lists_when_push_item_then_bring_no_lists() {
        let (origin, recorded) = serve(vec![
            (
                200,
                "application/json",
                r#"{"uuid":"user-1","access_token":"token-1"}"#.to_string(),
            ),
            (200, "application/json", r#"{"lists":[]}"#.to_string()),
        ])
        .await;
        let base_url = format!("{origin}/rest/v2");

        let err = push_item_to_bring_at(&creds(), "Tomatoes", None, &base_url)
            .await
            .expect_err("no list must fail");

        assert!(
            matches!(err, AppError::BringNoLists),
            "expected BringNoLists, got {err:?}"
        );
        assert_eq!(recorded.lock().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn given_save_returns_500_when_push_item_then_bring_network_error() {
        let (origin, _recorded) = serve(vec![
            (
                200,
                "application/json",
                r#"{"uuid":"user-1","access_token":"token-1"}"#.to_string(),
            ),
            (
                200,
                "application/json",
                r#"{"lists":[{"listUuid":"list-1"}]}"#.to_string(),
            ),
            (500, "text/plain", "boom".to_string()),
        ])
        .await;
        let base_url = format!("{origin}/rest/v2");

        let err = push_item_to_bring_at(&creds(), "Tomatoes", None, &base_url)
            .await
            .expect_err("save must fail");

        match err {
            AppError::BringNetworkError(msg) => assert!(msg.contains("boom"), "msg: {msg}"),
            other => panic!("expected BringNetworkError, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn given_working_api_when_check_status_then_connected_with_first_list() {
        let (origin, _recorded) = serve(vec![
            (
                200,
                "application/json",
                r#"{"uuid":"user-1","access_token":"token-1"}"#.to_string(),
            ),
            (
                200,
                "application/json",
                r#"{"lists":[{"listUuid":"list-1"}]}"#.to_string(),
            ),
        ])
        .await;
        let base_url = format!("{origin}/rest/v2");
        let credentials = creds();

        match check_bring_status_at(Some(&credentials), &base_url).await {
            BringStatus::Connected { list_uuid } => assert_eq!(list_uuid, "list-1"),
            other => panic!("expected Connected, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn given_bad_credentials_when_check_status_then_error_status() {
        let (origin, _recorded) = serve(vec![(403, "application/json", "{}".to_string())]).await;
        let base_url = format!("{origin}/rest/v2");
        let credentials = creds();

        match check_bring_status_at(Some(&credentials), &base_url).await {
            BringStatus::Error(msg) => assert!(msg.contains("Bring! login failed"), "msg: {msg}"),
            other => panic!("expected Error, got {other:?}"),
        }
    }
}
