use crate::error::AppError;
use crate::model::NewIngredientLine;
use crate::recipe;
use base64::Engine;
use genai::adapter::AdapterKind;

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
    /// Whether the provider's API-key environment variable holds a value.
    pub env_key_set: bool,
    /// Whether the provider can be used right now.
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
// Resolved target
// ---------------------------------------------------------------------------

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
        AdapterKind::from_lower_str(self.provider_id)
            .ok_or_else(|| AppError::Validation(format!("unknown provider: {}", self.provider_id)))
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
    ///
    /// The custom endpoint is documented as keyless-capable, but genai's
    /// OpenAI adapter requires a single key value and fails the whole call
    /// with `ResolverAuthDataNotSingleValue` for `AuthData::None` before any
    /// request leaves. A keyless target therefore gets a placeholder, the same
    /// way genai's own keyless adapters (Ollama) default to one, so the
    /// request reaches the endpoint instead of surfacing as
    /// `llm_model_not_found`.
    fn model_spec(&self, model: &str) -> Result<genai::ModelSpec, AppError> {
        if self.provider_id == PROVIDER_CUSTOM {
            return Ok(genai::ServiceTarget {
                endpoint: self.custom_endpoint()?,
                auth: self
                    .auth()
                    .unwrap_or_else(|| genai::resolver::AuthData::from_single("no-key")),
                model: genai::ModelIden::new(AdapterKind::OpenAI, model),
            }
            .into());
        }
        Ok(genai::ModelIden::new(self.adapter_kind()?, model).into())
    }

    /// Adapter and provider config for model listing.
    fn listing_config(&self) -> Result<(AdapterKind, genai::resolver::ProviderConfig), AppError> {
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

// ---------------------------------------------------------------------------
// Provider detection
// ---------------------------------------------------------------------------

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
            // A present-but-empty or whitespace-only variable counts as unset,
            // matching `settings::provider_env_key`, so both endpoints agree.
            let env_key_set = !env_var.is_empty()
                && std::env::var(&env_var).is_ok_and(|value| !value.trim().is_empty());
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

// ---------------------------------------------------------------------------
// Model listing
// ---------------------------------------------------------------------------

/// Lists the available model names for a given provider by querying the
/// provider's API.
///
/// For standard providers, auth is resolved from the target's key or env vars
/// and the default endpoint is used. For the custom provider, the target's
/// base URL is required and its key is optional.
///
/// Uses a 15-second timeout to avoid hanging the UI.
pub async fn list_models(target: &LlmTarget<'_>) -> Result<Vec<String>, AppError> {
    require_api_key(target)?;
    let client = genai::Client::default();
    let (adapter_kind, provider_config) = target.listing_config()?;

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

/// Timeout for LLM chat requests (import, generate, polish).
const LLM_CHAT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(60);

const SYSTEM_PROMPT: &str = "You are a recipe extraction assistant. Extract the recipe from the user's input (one or more images of a meal, a text description, a recipe URL, or any combination) and call the extract_recipe tool with the result. When the input contains multiple images — for example the front and back of a recipe card, or several pages of a recipe — treat all of them as parts of ONE recipe and merge their content into a single complete recipe: take the dish name and any dish photo from whichever image shows them, and combine ingredients and instructions from all images without duplicating entries. Always call the tool. If you can identify a photo URL of the finished dish from the recipe context (page text or your own knowledge for description-only inputs), provide it in the imageUrl field; otherwise omit it. When candidate dish image URLs are listed in the input, pick the most relevant one for imageUrl. Never invent a URL you are not confident exists.";

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
                "portion": { "type": "integer", "description": "Number of servings the recipe yields, e.g. 4" },
                "imageUrl": { "type": "string", "description": "A URL of a photo of the finished dish, if one can be identified from the recipe context" }
            },
            "required": ["name", "ingredients"]
        }))
}

// ---------------------------------------------------------------------------
// User content builder
// ---------------------------------------------------------------------------

