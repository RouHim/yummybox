use crate::error::AppError;
use crate::model::NewIngredientLine;
use crate::recipe;
use base64::Engine;
use genai::adapter::AdapterKind;
use genai::resolver::{AuthData, Endpoint, ProviderConfig};

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

pub struct LlmImage {
    pub bytes: Vec<u8>,
    pub content_type: String,
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LlmProviderInfo {
    pub id: String,
    pub name: String,
    pub env_var: String,
    pub configured: bool,
    pub supports_custom_endpoint: bool,
}

#[derive(serde::Serialize)]
pub struct LlmProvidersResponse {
    pub providers: Vec<LlmProviderInfo>,
}

#[derive(serde::Serialize)]
pub struct LlmModelsResponse {
    pub models: Vec<String>,
}

// ---------------------------------------------------------------------------
// Provider detection
// ---------------------------------------------------------------------------

/// Returns the list of LLM providers and whether they are configured.
/// Providers with a configured API key env var are marked `configured: true`.
/// Ollama is always `configured: true` (no API key; local server).
/// A synthetic "custom" provider for OpenAI-compatible endpoints is appended.
pub fn list_providers() -> Vec<LlmProviderInfo> {
    let kinds: &[(AdapterKind, &str)] = &[
        (AdapterKind::OpenAI, "OpenAI"),
        (AdapterKind::Anthropic, "Anthropic"),
        (AdapterKind::Gemini, "Gemini"),
        (AdapterKind::Groq, "Groq"),
        (AdapterKind::Ollama, "Ollama"),
        (AdapterKind::DeepSeek, "DeepSeek"),
        (AdapterKind::Xai, "xAI"),
    ];

    let mut providers: Vec<LlmProviderInfo> = kinds
        .iter()
        .map(|(kind, name)| {
            let env_var = kind.default_key_env_name().unwrap_or("").to_string();
            let configured = if matches!(kind, AdapterKind::Ollama) {
                true
            } else {
                std::env::var(&env_var).is_ok()
            };
            LlmProviderInfo {
                id: kind.as_lower_str().to_string(),
                name: name.to_string(),
                env_var,
                configured,
                supports_custom_endpoint: false,
            }
        })
        .collect();

    // Append the synthetic "custom" OpenAI-compatible endpoint provider
    providers.push(LlmProviderInfo {
        id: "custom".to_string(),
        name: "Custom OpenAI-compatible".to_string(),
        env_var: String::new(),
        configured: true,
        supports_custom_endpoint: true,
    });

    providers
}

// ---------------------------------------------------------------------------
// Model listing
// ---------------------------------------------------------------------------

/// Lists the available model names for a given provider by querying the
/// provider's API.
///
/// For standard providers, auth is resolved from env vars and the default
/// endpoint is used. For `provider_id == "custom"`, `base_url` is required
/// and `api_key` is optional.
///
/// Uses a 15-second timeout to avoid hanging the UI.
pub async fn list_models(
    provider_id: &str,
    base_url: Option<&str>,
    api_key: Option<&str>,
) -> Result<Vec<String>, AppError> {
    let client = genai::Client::default();

    let (adapter_kind, provider_config) = if provider_id == "custom" {
        let base_url = base_url
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .ok_or_else(|| {
                AppError::BadRequest("base_url is required for custom provider".into())
            })?;
        let base_url = if base_url.ends_with('/') {
            base_url.to_string()
        } else {
            format!("{base_url}/")
        };
        let endpoint = Endpoint::from_owned(base_url);
        let auth = match api_key.map(|s| s.trim()).filter(|s| !s.is_empty()) {
            Some(key) => AuthData::from_single(key),
            None => AuthData::None,
        };
        (AdapterKind::OpenAI, ProviderConfig::from((endpoint, auth)))
    } else {
        let kind = AdapterKind::from_lower_str(provider_id)
            .ok_or_else(|| AppError::BadRequest(format!("unknown provider: {provider_id}")))?;
        (kind, ProviderConfig::default())
    };

    let models_fut = client.all_model_names(adapter_kind, provider_config);
    let models = tokio::time::timeout(std::time::Duration::from_secs(15), models_fut)
        .await
        .map_err(|_| {
            AppError::Llm(
                "Model listing timed out after 15 seconds".into(),
                "llm_timeout",
            )
        })?
        .map_err(map_genai_error)?;
    Ok(models)
}

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const TOOL_NAME: &str = "extract_recipe";

const SYSTEM_PROMPT: &str = "You are a recipe extraction assistant. Extract the recipe from the user's input (an image of a meal, a text description, a recipe URL, or any combination) and call the extract_recipe tool with the result. Always call the tool. If you can identify a photo URL of the finished dish from the recipe context (page text or your own knowledge for description-only inputs), provide it in the imageUrl field; otherwise omit it. When candidate dish image URLs are listed in the input, pick the most relevant one for imageUrl. Never invent a URL you are not confident exists.";

fn recipe_tool() -> genai::chat::Tool {
    genai::chat::Tool::new(TOOL_NAME)
        .with_description("Extract a structured recipe from the user's input.")
        .with_schema(serde_json::json!({
            "type": "object",
            "properties": {
                "name": { "type": "string", "description": "The recipe name" },
                "ingredients": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "name": { "type": "string", "description": "Ingredient name" },
                            "quantity": { "type": "string", "description": "Quantity e.g. '200g', '2 cups'" }
                        },
                        "required": ["name"]
                    }
                },
                "instructions": { "type": "string", "description": "Cooking instructions" },
                "imageUrl": { "type": "string", "description": "A URL of a photo of the finished dish, if one can be identified from the recipe context" }
            },
            "required": ["name", "ingredients"]
        }))
}

