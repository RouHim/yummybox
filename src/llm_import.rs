use crate::error::AppError;
use crate::model::NewIngredientLine;
use crate::recipe;
use base64::Engine;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

pub struct LlmImage {
    pub bytes: Vec<u8>,
    pub content_type: String,
}

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const TOOL_NAME: &str = "extract_recipe";

const SYSTEM_PROMPT: &str = "You are a recipe extraction assistant. Extract the recipe from the user's input (an image of a meal, a text description, or both) and call the extract_recipe tool with the result. Always call the tool.";

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
                "instructions": { "type": "string", "description": "Cooking instructions" }
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

pub async fn import_via_llm(
    model: &str,
    hint: Option<&str>,
    image: Option<LlmImage>,
) -> Result<recipe::ImportDraft, AppError> {
    let client = genai::Client::default();
    let user_content = build_user_content(hint, image.as_ref());

    let chat_req = genai::chat::ChatRequest::new(vec![
        genai::chat::ChatMessage::system(SYSTEM_PROMPT),
        genai::chat::ChatMessage::user(user_content),
    ])
    .with_tools(vec![recipe_tool()]);

    let chat_fut = client.exec_chat(model, chat_req, None);
    let chat_res = match tokio::time::timeout(std::time::Duration::from_secs(60), chat_fut).await {
        Ok(r) => r,
        Err(_) => {
            return Err(AppError::Internal(
                "LLM request timed out after 60 seconds".into(),
            ));
        }
    };
    let chat_res = chat_res.map_err(map_genai_error)?;

    let tool_calls = chat_res.into_tool_calls();
    let first = tool_calls.first().ok_or_else(|| {
        AppError::UnprocessableEntity("could not parse a recipe from input".into())
    })?;
    build_draft_from_tool_args(&first.fn_arguments)
}

// ---------------------------------------------------------------------------
// Error mapping
// ---------------------------------------------------------------------------

fn map_genai_error(err: genai::Error) -> AppError {
    match &err {
        genai::Error::RequiresApiKey { model_iden }
        | genai::Error::NoAuthResolver { model_iden }
        | genai::Error::NoAuthData { model_iden } => {
            let env_var = provider_env_var(&model_iden.adapter_kind);
            AppError::Internal(format!(
                "API key not configured for provider '{}': set the {} environment variable",
                model_iden.adapter_kind, env_var
            ))
        }
        genai::Error::Resolver { model_iden, .. }
        | genai::Error::ModelMapperFailed { model_iden, .. } => AppError::BadRequest(format!(
            "model '{}' could not be resolved: {err}",
            model_iden
        )),
        _ => AppError::Internal(format!("LLM request failed: {err}")),
    }
}

fn provider_env_var(adapter_kind: &genai::adapter::AdapterKind) -> &'static str {
    use genai::adapter::AdapterKind;
    #[allow(unreachable_patterns)]
    match adapter_kind {
        AdapterKind::OpenAI | AdapterKind::OpenAIResp => "OPENAI_API_KEY",
        AdapterKind::Anthropic => "ANTHROPIC_API_KEY",
        AdapterKind::Gemini => "GEMINI_API_KEY",
        AdapterKind::Ollama | AdapterKind::OllamaCloud => "OLLAMA_API_KEY",
        AdapterKind::Groq => "GROQ_API_KEY",
        AdapterKind::DeepSeek => "DEEPSEEK_API_KEY",
        AdapterKind::Xai => "XAI_API_KEY",
        AdapterKind::Cohere => "COHERE_API_KEY",
        AdapterKind::Together => "TOGETHER_API_KEY",
        AdapterKind::Fireworks => "FIREWORKS_API_KEY",
        AdapterKind::Nebius => "NEBIUS_API_KEY",
        AdapterKind::Mimo => "MIMO_API_KEY",
        AdapterKind::MiniMax => "MINIMAX_API_KEY",
        AdapterKind::Zai => "ZAI_API_KEY",
        AdapterKind::BigModel => "BIGMODEL_API_KEY",
        AdapterKind::Aliyun => "ALIYUN_API_KEY",
        AdapterKind::Baidu => "BAIDU_API_KEY",
        AdapterKind::Moonshot => "MOONSHOT_API_KEY",
        AdapterKind::Aihubmix => "AIHUBMIX_API_KEY",
        AdapterKind::OpenRouter => "OPEN_ROUTER_API_KEY",
        AdapterKind::Vertex => "Vertex (uses GCP credentials)",
        AdapterKind::BedrockApi => "BEDROCK_API_KEY",
        AdapterKind::GithubCopilot => "GITHUB_COPILOT_API_KEY",
        AdapterKind::OpenCodeGo => "OPENCODE_GO_API_KEY",
        _ => "the provider's API key environment variable",
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
}