fn build_user_content(hint: Option<&str>, images: &[LlmImage]) -> genai::chat::MessageContent {
    let mut parts = Vec::new();
    if let Some(h) = hint.map(str::trim).filter(|s| !s.is_empty()) {
        parts.push(genai::chat::ContentPart::from_text(h));
    }
    for img in images {
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

pub async fn import_via_llm(
    target: &LlmTarget<'_>,
    model: &str,
    hint: Option<&str>,
    images: Vec<LlmImage>,
    skip_image_download: bool,
) -> Result<recipe::ImportDraft, AppError> {
    require_api_key(target)?;
    let client = target.client();
    let user_content = build_user_content(hint, &images);

    let chat_req = genai::chat::ChatRequest::new(vec![
        genai::chat::ChatMessage::system(SYSTEM_PROMPT),
        genai::chat::ChatMessage::user(user_content),
    ])
    .with_tools(vec![recipe_tool()]);
    let model_spec = target.model_spec(model)?;
    let chat_fut = client.exec_chat(model_spec, chat_req, None);

    let chat_res = match tokio::time::timeout(LLM_CHAT_TIMEOUT, chat_fut).await {
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
    build_draft_from_tool_args(&first.fn_arguments, skip_image_download).await
}

// ---------------------------------------------------------------------------
// Generate meal (on-the-fly from ingredients / photos)
// ---------------------------------------------------------------------------

const GENERATE_SYSTEM_PROMPT: &str = "You are a creative cooking assistant. Create a recipe from the user's available ingredients (a text list, photos, or both). The recipe must primarily use the provided ingredients; you may add only staple seasonings such as salt, pepper, oil, herbs, and spices. Preserve the exact quantities the user specified; assign plausible quantities to ingredients that have none. If an ingredient appears in both the text list and the photos, list it once, using the quantity from the text list. Respond in the same language as the user's input. Never invent an image URL; always leave the imageUrl field empty. Call the extract_recipe tool with the result. Always call the tool.";

/// Generate a complete recipe draft from an ingredient list and/or photos.
/// Same plumbing as `import_via_llm` (tool call, 60s timeout, error mapping).
/// The draft never downloads a dish photo: the dish is created from scratch,
/// so any `imageUrl` the model returns is necessarily invented (SSRF guard),
/// and user photos are sent to the model directly.
pub async fn generate_meal_via_llm(
    target: &LlmTarget<'_>,
    model: &str,
    ingredients: Option<&str>,
    images: Vec<LlmImage>,
) -> Result<recipe::ImportDraft, AppError> {
    require_api_key(target)?;
    let client = target.client();
    let user_content = build_user_content(ingredients, &images);

    let chat_req = genai::chat::ChatRequest::new(vec![
        genai::chat::ChatMessage::system(GENERATE_SYSTEM_PROMPT),
        genai::chat::ChatMessage::user(user_content),
    ])
    .with_tools(vec![recipe_tool()]);
    let model_spec = target.model_spec(model)?;
    let chat_fut = client.exec_chat(model_spec, chat_req, None);

    let chat_res = match tokio::time::timeout(LLM_CHAT_TIMEOUT, chat_fut).await {
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
    // The dish is created from scratch, so any `imageUrl` the model returns is
    // necessarily invented; never fetch it (SSRF guard).
    build_draft_from_tool_args(&first.fn_arguments, true).await
}

// ---------------------------------------------------------------------------
// Polish instructions
// ---------------------------------------------------------------------------

const POLISH_SYSTEM_PROMPT: &str = "You are a cooking assistant. Improve the given cooking instructions for clarity, structure, and readability. Preserve the original meaning and the same language as the input. Format the result as HTML using only these tags: p, br, strong, em, b, i, ul, ol, li. Return only the improved instructions, no commentary or preamble.";

pub async fn polish_instructions(
    target: &LlmTarget<'_>,
    model: &str,
    meal_name: &str,
    ingredients: &[NewIngredientLine],
    instructions: &str,
) -> Result<String, AppError> {
    require_api_key(target)?;
    let client = target.client();

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
    let model_spec = target.model_spec(model)?;
    let chat_fut = client.exec_chat(model_spec, chat_req, None);

    let chat_res = match tokio::time::timeout(LLM_CHAT_TIMEOUT, chat_fut).await {
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
        | genai::Error::NoAuthData { model_iden } => api_key_missing_error(
            &model_iden.adapter_kind.to_string(),
            model_iden.adapter_kind.default_key_env_name(),
        ),
        genai::Error::Resolver { model_iden, .. }
        | genai::Error::ModelMapperFailed { model_iden, .. } => AppError::Llm(
            format!("model '{}' could not be resolved: {err}", model_iden),
            "llm_model_not_found",
        ),
        _ => AppError::Llm(format!("LLM request failed: {err}"), "llm_request_failed"),
    }
}

/// The actionable error for a provider that needs a key but has none. The
/// sentence names the provider's API-key environment variable when it has one.
fn api_key_missing_error(provider: &str, env_var: Option<&str>) -> AppError {
    let env_var = env_var.unwrap_or("the provider's API key environment variable");
    AppError::Llm(
        format!(
            "API key not configured for provider '{provider}': store one in Settings or set the {env_var} environment variable"
        ),
        "llm_api_key_missing",
    )
}

/// Refuse to run keyless against a provider that needs a key. genai swallows
/// the unresolved `FromEnv` key, the request leaves without an Authorization
/// header, and the provider's 401 would surface as an opaque
/// `llm_request_failed` (on the chat paths, a resolver error that blames the
/// model instead). Answering `llm_api_key_missing` up front keeps a keyless
/// install on the same actionable error whether the key is stored, inherited
/// or missing, and keeps the request from leaving at all.
///
/// Callers that spend something before the LLM call (the bare-URL hint fetch
/// in `import`) call this themselves, so the missing key is reported before
/// that cost.
pub(crate) fn require_api_key(target: &LlmTarget<'_>) -> Result<(), AppError> {
    if needs_no_api_key(target.provider_id) || target.auth().is_some() {
        return Ok(());
    }
    let env_var = AdapterKind::from_lower_str(target.provider_id)
        .and_then(|kind| kind.default_key_env_name());
    Err(api_key_missing_error(target.provider_id, env_var))
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
    #[serde(default)]
    portion: Option<i32>,
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
    skip_image_download: bool,
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

    // Skip the download when the user uploaded an image (FR-005) or when the
    // caller must not fetch an LLM-supplied URL (generate flow, SSRF guard).
    let image_base64 = if skip_image_download {
        None
    } else {
        try_download_llm_image(image_url).await
    };

    Ok(recipe::ImportDraft {
        name,
        ingredients,
        instructions: recipe::sanitize_instructions(&draft.instructions.unwrap_or_default()),
        image_base64,
        portions: draft.portion,
        source_url: None,
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
        let spec = target.model_spec("local-model").expect("model spec");
        let debug = format!("{:?}", spec);
        assert!(debug.contains("localhost:8080/v1/"));
        assert!(debug.contains("local-model"));
        // The key must travel with the spec. `AuthData`'s Debug redacts it, so
        // read it back instead of searching the debug output.
        let genai::ModelSpec::Target(service_target) = spec else {
            panic!("custom provider must produce a fully resolved service target");
        };
        assert_eq!(
            service_target.auth.single_key_value().expect("single key"),
            "sk-123"
        );
    }

    #[test]
    fn given_custom_provider_without_base_url_when_model_spec_then_rejected() {
        let target = LlmTarget {
            provider_id: PROVIDER_CUSTOM,
            base_url: None,
            api_key: None,
        };
        let err = target.model_spec("local-model").expect_err("must reject");
        assert!(
            err.to_string().contains("customBaseUrl must be set"),
            "{err}"
        );
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
    fn given_keyless_ollama_when_require_api_key_then_accepted() {
        // `require_api_key` is the single gate in front of every AI entry
        // point, and Ollama is exempt from it: dropping that half would turn
        // every Ollama flow into a 400 `llm_api_key_missing`.
        let target = LlmTarget {
            provider_id: "ollama",
            base_url: None,
            api_key: None,
        };
        require_api_key(&target).expect("a keyless ollama target needs no key");
    }

    #[test]
    fn given_keyless_openai_when_require_api_key_then_llm_api_key_missing() {
        // The keyless half of the gate: a provider that needs a key must be
        // refused before the request leaves, with the code the UI acts on.
        let target = LlmTarget {
            provider_id: "openai",
            base_url: None,
            api_key: None,
        };
        let err = require_api_key(&target).expect_err("must reject a keyless openai target");
        match err {
            AppError::Llm(message, code) => {
                assert_eq!(code, "llm_api_key_missing");
                assert!(message.contains("openai"), "{message}");
            }
            other => panic!("expected an Llm error, got {other:?}"),
        }
    }

    #[test]
    fn given_key_when_target_then_auth_data_holds_it() {
        let target = LlmTarget {
            provider_id: "openai",
            base_url: None,
            api_key: Some("sk-stored"),
        };
        // `AuthData`'s Debug redacts the key, so read the value back.
        let key = target
            .auth()
            .expect("auth data")
            .single_key_value()
            .expect("single key");
        assert_eq!(key, "sk-stored");
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
            "name": "Blank Qty",
            "ingredients": [{"name": "flour", "quantity": "  "}]
        });
        let draft = build_draft_from_tool_args(&args, false)
            .await
            .expect("should succeed");
        assert_eq!(draft.ingredients[0].quantity, None);
    }

    #[test]
    fn given_multiple_images_when_build_user_content_then_parts_in_order() {
        let hint = Some("front and back");
        let images = vec![
            LlmImage {
                bytes: b"front".to_vec(),
                content_type: "image/jpeg".to_string(),
            },
            LlmImage {
                bytes: b"back".to_vec(),
                content_type: "image/jpeg".to_string(),
            },
        ];
        let content = build_user_content(hint, &images);
        let parts = content.parts();
        assert_eq!(parts.len(), 3);
        // parts[0] is the hint text; parts[1..] are the images in order.
        for (idx, img) in images.iter().enumerate() {
            match &parts[idx + 1] {
                genai::chat::ContentPart::Binary(b) => match &b.source {
                    genai::chat::BinarySource::Base64(b64) => {
                        let decoded = base64::engine::general_purpose::STANDARD
                            .decode(b64.as_bytes())
                            .expect("valid base64");
                        assert_eq!(decoded, img.bytes);
                    }
                    _ => panic!("expected base64 binary source"),
                },
                _ => panic!("expected binary content part"),
            }
        }
    }

    #[test]
    fn list_providers_includes_all_providers() {
        let providers = list_providers(None, false);
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
        let providers = list_providers(None, false);
        let ollama = providers
            .iter()
            .find(|p| p.id == "ollama")
            .expect("ollama should exist");
        assert!(ollama.configured, "ollama should always be configured");
    }

    #[test]
    fn list_providers_custom_supports_custom_endpoint() {
        let providers = list_providers(None, false);
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

    #[test]
    fn given_text_and_two_images_when_build_user_content_then_all_parts_present() {
        let img1 = LlmImage {
            bytes: b"aaa".to_vec(),
            content_type: "image/jpeg".to_string(),
        };
        let img2 = LlmImage {
            bytes: b"bbb".to_vec(),
            content_type: "image/png".to_string(),
        };
        let content = build_user_content(Some("tomatoes, cheese"), &[img1, img2]);
        let debug = format!("{:?}", content);
        assert!(debug.contains("tomatoes, cheese"));
        assert!(debug.contains("YWFh"), "base64 of first image missing"); // b64("aaa")
        assert!(debug.contains("YmJi"), "base64 of second image missing"); // b64("bbb")
        assert!(debug.contains("image/png"));
    }

    #[test]
    fn given_no_images_when_build_user_content_then_text_only() {
        let content = build_user_content(Some("eggs"), &[]);
        let debug = format!("{:?}", content);
        assert!(debug.contains("eggs"));
        assert!(!debug.contains("image"));
    }

    #[test]
    fn given_blank_hint_when_build_user_content_then_ignored() {
        let img = LlmImage {
            bytes: b"ccc".to_vec(),
            content_type: "image/jpeg".to_string(),
        };
        let content = build_user_content(Some("   "), &[img]);
        let debug = format!("{:?}", content);
        // The trimmed-empty hint must not become a content part; only the image remains.
        assert_eq!(content.parts().len(), 1);
        assert!(!debug.contains("   "));
        assert!(debug.contains("Y2Nj")); // b64("ccc")
    }
}