// ---------------------------------------------------------------------------
// User content builder
// ---------------------------------------------------------------------------

fn build_user_content(hint: Option<&str>, image: Option<&LlmImage>) -> genai::chat::MessageContent {
    let mut parts = Vec::new();
    if let Some(h) = hint.map(str::trim).filter(|s| !s.is_empty()) {
        parts.push(genai::chat::ContentPart::from_text(h));
    }
    if let Some(img) = image {
        let b64 = base64::engine::general_purpose::STANDARD.encode(&img.bytes);
        parts.push(genai::chat::ContentPart::from_binary_base64(
            &img.content_type,
            b64,
            Some("image".to_string()),
        ));
    }
    genai::chat::MessageContent::from_parts(parts)
}

// ---------------------------------------------------------------------------
// Main import function
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Shared model spec builder
// ---------------------------------------------------------------------------

fn build_model_spec(
    model: &str,
    base_url: Option<&str>,
    api_key: Option<&str>,
) -> genai::ModelSpec {
    use genai::resolver::{AuthData, Endpoint};

    if let Some(base_url) = base_url.map(|s| s.trim()).filter(|s| !s.is_empty()) {
        let base_url = if base_url.ends_with('/') {
            base_url.to_string()
        } else {
            format!("{base_url}/")
        };
        let endpoint = Endpoint::from_owned(base_url);
        let auth = match api_key.map(|s| s.trim()).filter(|s| !s.is_empty()) {
            Some(key) => AuthData::from_single(key),
            None => AuthData::None,
        };
        genai::ServiceTarget {
            endpoint,
            auth,
            model: genai::ModelIden::new(AdapterKind::OpenAI, model),
        }
        .into()
    } else {
        model.into()
    }
}

// ---------------------------------------------------------------------------
// Main import function
// ---------------------------------------------------------------------------

pub async fn import_via_llm(
    model: &str,
    hint: Option<&str>,
    image: Option<LlmImage>,
    base_url: Option<&str>,
    api_key: Option<&str>,
    has_user_image: bool,
) -> Result<recipe::ImportDraft, AppError> {
    let client = genai::Client::default();
    let user_content = build_user_content(hint, image.as_ref());

    let chat_req = genai::chat::ChatRequest::new(vec![
        genai::chat::ChatMessage::system(SYSTEM_PROMPT),
        genai::chat::ChatMessage::user(user_content),
    ])
    .with_tools(vec![recipe_tool()]);
    let model_spec = build_model_spec(model, base_url, api_key);
    let chat_fut = client.exec_chat(model_spec, chat_req, None);

    let chat_res = match tokio::time::timeout(std::time::Duration::from_secs(60), chat_fut).await {
        Ok(r) => r,
        Err(_) => {
            return Err(AppError::Llm(
                "LLM request timed out after 60 seconds".into(),
                "llm_timeout",
            ));
        }
    };
    let chat_res = chat_res.map_err(map_genai_error)?;

    let tool_calls = chat_res.into_tool_calls();
    let first = tool_calls.first().ok_or_else(|| {
        AppError::Llm(
            "could not parse a recipe from input".into(),
            "llm_parse_failed",
        )
    })?;
    build_draft_from_tool_args(&first.fn_arguments, has_user_image).await
}

// ---------------------------------------------------------------------------
// Polish instructions
// ---------------------------------------------------------------------------

const POLISH_SYSTEM_PROMPT: &str = "You are a cooking assistant. Improve the given cooking instructions for clarity, structure, and readability. Preserve the original meaning and the same language as the input. Format the result as HTML using only these tags: p, br, strong, em, b, i, ul, ol, li. Return only the improved instructions, no commentary or preamble.";

