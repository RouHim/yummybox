use axum::body::Body;
use axum::http::{Response, StatusCode, Uri};
use axum::response::IntoResponse;
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "web/build/"]
pub struct Assets;

fn mime_for(path: &str) -> &'static str {
    match path.rsplit('.').next().unwrap_or("") {
        "html" => "text/html; charset=utf-8",
        "js" | "mjs" => "application/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "svg" => "image/svg+xml",
        "json" => "application/json; charset=utf-8",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "ico" => "image/x-icon",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        "ttf" => "font/ttf",
        "txt" => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

pub async fn spa_fallback(uri: Uri) -> impl IntoResponse {
    let path = uri.path().trim_start_matches('/');
    let path = if path.is_empty() { "index.html" } else { path };

    if let Some(file) = Assets::get(path) {
        Response::builder()
            .status(StatusCode::OK)
            .header("content-type", mime_for(path))
            .body(Body::from(file.data.into_owned()))
            .unwrap()
    } else {
        let index = Assets::get("index.html").expect("index.html must exist in embedded assets");
        Response::builder()
            .status(StatusCode::OK)
            .header("content-type", "text/html; charset=utf-8")
            .body(Body::from(index.data.into_owned()))
            .unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::Router;
    use axum::body::to_bytes;
    use axum::http::Request;
    use tower::ServiceExt;

    #[tokio::test]
    async fn given_root_uri_when_spa_fallback_then_returns_index_html() {
        let app = Router::new().fallback(spa_fallback);
        let response = app
            .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), 65536).await.unwrap();
        let body_str = String::from_utf8_lossy(&body);
        assert!(
            body_str.contains("<!doctype") || body_str.contains("<html"),
            "index.html should contain doctype or html tag, got: {body_str}"
        );
    }

    #[tokio::test]
    async fn given_spa_route_when_spa_fallback_then_returns_index_html() {
        let app = Router::new().fallback(spa_fallback);
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/some/spa/route")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), 65536).await.unwrap();
        let body_str = String::from_utf8_lossy(&body);
        assert!(body_str.contains("<!doctype") || body_str.contains("<html"));
    }

    #[tokio::test]
    async fn given_known_asset_when_spa_fallback_then_returns_with_correct_type() {
        // Find any .js file from embedded assets
        let js_file = Assets::iter()
            .find(|f| f.ends_with(".js") || f.ends_with(".mjs"))
            .expect("should have at least one .js file in build output");

        let app = Router::new().fallback(spa_fallback);
        let response = app
            .oneshot(
                Request::builder()
                    .uri(format!("/{js_file}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let content_type = response
            .headers()
            .get("content-type")
            .expect("should have content-type header")
            .to_str()
            .unwrap();
        assert!(
            content_type.contains("javascript"),
            "expected javascript content-type, got: {content_type}"
        );

        let expected = Assets::get(&js_file).expect("embedded asset should exist");
        let body = to_bytes(response.into_body(), 65536).await.unwrap();
        assert_eq!(&body[..], expected.data.as_ref());
    }
}