#[derive(serde::Deserialize)]
struct LlmIngredient {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    quantity: Option<String>,
}

fn build_draft_from_tool_args(args: &serde_json::Value) -> Result<recipe::ImportDraft, AppError> {
    let draft: LlmRecipeDraft = serde_json::from_value(args.clone()).map_err(|e| {
        AppError::UnprocessableEntity(format!("could not parse a recipe from input: {e}"))
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
        return Err(AppError::UnprocessableEntity(
            "could not parse a recipe from input".into(),
        ));
    }
    let ingredients: Vec<_> = ingredients
        .into_iter()
        .filter(|i| !i.name.is_empty())
        .collect();
    Ok(recipe::ImportDraft {
        name,
        ingredients,
        instructions: recipe::sanitize_instructions(&draft.instructions.unwrap_or_default()),
        image_base64: None,
    })
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn given_empty_name_when_build_draft_then_422() {
        let args = serde_json::json!({"name": "", "ingredients": [{"name": "x"}]});
        let result = build_draft_from_tool_args(&args);
        assert!(result.is_err());
        match result {
            Err(AppError::UnprocessableEntity(msg)) => {
                assert!(msg.contains("could not parse a recipe from input"));
            }
            _ => panic!("expected UnprocessableEntity, got {:?}", result),
        }
    }

    #[test]
    fn given_no_ingredients_when_build_draft_then_422() {
        let args = serde_json::json!({"name": "Pasta", "ingredients": []});
        let result = build_draft_from_tool_args(&args);
        assert!(result.is_err());
        match result {
            Err(AppError::UnprocessableEntity(msg)) => {
                assert!(msg.contains("could not parse a recipe from input"));
            }
            _ => panic!("expected UnprocessableEntity, got {:?}", result),
        }
    }

    #[test]
    fn given_all_empty_ingredient_names_when_build_draft_then_422() {
        let args =
            serde_json::json!({"name": "Pasta", "ingredients": [{"name": ""}, {"name": "  "}]});
        let result = build_draft_from_tool_args(&args);
        assert!(result.is_err());
    }

    #[test]
    fn given_valid_args_when_build_draft_then_returns_draft() {
        let args = serde_json::json!({
            "name": "Pasta",
            "ingredients": [{"name": "flour", "quantity": "200g"}],
            "instructions": "Boil"
        });
        let draft = build_draft_from_tool_args(&args).expect("should succeed");
        assert_eq!(draft.name, "Pasta");
        assert_eq!(draft.ingredients.len(), 1);
        assert_eq!(draft.ingredients[0].name, "flour");
        assert_eq!(draft.ingredients[0].quantity.as_deref(), Some("200g"));
        assert_eq!(draft.instructions, "Boil");
        assert!(draft.image_base64.is_none());
    }

    #[test]
    fn given_missing_instructions_when_build_draft_then_empty_string() {
        let args = serde_json::json!({
            "name": "Pasta",
            "ingredients": [{"name": "flour"}]
        });
        let draft = build_draft_from_tool_args(&args).expect("should succeed");
        assert_eq!(draft.instructions, "");
    }

    #[test]
    fn given_quantity_blank_when_build_draft_then_quantity_none() {
        let args = serde_json::json!({
            "name": "Pasta",
            "ingredients": [{"name": "flour", "quantity": "  "}]
        });
        let draft = build_draft_from_tool_args(&args).expect("should succeed");
        assert_eq!(draft.ingredients[0].quantity, None);
    }
}