pub async fn polish_instructions(
    model: &str,
    meal_name: &str,
    ingredients: &[NewIngredientLine],
    instructions: &str,
    base_url: Option<&str>,
    api_key: Option<&str>,
) -> Result<String, AppError> {
    let client = genai::Client::default();

    let mut user_text = format!("Meal: {meal_name}\n\nIngredients:\n");
    for ing in ingredients {
        user_text.push_str(&format!("- {}\n", ing.name));
    }
    if instructions.trim().is_empty() {
        user_text.push_str("\nNo instructions provided yet.");
    } else {
        user_text.push_str(&format!("\nInstructions:\n{instructions}"));
    }

    let chat_req = genai::chat::ChatRequest::new(vec![
        genai::chat::ChatMessage::system(POLISH_SYSTEM_PROMPT),
        genai::chat::ChatMessage::user(user_text),
    ]);
    let model_spec = build_model_spec(model, base_url, api_key);
    let chat_fut = client.exec_chat(model_spec, chat_req, None);

    let chat_res = match tokio::time::timeout(std::time::Duration::from_secs(60), chat_fut).await {
        Ok(r) => r,
        Err(_) => {
            return Err(AppError::Llm(
                "LLM request timed out after 60 seconds".into(),
                "llm_timeout",
            ));
        }
    };
    let chat_res = chat_res.map_err(map_genai_error)?;

    let text = chat_res.into_first_text().ok_or_else(|| {
        AppError::Llm(
            "LLM returned no polished instructions".into(),
            "llm_parse_failed",
        )
    })?;
    let text = text.trim();
    if text.is_empty() {
        return Err(AppError::Llm(
            "LLM returned no polished instructions".into(),
            "llm_parse_failed",
        ));
    }
    Ok(recipe::sanitize_instructions(text))
}

// ---------------------------------------------------------------------------
// Error mapping
// ---------------------------------------------------------------------------

fn map_genai_error(err: genai::Error) -> AppError {
    match &err {
        genai::Error::RequiresApiKey { model_iden }
        | genai::Error::NoAuthResolver { model_iden }
        | genai::Error::NoAuthData { model_iden } => {
            let env_var = model_iden
                .adapter_kind
                .default_key_env_name()
                .unwrap_or("the provider's API key environment variable");
            AppError::Llm(
                format!(
                    "API key not configured for provider '{}': set the {} environment variable",
                    model_iden.adapter_kind, env_var
                ),
                "llm_api_key_missing",
            )
        }
        genai::Error::Resolver { model_iden, .. }
        | genai::Error::ModelMapperFailed { model_iden, .. } => AppError::Llm(
            format!("model '{}' could not be resolved: {err}", model_iden),
            "llm_model_not_found",
        ),
        _ => AppError::Llm(format!("LLM request failed: {err}"), "llm_request_failed"),
    }
}

// ---------------------------------------------------------------------------
// Tool output → ImportDraft
// ---------------------------------------------------------------------------

#[derive(serde::Deserialize)]
struct LlmRecipeDraft {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    ingredients: Vec<LlmIngredient>,
    #[serde(default)]
    instructions: Option<String>,
    #[serde(default)]
    #[serde(rename = "imageUrl")]
    image_url: Option<String>,
}

#[derive(serde::Deserialize)]
struct LlmIngredient {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    quantity: Option<String>,
}

async fn build_draft_from_tool_args(
    args: &serde_json::Value,
    has_user_image: bool,
) -> Result<recipe::ImportDraft, AppError> {
    let draft: LlmRecipeDraft = serde_json::from_value(args.clone()).map_err(|e| {
        AppError::Llm(
            format!("could not parse a recipe from input: {e}"),
            "llm_parse_failed",
        )
    })?;
    let name = draft.name.map(|s| s.trim().to_string()).unwrap_or_default();
    let ingredients: Vec<NewIngredientLine> = draft
        .ingredients
        .into_iter()
        .map(|i| NewIngredientLine {
            name: i.name.map(|s| s.trim().to_string()).unwrap_or_default(),
            quantity: i.quantity.filter(|s| !s.trim().is_empty()),
        })
        .collect();
    if name.is_empty() || ingredients.iter().all(|i| i.name.is_empty()) {
        return Err(AppError::Llm(
            "could not parse a recipe from input".into(),
            "llm_parse_failed",
        ));
    }
    let ingredients: Vec<_> = ingredients
        .into_iter()
        .filter(|i| !i.name.is_empty())
        .collect();

    let image_url = draft
        .image_url
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());

    // FR-005: skip download when the user uploaded an image.
    let image_base64 = if has_user_image {
        None
    } else {
        try_download_llm_image(image_url).await
    };

    Ok(recipe::ImportDraft {
        name,
        ingredients,
        instructions: recipe::sanitize_instructions(&draft.instructions.unwrap_or_default()),
        image_base64,
    })
}

/// Best-effort: download `url`, convert to JPEG, base64-encode.
/// Returns `None` on any failure (network, non-image content, decode).
/// `url` is already trimmed and non-empty; `None` → no download attempted.
async fn try_download_llm_image(url: Option<&str>) -> Option<String> {
    let url = url?; // None → no download
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .connect_timeout(std::time::Duration::from_secs(10))
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36")
        .build()
        .ok()?;
    let jpeg_bytes = recipe::try_download_image(&client, url).await?;
    let b64 = base64::engine::general_purpose::STANDARD.encode(&jpeg_bytes);
    Some(b64)
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn given_valid_args_when_build_model_spec_then_succeeds() {
        // Standard model name (no custom endpoint)
        let spec = build_model_spec("gpt-4o-mini", None, None);
        let debug = format!("{:?}", spec);
        assert!(debug.contains("gpt-4o-mini"));

        // Custom endpoint with api key
        let spec = build_model_spec(
            "local-model",
            Some("http://localhost:8080/v1/"),
            Some("sk-123"),
        );
        let debug = format!("{:?}", spec);
        assert!(debug.contains("localhost:8080"));
        assert!(debug.contains("local-model"));

        // Custom endpoint without trailing slash gets normalized
        let spec = build_model_spec("llama3", Some("http://127.0.0.1:11434/v1"), None);
        let debug = format!("{:?}", spec);
        assert!(debug.contains("127.0.0.1:11434"));
    }

    #[tokio::test]
    async fn given_empty_name_when_build_draft_then_422() {
        let args = serde_json::json!({"name": "", "ingredients": [{"name": "x"}]});
        let result = build_draft_from_tool_args(&args, false).await;
        assert!(result.is_err());
        match result {
            Err(AppError::Llm(msg, code)) => {
                assert!(msg.contains("could not parse a recipe from input"));
                assert_eq!(code, "llm_parse_failed");
            }
            _ => panic!("expected Llm error, got {:?}", result),
        }
    }

    #[tokio::test]
    async fn given_no_ingredients_when_build_draft_then_422() {
        let args = serde_json::json!({"name": "Pasta", "ingredients": []});
        let result = build_draft_from_tool_args(&args, false).await;
        assert!(result.is_err());
        match result {
            Err(AppError::Llm(msg, code)) => {
                assert!(msg.contains("could not parse a recipe from input"));
                assert_eq!(code, "llm_parse_failed");
            }
            _ => panic!("expected Llm error, got {:?}", result),
        }
    }

    #[tokio::test]
    async fn given_all_empty_ingredient_names_when_build_draft_then_422() {
        let args =
            serde_json::json!({"name": "Pasta", "ingredients": [{"name": ""}, {"name": "  "}]});
        let result = build_draft_from_tool_args(&args, false).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn given_valid_args_when_build_draft_then_returns_draft() {
        let args = serde_json::json!({
            "name": "Pasta",
            "ingredients": [{"name": "flour", "quantity": "200g"}],
            "instructions": "Boil"
        });
        let draft = build_draft_from_tool_args(&args, false)
            .await
            .expect("should succeed");
        assert_eq!(draft.name, "Pasta");
        assert_eq!(draft.ingredients.len(), 1);
        assert_eq!(draft.ingredients[0].name, "flour");
        assert_eq!(draft.ingredients[0].quantity.as_deref(), Some("200g"));
        assert_eq!(draft.instructions, "Boil");
        assert!(draft.image_base64.is_none());
    }

    #[tokio::test]
    async fn given_missing_instructions_when_build_draft_then_empty_string() {
        let args = serde_json::json!({
            "name": "Pasta",
            "ingredients": [{"name": "flour"}]
        });
        let draft = build_draft_from_tool_args(&args, false)
            .await
            .expect("should succeed");
        assert_eq!(draft.instructions, "");
    }

    #[tokio::test]
    async fn given_quantity_blank_when_build_draft_then_quantity_none() {
        let args = serde_json::json!({
            "name": "Pasta",
            "ingredients": [{"name": "flour", "quantity": "  "}]
        });
        let draft = build_draft_from_tool_args(&args, false)
            .await
            .expect("should succeed");
        assert_eq!(draft.ingredients[0].quantity, None);
    }

    #[test]
    fn list_providers_includes_all_providers() {
        let providers = list_providers();
        let ids: Vec<&str> = providers.iter().map(|p| p.id.as_str()).collect();
        for expected in &[
            "openai",
            "anthropic",
            "gemini",
            "groq",
            "ollama",
            "deepseek",
            "xai",
            "custom",
        ] {
            assert!(
                ids.contains(expected),
                "expected provider '{expected}' not found in: {ids:?}"
            );
        }
    }

    #[test]
    fn list_providers_ollama_always_configured() {
        let providers = list_providers();
        let ollama = providers
            .iter()
            .find(|p| p.id == "ollama")
            .expect("ollama should exist");
        assert!(ollama.configured, "ollama should always be configured");
    }

    #[test]
    fn list_providers_custom_supports_custom_endpoint() {
        let providers = list_providers();
        let custom = providers
            .iter()
            .find(|p| p.id == "custom")
            .expect("custom should exist");
        assert!(custom.supports_custom_endpoint);
        assert!(custom.env_var.is_empty());
    }

    #[tokio::test]
    async fn given_image_url_when_no_user_image_then_downloads_image() {
        // 1x1 white JPEG (valid, will be re-encoded by convert_to_jpeg).
        let jpeg_bytes: &[u8] = &[
            0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10, 0x4A, 0x46, 0x49, 0x46, 0x00, 0x01, 0x01, 0x00,
            0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0xFF, 0xDB, 0x00, 0x43, 0x00, 0x02, 0x01, 0x01,
            0x01, 0x01, 0x01, 0x02, 0x01, 0x01, 0x01, 0x02, 0x02, 0x02, 0x02, 0x02, 0x02, 0x04,
            0x03, 0x02, 0x02, 0x02, 0x02, 0x05, 0x04, 0x04, 0x03, 0x04, 0x06, 0x05, 0x06, 0x06,
            0x06, 0x05, 0x06, 0x06, 0x06, 0x07, 0x09, 0x08, 0x06, 0x07, 0x09, 0x07, 0x06, 0x06,
            0x08, 0x0B, 0x08, 0x09, 0x0A, 0x0A, 0x0A, 0x0A, 0x0A, 0x06, 0x08, 0x0B, 0x0C, 0x0B,
            0x0A, 0x0C, 0x09, 0x0A, 0x0A, 0x0A, 0xFF, 0xDB, 0x00, 0x43, 0x01, 0x02, 0x02, 0x02,
            0x02, 0x02, 0x02, 0x05, 0x03, 0x03, 0x05, 0x0A, 0x07, 0x06, 0x07, 0x0A, 0x0A, 0x0A,
            0x0A, 0x0A, 0x0A, 0x0A, 0x0A, 0x0A, 0x0A, 0x0A, 0x0A, 0x0A, 0x0A, 0x0A, 0x0A, 0x0A,
            0x0A, 0x0A, 0x0A, 0x0A, 0x0A, 0x0A, 0x0A, 0x0A, 0x0A, 0x0A, 0x0A, 0x0A, 0x0A, 0x0A,
            0x0A, 0x0A, 0x0A, 0x0A, 0x0A, 0x0A, 0x0A, 0x0A, 0x0A, 0x0A, 0x0A, 0x0A, 0x0A, 0x0A,
            0x0A, 0x0A, 0x0A, 0x0A, 0x0A, 0x0A, 0xFF, 0xC0, 0x00, 0x11, 0x08, 0x00, 0x01, 0x00,
            0x01, 0x03, 0x01, 0x22, 0x00, 0x02, 0x11, 0x01, 0x03, 0x11, 0x01, 0xFF, 0xC4, 0x00,
            0x1F, 0x00, 0x00, 0x01, 0x05, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09,
            0x0A, 0x0B, 0xFF, 0xC4, 0x00, 0xB5, 0x10, 0x00, 0x02, 0x01, 0x03, 0x03, 0x02, 0x04,
            0x03, 0x05, 0x05, 0x04, 0x04, 0x00, 0x00, 0x01, 0x7D, 0x01, 0x02, 0x03, 0x00, 0x04,
            0x11, 0x05, 0x12, 0x21, 0x31, 0x41, 0x06, 0x13, 0x51, 0x61, 0x07, 0x22, 0x71, 0x14,
            0x32, 0x81, 0x91, 0xA1, 0x08, 0x23, 0x42, 0xB1, 0xC1, 0x15, 0x52, 0xD1, 0xF0, 0x24,
            0x33, 0x62, 0x72, 0x82, 0x09, 0x0A, 0x16, 0x17, 0x18, 0x19, 0x1A, 0x25, 0x26, 0x27,
            0x28, 0x29, 0x2A, 0x34, 0x35, 0x36, 0x37, 0x38, 0x39, 0x3A, 0x43, 0x44, 0x45, 0x46,
            0x47, 0x48, 0x49, 0x4A, 0x53, 0x54, 0x55, 0x56, 0x57, 0x58, 0x59, 0x5A, 0x63, 0x64,
            0x65, 0x66, 0x67, 0x68, 0x69, 0x6A, 0x73, 0x74, 0x75, 0x76, 0x77, 0x78, 0x79, 0x7A,
            0x83, 0x84, 0x85, 0x86, 0x87, 0x88, 0x89, 0x8A, 0x92, 0x93, 0x94, 0x95, 0x96, 0x97,
            0x98, 0x99, 0x9A, 0xA2, 0xA3, 0xA4, 0xA5, 0xA6, 0xA7, 0xA8, 0xA9, 0xAA, 0xB2, 0xB3,
            0xB4, 0xB5, 0xB6, 0xB7, 0xB8, 0xB9, 0xBA, 0xC2, 0xC3, 0xC4, 0xC5, 0xC6, 0xC7, 0xC8,
            0xC9, 0xCA, 0xD2, 0xD3, 0xD4, 0xD5, 0xD6, 0xD7, 0xD8, 0xD9, 0xDA, 0xE1, 0xE2, 0xE3,
            0xE4, 0xE5, 0xE6, 0xE7, 0xE8, 0xE9, 0xEA, 0xF1, 0xF2, 0xF3, 0xF4, 0xF5, 0xF6, 0xF7,
            0xF8, 0xF9, 0xFA, 0xFF, 0xC4, 0x00, 0x1F, 0x01, 0x00, 0x03, 0x01, 0x01, 0x01, 0x01,
            0x01, 0x01, 0x01, 0x01, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x02,
            0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A, 0x0B, 0xFF, 0xC4, 0x00, 0xB5, 0x11,
            0x00, 0x02, 0x01, 0x02, 0x04, 0x04, 0x03, 0x04, 0x07, 0x05, 0x04, 0x04, 0x00, 0x01,
            0x02, 0x77, 0x00, 0x01, 0x02, 0x03, 0x11, 0x04, 0x05, 0x21, 0x31, 0x06, 0x12, 0x41,
            0x51, 0x07, 0x61, 0x71, 0x13, 0x22, 0x32, 0x81, 0x08, 0x14, 0x42, 0x91, 0xA1, 0xB1,
            0xC1, 0x09, 0x23, 0x33, 0x52, 0xF0, 0x15, 0x62, 0x72, 0xD1, 0x0A, 0x16, 0x24, 0x34,
            0xE1, 0x25, 0xF1, 0x17, 0x18, 0x19, 0x1A, 0x26, 0x27, 0x28, 0x29, 0x2A, 0x35, 0x36,
            0x37, 0x38, 0x39, 0x3A, 0x43, 0x44, 0x45, 0x46, 0x47, 0x48, 0x49, 0x4A, 0x53, 0x54,
            0x55, 0x56, 0x57, 0x58, 0x59, 0x5A, 0x63, 0x64, 0x65, 0x66, 0x67, 0x68, 0x69, 0x6A,
            0x73, 0x74, 0x75, 0x76, 0x77, 0x78, 0x79, 0x7A, 0x82, 0x83, 0x84, 0x85, 0x86, 0x87,
            0x88, 0x89, 0x8A, 0x92, 0x93, 0x94, 0x95, 0x96, 0x97, 0x98, 0x99, 0x9A, 0xA2, 0xA3,
            0xA4, 0xA5, 0xA6, 0xA7, 0xA8, 0xA9, 0xAA, 0xB2, 0xB3, 0xB4, 0xB5, 0xB6, 0xB7, 0xB8,
            0xB9, 0xBA, 0xC2, 0xC3, 0xC4, 0xC5, 0xC6, 0xC7, 0xC8, 0xC9, 0xCA, 0xD2, 0xD3, 0xD4,
            0xD5, 0xD6, 0xD7, 0xD8, 0xD9, 0xDA, 0xE2, 0xE3, 0xE4, 0xE5, 0xE6, 0xE7, 0xE8, 0xE9,
            0xEA, 0xF2, 0xF3, 0xF4, 0xF5, 0xF6, 0xF7, 0xF8, 0xF9, 0xFA, 0xFF, 0xDA, 0x00, 0x0C,
            0x03, 0x01, 0x00, 0x02, 0x11, 0x03, 0x11, 0x00, 0x3F, 0x00, 0xFD, 0xFC, 0xA2, 0x8A,
            0x28, 0x03, 0xFF, 0xD9,
        ];
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let body = jpeg_bytes.to_vec();
        tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            use tokio::io::AsyncWriteExt;
            let resp = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: image/jpeg\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            let _ = stream.write_all(resp.as_bytes()).await;
            let _ = stream.write_all(&body).await;
        });
        let url = format!("http://127.0.0.1:{port}/img.jpg");
        let args = serde_json::json!({
            "name": "Soup",
            "ingredients": [{"name": "water"}],
            "imageUrl": url
        });
        let draft = build_draft_from_tool_args(&args, false)
            .await
            .expect("should succeed");
        assert!(draft.image_base64.is_some(), "image should be downloaded");
        // decode the base64 -> valid JPEG (starts with 0xFF 0xD8 0xFF)
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(draft.image_base64.unwrap())
            .unwrap();
        assert_eq!(&decoded[0..3], &[0xFF, 0xD8, 0xFF], "should be valid JPEG");
    }

    #[tokio::test]
    async fn given_unreachable_image_url_when_build_draft_then_no_error_no_image() {
        let args = serde_json::json!({
            "name": "Soup",
            "ingredients": [{"name": "water"}],
            "imageUrl": "http://127.0.0.1:1/nope.jpg"
        });
        let draft = build_draft_from_tool_args(&args, false)
            .await
            .expect("should succeed");
        assert!(draft.image_base64.is_none());
    }

    #[tokio::test]
    async fn given_user_image_when_image_url_present_then_no_download() {
        // Use a valid server URL; if skip is broken the download would succeed
        // and the test would fail, proving the skip path works.
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        // Serve a real JPEG so a faulty skip would download it
        let jpeg_bytes: Vec<u8> = vec![
            0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10, 0x4A, 0x46, 0x49, 0x46, 0x00, 0x01, 0x01, 0x00,
            0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0xFF, 0xDB, 0x00, 0x43, 0x00, 0x02, 0x01, 0x01,
            0x01, 0x01, 0x01, 0x02, 0x01, 0x01, 0x01, 0x02, 0x02, 0x02, 0x02, 0x02, 0x02, 0x04,
            0x03, 0x02, 0x02, 0x02, 0x02, 0x05, 0x04, 0x04, 0x03, 0x04, 0x06, 0x05, 0x06, 0x06,
            0x06, 0x05, 0x06, 0x06, 0x06, 0x07, 0x09, 0x08, 0x06, 0x07, 0x09, 0x07, 0x06, 0x06,
            0x08, 0x0B, 0x08, 0x09, 0x0A, 0x0A, 0x0A, 0x0A, 0x0A, 0x06, 0x08, 0x0B, 0x0C, 0x0B,
            0x0A, 0x0C, 0x09, 0x0A, 0x0A, 0x0A, 0xFF, 0xDB, 0x00, 0x43, 0x01, 0x02, 0x02, 0x02,
            0x02, 0x02, 0x05, 0x03, 0x03, 0x05, 0x0A, 0x07, 0x06, 0x07, 0x0A, 0x0A, 0x0A, 0x0A,
            0x0A, 0x0A, 0x0A, 0x0A, 0x0A, 0x0A, 0x0A, 0x0A, 0x0A, 0x0A, 0x0A, 0x0A, 0x0A, 0x0A,
            0x0A, 0x0A, 0x0A, 0x0A, 0x0A, 0x0A, 0x0A, 0x0A, 0x0A, 0x0A, 0x0A, 0x0A, 0x0A, 0x0A,
            0x0A, 0x0A, 0x0A, 0x0A, 0x0A, 0x0A, 0x0A, 0x0A, 0x0A, 0x0A, 0x0A, 0x0A, 0x0A, 0x0A,
            0x0A, 0x0A, 0x0A, 0x0A, 0x0A, 0x0A, 0xFF, 0xC0, 0x00, 0x11, 0x08, 0x00, 0x01, 0x00,
            0x01, 0x03, 0x01, 0x22, 0x00, 0x02, 0x11, 0x01, 0x03, 0x11, 0x01, 0xFF, 0xC4, 0x00,
            0x1F, 0x00, 0x00, 0x01, 0x05, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09,
            0x0A, 0x0B, 0xFF, 0xC4, 0x00, 0xB5, 0x10, 0x00, 0x02, 0x01, 0x03, 0x03, 0x02, 0x04,
            0x03, 0x05, 0x05, 0x04, 0x04, 0x00, 0x00, 0x01, 0x7D, 0x01, 0x02, 0x03, 0x00, 0x04,
            0x11, 0x05, 0x12, 0x21, 0x31, 0x41, 0x06, 0x13, 0x51, 0x61, 0x07, 0x22, 0x71, 0x14,
            0x32, 0x81, 0x91, 0xA1, 0x08, 0x23, 0x42, 0xB1, 0xC1, 0x15, 0x52, 0xD1, 0xF0, 0x24,
            0x33, 0x62, 0x72, 0x82, 0x09, 0x0A, 0x16, 0x17, 0x18, 0x19, 0x1A, 0x25, 0x26, 0x27,
            0x28, 0x29, 0x2A, 0x34, 0x35, 0x36, 0x37, 0x38, 0x39, 0x3A, 0x43, 0x44, 0x45, 0x46,
            0x47, 0x48, 0x49, 0x4A, 0x53, 0x54, 0x55, 0x56, 0x57, 0x58, 0x59, 0x5A, 0x63, 0x64,
            0x65, 0x66, 0x67, 0x68, 0x69, 0x6A, 0x73, 0x74, 0x75, 0x76, 0x77, 0x78, 0x79, 0x7A,
            0x83, 0x84, 0x85, 0x86, 0x87, 0x88, 0x89, 0x8A, 0x92, 0x93, 0x94, 0x95, 0x96, 0x97,
            0x98, 0x99, 0x9A, 0xA2, 0xA3, 0xA4, 0xA5, 0xA6, 0xA7, 0xA8, 0xA9, 0xAA, 0xB2, 0xB3,
            0xB4, 0xB5, 0xB6, 0xB7, 0xB8, 0xB9, 0xBA, 0xC2, 0xC3, 0xC4, 0xC5, 0xC6, 0xC7, 0xC8,
            0xC9, 0xCA, 0xD2, 0xD3, 0xD4, 0xD5, 0xD6, 0xD7, 0xD8, 0xD9, 0xDA, 0xE1, 0xE2, 0xE3,
            0xE4, 0xE5, 0xE6, 0xE7, 0xE8, 0xE9, 0xEA, 0xF1, 0xF2, 0xF3, 0xF4, 0xF5, 0xF6, 0xF7,
            0xF8, 0xF9, 0xFA, 0xFF, 0xC4, 0x00, 0x1F, 0x01, 0x00, 0x03, 0x01, 0x01, 0x01, 0x01,
            0x01, 0x01, 0x01, 0x01, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x02,
            0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A, 0x0B, 0xFF, 0xC4, 0x00, 0xB5, 0x11,
            0x00, 0x02, 0x01, 0x02, 0x04, 0x04, 0x03, 0x04, 0x07, 0x05, 0x04, 0x04, 0x00, 0x01,
            0x02, 0x77, 0x00, 0x01, 0x02, 0x03, 0x11, 0x04, 0x05, 0x21, 0x31, 0x06, 0x12, 0x41,
            0x51, 0x07, 0x61, 0x71, 0x13, 0x22, 0x32, 0x81, 0x08, 0x14, 0x42, 0x91, 0xA1, 0xB1,
            0xC1, 0x09, 0x23, 0x33, 0x52, 0xF0, 0x15, 0x62, 0x72, 0xD1, 0x0A, 0x16, 0x24, 0x34,
            0xE1, 0x25, 0xF1, 0x17, 0x18, 0x19, 0x1A, 0x26, 0x27, 0x28, 0x29, 0x2A, 0x35, 0x36,
            0x37, 0x38, 0x39, 0x3A, 0x43, 0x44, 0x45, 0x46, 0x47, 0x48, 0x49, 0x4A, 0x53, 0x54,
            0x55, 0x56, 0x57, 0x58, 0x59, 0x5A, 0x63, 0x64, 0x65, 0x66, 0x67, 0x68, 0x69, 0x6A,
            0x73, 0x74, 0x75, 0x76, 0x77, 0x78, 0x79, 0x7A, 0x82, 0x83, 0x84, 0x85, 0x86, 0x87,
            0x88, 0x89, 0x8A, 0x92, 0x93, 0x94, 0x95, 0x96, 0x97, 0x98, 0x99, 0x9A, 0xA2, 0xA3,
            0xA4, 0xA5, 0xA6, 0xA7, 0xA8, 0xA9, 0xAA, 0xB2, 0xB3, 0xB4, 0xB5, 0xB6, 0xB7, 0xB8,
            0xB9, 0xBA, 0xC2, 0xC3, 0xC4, 0xC5, 0xC6, 0xC7, 0xC8, 0xC9, 0xCA, 0xD2, 0xD3, 0xD4,
            0xD5, 0xD6, 0xD7, 0xD8, 0xD9, 0xDA, 0xE2, 0xE3, 0xE4, 0xE5, 0xE6, 0xE7, 0xE8, 0xE9,
            0xEA, 0xF2, 0xF3, 0xF4, 0xF5, 0xF6, 0xF7, 0xF8, 0xF9, 0xFA, 0xFF, 0xDA, 0x00, 0x0C,
            0x03, 0x01, 0x00, 0x02, 0x11, 0x03, 0x11, 0x00, 0x3F, 0x00, 0xFD, 0xFC, 0xA2, 0x8A,
            0x28, 0x03, 0xFF, 0xD9,
        ];
        let body = jpeg_bytes.clone();
        tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            use tokio::io::AsyncWriteExt;
            let resp = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: image/jpeg\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            let _ = stream.write_all(resp.as_bytes()).await;
            let _ = stream.write_all(&body).await;
        });
        let url = format!("http://127.0.0.1:{port}/img.jpg");
        let args = serde_json::json!({
            "name": "Soup",
            "ingredients": [{"name": "water"}],
            "imageUrl": url
        });
        let draft = build_draft_from_tool_args(&args, true)
            .await
            .expect("should succeed");
        assert!(
            draft.image_base64.is_none(),
            "user image should take precedence, no download"
        );
    }
}
