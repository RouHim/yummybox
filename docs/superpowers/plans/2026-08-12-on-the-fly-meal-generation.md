# On-the-Fly AI Meal Generation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let users generate a complete recipe on the fly from a free-text ingredient list and/or up to 5 ingredient photos via a configured LLM, then optionally save it as a meal.

**Architecture:** A new multipart endpoint `POST /api/import/generate` (handler in `src/import.rs`) sends the ingredient text plus up to 5 images to a new `generate_meal_via_llm` function in `src/llm_import.rs` — the same genai tool-call plumbing as the existing LLM import, with a new system prompt. It returns the existing `recipe::ImportDraft`; nothing persists until the user saves. On the frontend, a top-bar "Generate meal" link opens a dedicated `/spontaneous` route (`web/src/routes/spontaneous/+page.svelte`) hosting an inline generator: an ingredients textarea, a photo input (`GenerateImageInput`), and the LLM provider/model picker (extracted into `web/src/lib/components/LlmConfigPicker.svelte`) in a side panel. A successful generation renders an editable draft inline (`MealForm`) that can be saved as a meal (POST `/api/meals`, then navigate to `/meals`) or cooked without persisting via the "Cook now" action (the draft crosses to `/spontaneous/cook` through `sessionStorage`); the provider config is persisted and restored on revisit with the settings panel collapsed. **Implementation note:** the original deep-link design (`/meals?generate=1` opening the add-meal modal in a new "Generate" import tab) was replaced during implementation by this dedicated `/spontaneous` route and inline draft — the flow the 2026-08-13 plan already assumes — and the shipped `tests/e2e/generate-meal.spec.ts` tests that route.

**Tech Stack:** Rust (axum 0.8, sqlx 0.9, genai 0.6), Svelte 5 runes + SvelteKit (adapter-static, SSR off), Vitest, Playwright.

## Global Constraints

- Rust 1.85+, edition 2024. `cargo fmt` and `cargo clippy --all-targets --all-features -- -D warnings` must pass. No `unwrap`/`expect` in non-test code.
- BDD test naming: Rust `given_<precondition>_when_<action>_then_<expected>`, frontend `describe`/`it` with observable behavior.
- Frontend: Svelte 5 runes, strict TypeScript, `cd web && npm run check` must pass. No stores — component-local `$state` only (URL query param used for cross-component state).
- i18n: every new string MUST be added to `web/src/lib/i18n/en.ts`, `web/src/lib/i18n/de.ts`, AND the `TranslationKey` union in `web/src/lib/i18n/types.ts`.
- No new dependencies (backend or frontend). Reuse `genai` multi-part message content for multiple images.
- All LLM failures reuse the existing error codes (`llm_timeout`, `llm_parse_failed`, `llm_api_key_missing`, `llm_request_failed`) via `AppError::Llm`.
- Source of truth: `.spec/spontaneous-on-the-fly-meal-generation.md` (FR-001…FR-009, SC-001…SC-006).
- Workflow e2e runs on `:11342` with an isolated DB; the visual suite (`web/playwright.config.ts`) is untouched.

---

### Task 1: Backend — multi-image user content + generate LLM function

**Files:**
- Modify: `src/llm_import.rs` (change `build_user_content`, add `GENERATE_SYSTEM_PROMPT` + `generate_meal_via_llm`, extend `#[cfg(test)] mod tests`)
- Test: `src/llm_import.rs` (`mod tests`)

**Interfaces:**
- Consumes: `LlmImage { bytes: Vec<u8>, content_type: String }`, `recipe_tool()`, `build_model_spec(model, base_url, api_key)`, `map_genai_error(err)`, `build_draft_from_tool_args(&args, has_user_image)` — all existing.
- Produces: `fn build_user_content(hint: Option<&str>, images: &[LlmImage]) -> genai::chat::MessageContent` (signature changed from `Option<&LlmImage>`; single call site `import_via_llm` updated in this task) and `pub async fn generate_meal_via_llm(model: &str, ingredients: Option<&str>, images: Vec<LlmImage>, base_url: Option<&str>, api_key: Option<&str>) -> Result<recipe::ImportDraft, AppError>`, which Task 2's handler calls.

- [ ] **Step 1: Write the failing tests**

Append to `mod tests` in `src/llm_import.rs`:

```rust
    #[test]
    fn given_text_and_two_images_when_build_user_content_then_all_parts_present() {
        let img1 = LlmImage { bytes: b"aaa".to_vec(), content_type: "image/jpeg".to_string() };
        let img2 = LlmImage { bytes: b"bbb".to_vec(), content_type: "image/png".to_string() };
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
        let img = LlmImage { bytes: b"ccc".to_vec(), content_type: "image/jpeg".to_string() };
        let content = build_user_content(Some("   "), &[img]);
        let debug = format!("{:?}", content);
        assert!(!debug.contains("eggs"));
        assert!(debug.contains("Y2Nj")); // b64("ccc")
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --lib build_user_content`
Expected: compile error — `build_user_content` still has the old `image: Option<&LlmImage>` signature, so the tests do not compile (red).

- [ ] **Step 3: Implement**

Replace the existing `build_user_content` with the slice version, update the `import_via_llm` call site, and add the generation prompt + function directly below `import_via_llm`:

```rust
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
```

In `import_via_llm`, change the call site (line ~243):

```rust
    let user_content = build_user_content(hint, image.as_ref().map_or(&[][..], std::slice::from_ref));
```

Add below `import_via_llm`:

```rust
// ---------------------------------------------------------------------------
// Generate meal (on-the-fly from ingredients / photos)
// ---------------------------------------------------------------------------

const GENERATE_SYSTEM_PROMPT: &str = "You are a creative cooking assistant. Create a recipe from the user's available ingredients (a text list, photos, or both). The recipe must primarily use the provided ingredients; you may add only staple seasonings such as salt, pepper, oil, herbs, and spices. Preserve the exact quantities the user specified; assign plausible quantities to ingredients that have none. Respond in the same language as the user's input. Call the extract_recipe tool with the result. Always call the tool.";

/// Generate a complete recipe draft from an ingredient list and/or photos.
/// Same plumbing as `import_via_llm` (tool call, 60s timeout, error mapping);
/// `has_user_image` is derived from `!images.is_empty()` so the draft does not
/// try to download a dish photo when the user already uploaded photos.
pub async fn generate_meal_via_llm(
    model: &str,
    ingredients: Option<&str>,
    images: Vec<LlmImage>,
    base_url: Option<&str>,
    api_key: Option<&str>,
) -> Result<recipe::ImportDraft, AppError> {
    let client = genai::Client::default();
    let user_content = build_user_content(ingredients, &images);

    let chat_req = genai::chat::ChatRequest::new(vec![
        genai::chat::ChatMessage::system(GENERATE_SYSTEM_PROMPT),
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
    build_draft_from_tool_args(&first.fn_arguments, !images.is_empty()).await
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test --lib build_user_content` and `cargo test --lib llm_import`
Expected: new tests pass; existing `build_draft_from_tool_args` tests (including `given_user_image_when_image_url_present_then_no_download`) still pass.

- [ ] **Step 5: Commit**

```bash
git add src/llm_import.rs
git commit -m "feat: add multi-image LLM meal generation backend"
```

---

### Task 2: Backend — `generate_meal` handler + route

**Files:**
- Modify: `src/import.rs` (add `generate_meal` handler, hoist `MAX_IMAGE_BYTES` to module level, remove the function-local copy inside `import_from_llm`)
- Modify: `src/main.rs` (register route)
- Modify: `src/routes_tests.rs` (register route in the test router, add `build_generate_multipart` helper + validation tests)

**Interfaces:**
- Consumes: `generate_meal_via_llm(model, ingredients, images, base_url, api_key)` from Task 1; `AppError` variants `BadRequest`/`PayloadTooLarge`; `recipe::ImportDraft`.
- Produces: `pub(crate) async fn generate_meal(State(_state): State<Arc<AppState>>, mut multipart: Multipart) -> Result<Json<recipe::ImportDraft>, AppError>` served at `POST /api/import/generate`; module constants `MAX_GENERATE_IMAGES: usize = 5`, `MAX_INGREDIENTS_CHARS: usize = 20000`, `MAX_IMAGE_BYTES: usize = 20 * 1024 * 1024`.

- [ ] **Step 1: Write the failing tests**

In `src/routes_tests.rs`, add the route to the test router (next to the `/import/llm` line, ~line 45):

```rust
        .route("/import/generate", post(crate::import::generate_meal))
```

Add below the existing LLM import tests (after `build_llm_multipart`):

```rust
// ---------------------------------------------------------------
// Generate meal route tests
// ---------------------------------------------------------------

fn build_generate_multipart(
    model: Option<&str>,
    ingredients: Option<&str>,
    images: &[&[u8]],
) -> (Vec<u8>, String) {
    let boundary = "testboundaryGEN";
    let mut body = Vec::new();

    if let Some(m) = model {
        body.extend_from_slice(b"--");
        body.extend_from_slice(boundary.as_bytes());
        body.extend_from_slice(b"\r\n");
        body.extend_from_slice(b"Content-Disposition: form-data; name=\"model\"\r\n\r\n");
        body.extend_from_slice(m.as_bytes());
        body.extend_from_slice(b"\r\n");
    }
    if let Some(ing) = ingredients {
        body.extend_from_slice(b"--");
        body.extend_from_slice(boundary.as_bytes());
        body.extend_from_slice(b"\r\n");
        body.extend_from_slice(b"Content-Disposition: form-data; name=\"ingredients\"\r\n\r\n");
        body.extend_from_slice(ing.as_bytes());
        body.extend_from_slice(b"\r\n");
    }
    for img in images {
        body.extend_from_slice(b"--");
        body.extend_from_slice(boundary.as_bytes());
        body.extend_from_slice(b"\r\n");
        body.extend_from_slice(
            b"Content-Disposition: form-data; name=\"image\"; filename=\"photo.jpg\"\r\n",
        );
        body.extend_from_slice(b"Content-Type: image/jpeg\r\n\r\n");
        body.extend_from_slice(img);
        body.extend_from_slice(b"\r\n");
    }

    body.extend_from_slice(b"--");
    body.extend_from_slice(boundary.as_bytes());
    body.extend_from_slice(b"--\r\n");

    let content_type = format!("multipart/form-data; boundary={boundary}");
    (body, content_type)
}

#[tokio::test]
async fn given_no_fields_when_generate_meal_then_400_missing_model() {
    let ctx = setup().await;
    let (body, content_type) = build_generate_multipart(None, None, &[]);
    let response = ctx
        .app
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
    assert!(json["error"].as_str().unwrap().contains("model"));
}

#[tokio::test]
async fn given_model_without_input_when_generate_meal_then_400() {
    let ctx = setup().await;
    let (body, content_type) = build_generate_multipart(Some("mock-model"), None, &[]);
    let response = ctx
        .app
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
    assert!(
        json["error"]
            .as_str()
            .unwrap()
            .contains("at least one of ingredients or an image")
    );
}

#[tokio::test]
async fn given_six_images_when_generate_meal_then_400() {
    let ctx = setup().await;
    let imgs: Vec<Vec<u8>> = (0..6).map(|i| vec![i as u8; 16]).collect();
    let refs: Vec<&[u8]> = imgs.iter().map(|v| v.as_slice()).collect();
    let (body, content_type) = build_generate_multipart(Some("mock-model"), Some("flour"), &refs);
    let response = ctx
        .app
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
    assert!(json["error"].as_str().unwrap().contains("5 images"));
}

#[tokio::test]
async fn given_oversized_image_when_generate_meal_then_413() {
    let ctx = setup().await;
    let oversized = vec![0u8; 21_000_001];
    let (body, content_type) = build_generate_multipart(Some("mock-model"), Some("flour"), &[&oversized]);
    let response = ctx
        .app
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
    assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
}

#[tokio::test]
async fn given_long_ingredients_when_generate_meal_then_400() {
    let ctx = setup().await;
    let long = "x".repeat(20001);
    let (body, content_type) = build_generate_multipart(Some("mock-model"), Some(&long), &[]);
    let response = ctx
        .app
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
    assert!(json["error"].as_str().unwrap().contains("20000"));
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --lib routes_tests given_ 2>&1 | grep -i generate`
Expected: `given_six_images_when_generate_meal_then_400` and friends fail with 404 (route not yet registered) — red.

- [ ] **Step 3: Implement**

In `src/import.rs`, hoist the image-size constant to module level (replace the function-local `const MAX_IMAGE_BYTES` inside `import_from_llm` — delete that local line) and add the new constants + handler. Place the handler after `import_from_llm`:

```rust
/// Maximum size of a single uploaded image (20 MB).
const MAX_IMAGE_BYTES: usize = 20 * 1024 * 1024;

/// Maximum number of ingredient photos accepted in one generation request.
const MAX_GENERATE_IMAGES: usize = 5;

/// Maximum length of the ingredients text field.
const MAX_INGREDIENTS_CHARS: usize = 20000;

/// Generate a recipe on the fly from an ingredient list and/or photos.
/// The LLM result is returned as a draft; nothing is persisted here.
#[instrument(skip(_state))]
pub(crate) async fn generate_meal(
    State(_state): State<Arc<AppState>>,
    mut multipart: Multipart,
) -> Result<Json<recipe::ImportDraft>, AppError> {
    let mut model: Option<String> = None;
    let mut ingredients: Option<String> = None;
    let mut images: Vec<crate::llm_import::LlmImage> = Vec::new();
    let mut base_url: Option<String> = None;
    let mut api_key: Option<String> = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::BadRequest(format!("invalid multipart data: {e}")))?
    {
        match field.name() {
            Some("model") => {
                let text = field.text().await.map_err(|e| {
                    AppError::BadRequest(format!("failed to read model field: {e}"))
                })?;
                model = Some(text);
            }
            Some("ingredients") => {
                let text = field.text().await.map_err(|e| {
                    AppError::BadRequest(format!("failed to read ingredients field: {e}"))
                })?;
                ingredients = Some(text);
            }
            Some("image") => {
                let content_type = field.content_type().map(String::from);
                let data = field.bytes().await.map_err(|e| {
                    AppError::BadRequest(format!("failed to read image field: {e}"))
                })?;
                images.push(crate::llm_import::LlmImage {
                    bytes: data.to_vec(),
                    content_type: content_type.unwrap_or_else(|| "image/jpeg".to_string()),
                });
            }
            Some("base_url") => {
                let text = field.text().await.map_err(|e| {
                    AppError::BadRequest(format!("failed to read base_url field: {e}"))
                })?;
                base_url = Some(text);
            }
            Some("api_key") => {
                let text = field.text().await.map_err(|e| {
                    AppError::BadRequest(format!("failed to read api_key field: {e}"))
                })?;
                api_key = Some(text);
            }
            _ => {}
        }
    }

    let model = model
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| AppError::BadRequest("missing 'model' field".into()))?;
    let ingredients = ingredients
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let base_url = base_url
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let api_key = api_key
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    if ingredients.is_none() && images.is_empty() {
        return Err(AppError::BadRequest(
            "at least one of ingredients or an image is required".into(),
        ));
    }

    if let Some(ing) = &ingredients {
        if ing.chars().count() > MAX_INGREDIENTS_CHARS {
            return Err(AppError::BadRequest(
                "ingredients must be at most 20000 characters".into(),
            ));
        }
    }

    if images.len() > MAX_GENERATE_IMAGES {
        return Err(AppError::BadRequest(format!(
            "at most {MAX_GENERATE_IMAGES} images may be uploaded"
        )));
    }
    for img in &images {
        if img.bytes.len() > MAX_IMAGE_BYTES {
            return Err(AppError::PayloadTooLarge(
                "image exceeds 20 MB limit".into(),
            ));
        }
    }

    let draft = crate::llm_import::generate_meal_via_llm(
        &model,
        ingredients.as_deref(),
        images,
        base_url.as_deref(),
        api_key.as_deref(),
    )
    .await?;
    Ok(Json(draft))
}
```

In `src/main.rs`, register the route after the `/import/llm` line (~line 114):

```rust
        .route("/import/generate", post(import::generate_meal))
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test --lib routes_tests` and `cargo test --lib import`
Expected: all five new route tests pass; existing import tests still pass.

- [ ] **Step 5: Commit**

```bash
git add src/import.rs src/main.rs src/routes_tests.rs
git commit -m "feat: add POST /api/import/generate handler with validation"
```

---

### Task 3: Frontend — `generateMeal` API client

**Files:**
- Modify: `web/src/lib/api.ts` (add `generateMeal`)
- Test: `web/src/lib/api.test.ts`

**Interfaces:**
- Consumes: `request<T>(url, options)` helper and `ImportDraft` type (both existing).
- Produces: `export async function generateMeal(model: string, ingredients: string, images: File[], baseUrl?: string, apiKey?: string): Promise<ImportDraft>` — consumed by Task 7's `onGenerate`.

- [ ] **Step 1: Write the failing tests**

Append to `web/src/lib/api.test.ts` (reuse the file's existing `mockFetch`/`mockResponse` helpers and the import line for `generateMeal`):

```ts
describe('generateMeal', () => {
	it('sends model, ingredients and multiple images in multipart form', async () => {
		const draft = { name: 'Pasta', ingredients: [], instructions: '', imageBase64: null, portions: null };
		mockResponse(200, draft);
		const img1 = new File([new Uint8Array([1])], 'a.jpg', { type: 'image/jpeg' });
		const img2 = new File([new Uint8Array([2])], 'b.png', { type: 'image/png' });
		await generateMeal('mock-model', 'flour\neggs', [img1, img2]);
		expect(mockFetch).toHaveBeenCalledTimes(1);
		const [url, opts] = mockFetch.mock.calls[0];
		expect(url).toBe('/api/import/generate');
		expect(opts.method).toBe('POST');
		const fd = opts.body as FormData;
		expect(fd.get('model')).toBe('mock-model');
		expect(fd.get('ingredients')).toBe('flour\neggs');
		const images = fd.getAll('image');
		expect(images).toHaveLength(2);
		expect(images[0]).toBe(img1);
		expect(images[1]).toBe(img2);
	});

	it('omits empty ingredients and includes custom endpoint fields', async () => {
		mockResponse(200, { name: '', ingredients: [], instructions: '', imageBase64: null, portions: null });
		await generateMeal('m', '   ', [], 'http://localhost:8080/v1/', 'sk-123');
		const fd = mockFetch.mock.calls[0][1].body as FormData;
		expect(fd.get('ingredients')).toBeNull();
		expect(fd.get('base_url')).toBe('http://localhost:8080/v1/');
		expect(fd.get('api_key')).toBe('sk-123');
	});
});
```

Update the import statement at the top of `api.test.ts` to include `generateMeal`.

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd web && npm test -- -t generateMeal`
Expected: FAIL — `generateMeal` is not exported (red).

- [ ] **Step 3: Implement**

Add to `web/src/lib/api.ts`, after `importFromLlm`:

```ts
export async function generateMeal(
	model: string,
	ingredients: string,
	images: File[],
	baseUrl?: string,
	apiKey?: string,
): Promise<ImportDraft> {
	const form = new FormData();
	form.set('model', model);
	if (ingredients.trim()) form.set('ingredients', ingredients);
	for (const img of images) form.append('image', img);
	if (baseUrl) form.set('base_url', baseUrl);
	if (apiKey) form.set('api_key', apiKey);
	return request<ImportDraft>('/api/import/generate', { method: 'POST', body: form });
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cd web && npm test -- -t generateMeal`
Expected: PASS (both tests).

- [ ] **Step 5: Commit**

```bash
git add web/src/lib/api.ts web/src/lib/api.test.ts
git commit -m "feat: add generateMeal API client"
```

---

### Task 4: Frontend — multi-image upload validation helpers

**Files:**
- Create: `web/src/lib/multi-image.ts`
- Test: `web/src/lib/multi-image.test.ts`

**Interfaces:**
- Consumes: nothing (pure module).
- Produces: `export const MAX_GENERATE_IMAGES = 5`, `export const MAX_GENERATE_IMAGE_BYTES = 20 * 1024 * 1024`, `export function validateGenerateImage(file: File): TranslationKey | null` (returns a `TranslationKey` on rejection — the type is imported from `$lib/i18n/types` — or `null` on accept) — consumed by Task 5's `GenerateImageInput` (which passes the result straight to `t()`).

- [ ] **Step 1: Write the failing tests**

Create `web/src/lib/multi-image.test.ts`:

```ts
import { describe, it, expect } from 'vitest';
import { MAX_GENERATE_IMAGE_BYTES, validateGenerateImage } from './multi-image';

describe('validateGenerateImage', () => {
	it('accepts a small PNG', () => {
		const file = new File([new Uint8Array(8)], 'a.png', { type: 'image/png' });
		expect(validateGenerateImage(file)).toBeNull();
	});

	it('rejects non-image files', () => {
		const file = new File(['x'], 'a.txt', { type: 'text/plain' });
		expect(validateGenerateImage(file)).toBe('generateErrorNotImage');
	});

	it('rejects files over 20 MB', () => {
		const big = new File([new Uint8Array(MAX_GENERATE_IMAGE_BYTES + 1)], 'big.jpg', { type: 'image/jpeg' });
		expect(validateGenerateImage(big)).toBe('generateErrorImageTooLarge');
	});
});
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd web && npm test -- -t validateGenerateImage`
Expected: FAIL — module not found (red).

- [ ] **Step 3: Implement**

Create `web/src/lib/multi-image.ts`:

```ts
import type { TranslationKey } from './i18n/types';

export const MAX_GENERATE_IMAGES = 5;
export const MAX_GENERATE_IMAGE_BYTES = 20 * 1024 * 1024;

/**
 * Returns a TranslationKey describing why `file` is rejected, or null if it
 * may be uploaded. Mirrors the backend limits (image/* and 20 MB).
 */
export function validateGenerateImage(file: File): TranslationKey | null {
	if (!file.type.startsWith('image/')) return 'generateErrorNotImage';
	if (file.size > MAX_GENERATE_IMAGE_BYTES) return 'generateErrorImageTooLarge';
	return null;
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cd web && npm test -- -t validateGenerateImage`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add web/src/lib/multi-image.ts web/src/lib/multi-image.test.ts
git commit -m "feat: add multi-image upload validation helpers"
```

---

### Task 5: Frontend — `GenerateImageInput` component

**Files:**
- Create: `web/src/lib/components/GenerateImageInput.svelte`

**Interfaces:**
- Consumes: `MAX_GENERATE_IMAGES`, `validateGenerateImage` from Task 4; `t` from `$lib/i18n`; `Icon` from `$lib/Icon.svelte`.
- Produces: `GenerateImageInput` with props `files: File[]` (bindable), `disabled: boolean`, `onerror: (error: string | null) => void` — consumed by Task 7's generate tab.

> **Implementation note:** The component shipped as `GenerateImageInput.svelte` (used by the `/spontaneous` route in Task 7). The name `MultiImageInput` belongs to a separate component that also shipped in this PR — `web/src/lib/components/MultiImageInput.svelte`, the add-meal dialog photo strip used by the LLM import tab of `web/src/routes/meals/+page.svelte`. That component exposes `onchange: (files: File[]) => void` and `onerror: (error: string | null) => void` props (no bindable `files`), caps staged photos at 5 (`MAX_PHOTOS`), supports paste/drag/URL staging, and uses `.multi-image-thumbs`/`.multi-image-thumb` classes; it is not produced by any task in this plan.

- [ ] **Step 1: Implement the component**

Create `web/src/lib/components/GenerateImageInput.svelte`:

```svelte
<script lang="ts">
	import { t } from '$lib/i18n';
	import Icon from '$lib/Icon.svelte';
	import { MAX_GENERATE_IMAGES, validateGenerateImage } from '$lib/multi-image';

	let {
		files = $bindable([]),
		disabled = false,
		onerror,
	}: {
		files?: File[];
		disabled?: boolean;
		onerror: (error: string | null) => void;
	} = $props();

	function addFiles(list: FileList | null) {
		if (!list) return;
		const incoming = Array.from(list);
		if (files.length + incoming.length > MAX_GENERATE_IMAGES) {
			onerror(t('generateTooManyImages'));
			return;
		}
		for (const f of incoming) {
			const err = validateGenerateImage(f);
			if (err) {
				onerror(t(err));
				return;
			}
		}
		onerror(null);
		files = [...files, ...incoming];
	}

	function removeFile(index: number) {
		files = files.filter((_, i) => i !== index);
	}
</script>

{#if files.length > 0}
	<ul class="multi-image__list">
		{#each files as file, i}
			<li class="multi-image__item">
				<img src={URL.createObjectURL(file)} alt="" />
				<button
					type="button"
					class="multi-image__remove btn btn--ghost"
					onclick={() => removeFile(i)}
					disabled={disabled}
					aria-label={t('fieldIngredientRemove')}
					title={t('fieldIngredientRemove')}
				>
					<Icon name="x" size={14} />
				</button>
			</li>
		{/each}
	</ul>
{/if}
<label class="multi-image__add">
	<input
		type="file"
		accept="image/*"
		multiple
		disabled={disabled || files.length >= MAX_GENERATE_IMAGES}
		onchange={(e) => {
			const input = e.target as HTMLInputElement;
			addFiles(input.files);
			input.value = '';
		}}
		class="multi-image__input"
	/>
	<span class="btn btn--ghost">{t('generateImagesLabel')}</span>
</label>

<style>
	.multi-image__list {
		display: flex;
		flex-wrap: wrap;
		gap: var(--space-3);
		list-style: none;
		padding: 0;
		margin: 0 0 var(--space-3);
	}
	.multi-image__item {
		position: relative;
		width: 72px;
		height: 72px;
		border-radius: var(--radius-md);
		overflow: hidden;
	}
	.multi-image__item img {
		width: 100%;
		height: 100%;
		object-fit: cover;
		display: block;
	}
	.multi-image__remove {
		position: absolute;
		top: 2px;
		right: 2px;
		padding: 2px;
		line-height: 1;
	}
	.multi-image__input {
		position: absolute;
		width: 1px;
		height: 1px;
		opacity: 0;
		overflow: hidden;
	}
</style>
```

Note: if `--radius-md` does not exist in `web/src/app.css`, use `var(--radius-full)` (check which radius tokens exist; fall back to `6px`).

- [ ] **Step 2: Verify it compiles**

Run: `cd web && npm run check`
Expected: 0 type errors.

- [ ] **Step 3: Commit**

```bash
git add web/src/lib/components/GenerateImageInput.svelte
git commit -m "feat: add GenerateImageInput component"
```

---

### Task 6: Frontend — extract shared `LlmConfigPicker` component

**Files:**
- Create: `web/src/lib/components/LlmConfigPicker.svelte`
- Modify: none — the planned `web/src/routes/meals/+page.svelte` refactor (delete moved logic/state, use the component in the `llm` tab) did not ship (see Step 2).

**Interfaces:**
- Consumes: `listLlmProviders`, `listLlmModels` from `$lib/api`; `readStoredLlmConfig` from `$lib/llm-config.svelte`; `t` from `$lib/i18n`; type `LlmProviderInfo` from `$lib/types`.
- Produces: `LlmConfigPicker` with bindable props `provider`, `model`, `customBaseUrl`, `customApiKey` (strings), plus `disabled: boolean`, `autorestore: boolean` (default `true`), `onrestored?: () => void`. The component owns provider/model listing, stored-config restore (guarded by `!provider` so it never clobbers user edits), and the 500ms custom-endpoint model-reload debounce.

- [ ] **Step 1: Create the component**

Create `web/src/lib/components/LlmConfigPicker.svelte`:

```svelte
<script lang="ts">
	import { listLlmProviders, listLlmModels, ApiError } from '$lib/api';
	import { readStoredLlmConfig } from '$lib/llm-config.svelte';
	import { t } from '$lib/i18n';
	import type { LlmProviderInfo } from '$lib/types';

	let {
		provider = $bindable(''),
		model = $bindable(''),
		customBaseUrl = $bindable(''),
		customApiKey = $bindable(''),
		disabled = false,
		autorestore = true,
		providersReady = $bindable(true),
		onrestored,
	}: {
		provider?: string;
		model?: string;
		customBaseUrl?: string;
		customApiKey?: string;
		disabled?: boolean;
		autorestore?: boolean;
		providersReady?: boolean;
		onrestored?: () => void;
	} = $props();

	let llmProviders = $state<LlmProviderInfo[]>([]);
	let llmProvidersLoading = $state(false);
	let llmProvidersLoaded = $state(false);
	let llmModels: string[] = $state([]);
	let llmModelsLoading = $state(false);
	let llmModelsError = $state<string | null>(null);
	let restored = $state(false);

	async function loadModels() {
		if (!provider) return;
		if (provider === 'custom' && !customBaseUrl.trim()) return;
		llmModelsLoading = true;
		llmModelsError = null;
		try {
			const resp = await listLlmModels(
				provider,
				provider === 'custom' ? customBaseUrl : undefined,
				provider === 'custom' ? customApiKey || undefined : undefined,
			);
			llmModels = resp.models;
			if (model && !resp.models.includes(model)) {
				llmModelsError = t('llmModelsLoadError');
			}
		} catch (err) {
			llmModels = [];
			llmModelsError = err instanceof ApiError
				? (err.code === 'REQUEST_FAILED' ? t('llmModelsLoadError') : `${t('llmModelsLoadError')} (${err.message})`)
				: t('llmModelsLoadError');
		} finally {
			llmModelsLoading = false;
		}
	}

	function onProviderChange() {
		model = '';
		llmModels = [];
		llmModelsError = null;
		customBaseUrl = '';
		customApiKey = '';
		if (provider && provider !== 'custom') {
			loadModels();
		}
	}

	$effect(() => {
		// Restore stored config once per mount; never overwrite user edits.
		if (autorestore && !restored) {
			restored = true;
			const stored = readStoredLlmConfig();
			if (stored && !provider) {
				provider = stored.provider;
				model = stored.model;
				customBaseUrl = stored.customBaseUrl;
				customApiKey = stored.customApiKey;
				if (stored.provider && stored.provider !== 'custom') {
					loadModels();
				}
				if (stored.provider && stored.model) {
					onrestored?.();
				}
			}
		}
		// Load providers once, then reconcile the restored provider.
		if (!llmProvidersLoaded && !llmProvidersLoading) {
			llmProvidersLoading = true;
			providersReady = true; // loading → tabs may show their forms
			listLlmProviders()
				.then((p) => {
					llmProviders = p;
					llmProvidersLoaded = true;
					llmProvidersLoading = false;
					providersReady = p.length > 0;
					if (provider && !p.some((pp) => pp.id === provider)) {
						provider = '';
						model = '';
					}
				})
				.catch(() => {
					llmProvidersLoading = false;
					providersReady = false;
				});
		}
	});

	// Debounced model loading for custom endpoint URL / API key changes.
	let _customDebounceTimer: ReturnType<typeof setTimeout> | undefined;
	$effect(() => {
		customBaseUrl;
		customApiKey;
		if (provider === 'custom' && customBaseUrl.trim()) {
			if (_customDebounceTimer) clearTimeout(_customDebounceTimer);
			_customDebounceTimer = setTimeout(() => {
				loadModels();
			}, 500);
		}
		return () => {
			if (_customDebounceTimer) clearTimeout(_customDebounceTimer);
		};
	});
</script>

{#if llmProviders.length === 0 && !llmProvidersLoading}
	<p class="form-error">{t('llmNoProviders')}</p>
{:else}
	<div class="import-subsection">
		<div class="llm-provider-row">
			<select bind:value={provider} onchange={onProviderChange}
				disabled={llmProvidersLoading || disabled}>
				<option value="">{t('llmProviderPlaceholder')}</option>
				{#each llmProviders as p}
					<option value={p.id} disabled={!p.configured && p.id !== 'ollama'}>
						{p.name}{p.configured ? '' : ` (${t('notConfigured')})`}
					</option>
				{/each}
			</select>

			{#if provider}
				{#if llmModelsLoading}
					<span class="import-loading">{t('llmModelLoading')}</span>
				{:else if llmModelsError}
					<input type="text" bind:value={model} placeholder={t('importLlmModelPlaceholder')} />
				{:else}
					<select bind:value={model} disabled={disabled}>
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
				<input type="url" bind:value={customBaseUrl} placeholder={t('llmCustomBaseUrlPlaceholder')} />
			</label>
			<label class="import-field">
				<span>{t('llmCustomApiKeyLabel')}</span>
				<input type="password" bind:value={customApiKey} placeholder={t('llmCustomApiKeyPlaceholder')} />
			</label>
		{/if}

		{#if llmModelsError}
			<p class="form-error">{llmModelsError}</p>
		{/if}
		{#if provider === 'ollama' && llmModelsError}
			<p class="import-info">{t('llmOllamaHint')}</p>
		{/if}
	</div>
{/if}
```

- [ ] **Step 2: Refactor `web/src/routes/meals/+page.svelte`**

> **Implementation note:** This refactor did not ship. The meals page kept its full inline provider/model picker — its diff in this PR only swaps `ImageInput` for `MultiImageInput` in the LLM import tab (single-file `importLlmImage: File | null` state becomes `importLlmImages: File[]`, fed through the new `onLlmImagesChange`). No state/effects/functions were moved out, and no `<LlmConfigPicker>` was added to the meals page; `LlmConfigPicker` is used exclusively by the `/spontaneous` route (Task 7). The planned deletions (provider/model state, restore/load/debounce effects, `loadLlmModels()`, `onProviderChange()`, the `listLlmProviders`/`listLlmModels` import, and the `LlmProviderInfo` annotation) and the settings-block replacement were never applied.

- [ ] **Step 3: Verify the component**

Run: `cd web && npm run check` — expected 0 errors.
Run: `cd web && npm test` — expected all existing tests pass (the picker itself is exercised by Task 9's e2e, which drives the same component on the `/spontaneous` route).

- [ ] **Step 4: Commit**

```bash
git add web/src/lib/components/LlmConfigPicker.svelte
git commit -m "refactor: extract shared LlmConfigPicker component"
```

---

### Task 7: Frontend — dedicated `/spontaneous` route with inline generator

> **Implementation note:** The original design for this task — a `/meals?generate=1` deep link that opens the existing add-meal modal in a new "Generate" import tab — was replaced during implementation by a dedicated `/spontaneous` route with an inline draft (the flow the 2026-08-13 plan already assumes). The shipped `tests/e2e/generate-meal.spec.ts` tests this route, not the modal. The steps below document what actually shipped.

**Files:**
- Add: `web/src/routes/spontaneous/+page.svelte` (generator page: ingredients textarea, photo input, Generate button, inline draft section)
- Add: `web/src/routes/spontaneous/cook/+page.svelte` (draft cooking view; see the 2026-08-13 plan for the `CookingView` extraction)
- Modify: `web/src/routes/+layout.svelte` (top-bar "Generate meal" link → `/spontaneous`)
- Modify: `web/src/app.css` (spontaneous page styles: `.generate-card`, `.generate-draft`, settings panel, responsive grid)

**Interfaces:**
- Consumes: `generateMeal` (Task 3), `GenerateImageInput` (Task 5), `LlmConfigPicker` (Task 6), `MealForm` (existing; `submitLabel` + `oncook` come from the 2026-08-13 plan), `persistLlmConfig`, `llmErrorMessage` (`web/src/lib/llm-error.ts`), `t`.
- Produces: the `/spontaneous` route with `onGenerate`, `discardDraft`, `onSave` (POST `/api/meals` via `createMeal`, then `goto('/meals')`) and `onCook` (draft image downscaled to a bounded JPEG data URL, stored in `sessionStorage` under `yummybox-cook-draft`, then `goto('/spontaneous/cook')`); the provider config is persisted via `persistLlmConfig` and restored on revisit with the settings panel collapsed.

- [ ] **Step 1: Add the generator page**

`web/src/routes/spontaneous/+page.svelte` holds the whole flow: an `ingredients` textarea (max 20000 chars) and `images` (`GenerateImageInput`, max 5) feed `onGenerate`, which calls `generateMeal(model, ingredients, images, baseUrl?, apiKey?)`, maps failures through `llmErrorMessage` into `generateError`, and on success stores the draft (`name`, `ingredients`, `instructions`, `portions`, `draftImage` built from `imageBase64`), persists the LLM config, collapses the settings panel, and scrolls the inline `.generate-draft` section into view.

- [ ] **Step 2: Inline editable draft**

A successful generation renders `MealForm` inside the `.generate-draft` section, pre-filled with the draft values (`{#key draftToken}` re-mounts it on new generations). "Save" (`onSave`) validates via the existing form and persists the meal through `createMeal`, then navigates to `/meals`. "Cook now" (`onCook`) serializes the draft — with the image downscaled to a bounded JPEG data URL so it fits the `sessionStorage` quota — and navigates to `/spontaneous/cook` without persisting anything. "Start over" (`discardDraft`) clears the draft and photo.

- [ ] **Step 3: Top-bar entry**

In `web/src/routes/+layout.svelte`'s `app-bar__actions`, add a link to `/spontaneous` with `aria-label`/`title` = `t('navGenerateMeal')` and the sparkles icon (styled via the `.app-bar__link` class in `web/src/app.css`).

- [ ] **Step 4: Verify**

Run: `cd web && npm run check` — expected 0 errors.
Run: `cd web && npm test` — expected all pass.

- [ ] **Step 5: Commit**

```bash
git add web/src/routes/spontaneous/+page.svelte web/src/routes/+layout.svelte web/src/app.css
git commit -m "feat: add /spontaneous meal generation route with inline draft"
```

---

### Task 8: Frontend — i18n strings (en/de) + German audit

**Files:**
- Modify: `web/src/lib/i18n/en.ts`
- Modify: `web/src/lib/i18n/de.ts`
- Modify: `web/src/lib/i18n/types.ts`
- Modify: `tests/e2e/i18n.spec.ts` (extend the German audit)

**Interfaces:** Consumes the existing `t(key)` lookup and `TranslationKey` union. Produces the new keys used by Tasks 5 and 7 (`navGenerateMeal`, `generateIngredientsLabel`, `generateIngredientsPlaceholder`, `generateImagesHint`, `generateImagesLabel`, `generateButton`, `generateButtonLoading`, `generateTooManyImages`, `generateErrorNotImage`, `generateErrorImageTooLarge`).

- [ ] **Step 1: Add keys to `en.ts`**

Insert alphabetically near the other LLM/import keys:

```ts
	navGenerateMeal: 'Spontaneous cooking',
	generateIngredientsLabel: 'Ingredients (one per line, quantity optional)',
	generateIngredientsPlaceholder: '3 eggs\nflour\n200 g cheese',
	generateImagesHint: 'Photos require a vision-capable model. The AI identifies the ingredients from the photos.',
	generateImagesLabel: 'Add photos',
	generateButton: 'Generate recipe',
	generateButtonLoading: 'Generating…',
	generateTooManyImages: 'At most 5 photos allowed',
	generateErrorNotImage: 'Only image files are allowed',
	generateErrorImageTooLarge: 'Photo exceeds 20 MB limit',
```

- [ ] **Step 2: Add keys to `de.ts`**

```ts
	navGenerateMeal: 'Spontan kochen',
	generateIngredientsLabel: 'Zutaten (eine pro Zeile, Menge optional)',
	generateIngredientsPlaceholder: '3 Eier\nMehl\n200 g Käse',
	generateImagesHint: 'Fotos benötigen ein visionfähiges Modell. Die KI erkennt die Zutaten auf den Fotos.',
	generateImagesLabel: 'Fotos hinzufügen',
	generateButton: 'Rezept generieren',
	generateButtonLoading: 'Generiere…',
	generateTooManyImages: 'Höchstens 5 Fotos erlaubt',
	generateErrorNotImage: 'Nur Bilddateien sind erlaubt',
	generateErrorImageTooLarge: 'Foto überschreitet das 20-MB-Limit',
```

- [ ] **Step 3: Add keys to the `TranslationKey` union in `types.ts`**

```ts
	| 'navGenerateMeal'
	| 'generateIngredientsLabel'
	| 'generateIngredientsPlaceholder'
	| 'generateImagesHint'
	| 'generateImagesLabel'
	| 'generateButton'
	| 'generateButtonLoading'
	| 'generateTooManyImages'
	| 'generateErrorNotImage'
	| 'generateErrorImageTooLarge'
```

- [ ] **Step 4: Extend the German audit in `tests/e2e/i18n.spec.ts`**

Inside the `'all UI strings translate to German'` test, append at the **end** of the test (after the existing form + validation assertions):

```ts
		// Top bar: spontaneous-cooking link (opens the standalone generate page)
		await expect(page.getByRole('link', { name: 'Spontan kochen' })).toBeVisible();

		// Close the dialog, then open the standalone generate page.
		await page.keyboard.press('Escape');
		await expect(dialog).not.toBeVisible();
		await page.getByRole('link', { name: 'Spontan kochen' }).click();
		await expect(page.getByRole('heading', { name: 'Spontan kochen' })).toBeVisible();
		await expect(page.getByText('Zutaten (eine pro Zeile, Menge optional)')).toBeVisible();
		await expect(page.getByRole('button', { name: 'Rezept generieren' })).toBeVisible();
```

- [ ] **Step 5: Verify**

Run: `cd web && npm test` — the `i18n.test.ts` dictionary-coverage tests pass with the new keys.
Run: `cd web && npm run check` — 0 errors.

- [ ] **Step 6: Commit**

```bash
git add web/src/lib/i18n/en.ts web/src/lib/i18n/de.ts web/src/lib/i18n/types.ts tests/e2e/i18n.spec.ts
git commit -m "feat: add generate meal i18n strings (en/de)"
```

---

### Task 9: E2E — mock LLM server + generate workflow spec

**Files:**
- Create: `tests/e2e/mock-llm.mjs`
- Modify: `tests/playwright.config.ts` (second webServer entry)
- Create: `tests/e2e/generate-meal.spec.ts`

**Interfaces:**
- Consumes: the `/api/import/generate` endpoint (Tasks 1-2) and the UI from Tasks 5-8. The mock speaks the OpenAI chat-completions protocol that `genai`'s custom-provider path calls.
- Produces: a mock OpenAI-compatible server on `127.0.0.1:18999` (`GET /v1/models`, `POST /v1/chat/completions` returning an `extract_recipe` tool call) and an e2e spec covering SC-001…SC-005.

- [ ] **Step 1: Create the mock server**

Create `tests/e2e/mock-llm.mjs`:

```js
// Mock OpenAI-compatible endpoint for e2e tests. The app's "custom" LLM
// provider is pointed at http://127.0.0.1:18999/v1/ and this server answers
// model listing and chat completions with a fixed recipe tool call.
import http from 'node:http';

const PORT = 18999;

const TOOL_CALL = JSON.stringify({
	name: 'Mock Pasta',
	ingredients: [
		{ name: 'flour', quantity: '200 g' },
		{ name: 'eggs', quantity: '3' },
	],
	instructions: '<p>Mix the ingredients and cook until done.</p>',
	portion: 4,
});

http
	.createServer((req, res) => {
		if (req.method === 'GET' && req.url === '/v1/models') {
			res.writeHead(200, { 'content-type': 'application/json' });
			res.end(JSON.stringify({ object: 'list', data: [{ id: 'mock-model' }] }));
			return;
		}
		if (req.method === 'POST' && req.url === '/v1/chat/completions') {
			let body = '';
			req.on('data', (chunk) => (body += chunk));
			req.on('end', () => {
				const payload = JSON.parse(body);
				res.writeHead(200, { 'content-type': 'application/json' });
				res.end(
					JSON.stringify({
						id: 'chatcmpl-mock',
						object: 'chat.completion',
						created: 0,
						model: payload.model,
						choices: [
							{
								index: 0,
								message: {
									role: 'assistant',
									content: null,
									tool_calls: [
										{
											id: 'call_mock',
											type: 'function',
											function: { name: 'extract_recipe', arguments: TOOL_CALL },
										},
									],
								},
								finish_reason: 'tool_calls',
							},
						],
						usage: { prompt_tokens: 1, completion_tokens: 1, total_tokens: 2 },
					}),
				);
			});
			return;
		}
		res.writeHead(404);
		res.end();
	})
	.listen(PORT, '127.0.0.1');
```

- [ ] **Step 2: Register the mock in `tests/playwright.config.ts`**

Change `webServer:` to an array (keep the existing cargo entry exactly as-is; add the mock after it). Note the config's `cwd: '..'` already applies to both commands:

```ts
	webServer: process.env.YUMMYBOX_NO_WEBSERVER === '1' ? undefined : [
		{
			command: "bash -c 'mkdir -p .e2e-db && if [ -x target/x86_64-unknown-linux-gnu/release/yummybox ]; then exec target/x86_64-unknown-linux-gnu/release/yummybox; elif [ -x target/release/yummybox ]; then exec target/release/yummybox; else exec cargo run --quiet; fi'",
			cwd: '..',
			url: 'http://127.0.0.1:11342/api/meals',
			reuseExistingServer: !process.env.CI,
			timeout: 60_000,
			stdout: 'pipe',
			stderr: 'pipe',
			env: {
				YUMMYBOX_PORT: '11342',
				YUMMYBOX_DATA_DIR: './.e2e-db',
			},
		},
		{
			command: 'node tests/e2e/mock-llm.mjs',
			cwd: '..',
			url: 'http://127.0.0.1:18999/v1/models',
			reuseExistingServer: !process.env.CI,
			timeout: 30_000,
		},
	],
```

- [ ] **Step 3: Create the workflow spec**

Create `tests/e2e/generate-meal.spec.ts` — the spec drives the shipped `/spontaneous` route (no dialog):

```ts
import { test, expect } from '@playwright/test';
import { resetMeals, setLocale } from './_helpers';

const TINY_PNG = Buffer.from(
	'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==',
	'base64',
);

async function configureMockProvider(page: import('@playwright/test').Page) {
	// Provider select is the first select in the picker.
	await page.locator('select').first().selectOption('custom');
	await page.getByLabel('Base URL').fill('http://127.0.0.1:18999/v1/');
	// genai's OpenAI adapter requires a key value even for keyless endpoints;
	// the mock ignores the Authorization header.
	await page.getByLabel('API Key (optional)').fill('mock-key');
	// Model list loads from the mock after the 500 ms debounce.
	await expect(page.locator('select').nth(1)).toBeVisible({ timeout: 10_000 });
	await page.locator('select').nth(1).selectOption('mock-model');
}

test.describe('Generate meal page', () => {
	test.beforeEach(async ({ request, page }) => {
		await setLocale(page, 'en');
		await resetMeals(request);
	});

	test('top bar button opens the generate page', async ({ page }) => {
		await page.goto('/');
		await page.getByRole('link', { name: 'Spontaneous cooking' }).click();
		await expect(page).toHaveURL(/\/spontaneous$/);
		await expect(page.getByRole('heading', { name: 'Spontaneous cooking' })).toBeVisible();
		// Generation must not have persisted anything.
		const res = await page.request.get('/api/meals');
		expect((await res.json()) as unknown[]).toHaveLength(0);
	});

	test('generate button is disabled until model and input are provided', async ({ page }) => {
		await page.goto('/spontaneous');
		const generateBtn = page.getByRole('button', { name: /^Generate recipe$/ });
		await expect(generateBtn).toBeDisabled();
		// Ingredients alone are not enough without a model.
		await page.getByLabel(/ingredients/i).fill('flour\neggs');
		await expect(generateBtn).toBeDisabled();
		await configureMockProvider(page);
		await expect(generateBtn).toBeEnabled();
	});

	test('generates a recipe via AI and saves it as a meal', async ({ page }) => {
		await page.goto('/spontaneous');
		await configureMockProvider(page);
		await page.getByLabel(/ingredients/i).fill('flour\neggs');
		await page.getByRole('button', { name: /^Generate recipe$/ }).click();
		// Draft appears in an editable form on the same page (no persistence yet).
		await expect(page.getByLabel('Name', { exact: true })).toHaveValue('Mock Pasta');
		await expect(page.getByText(/AI draft ready/)).toBeVisible();
		let res = await page.request.get('/api/meals');
		expect((await res.json()) as unknown[]).toHaveLength(0);
		// Explicit save persists the meal and returns to the meals list.
		await page.getByRole('button', { name: /^(Save|Speichern)$/ }).click();
		await expect(page).toHaveURL(/\/meals/);
		await expect(page.getByRole('listitem').filter({ hasText: 'Mock Pasta' })).toBeVisible();
		res = await page.request.get('/api/meals');
		expect((await res.json()) as unknown[]).toHaveLength(1);
	});

	test('generates from photos only', async ({ page }) => {
		await page.goto('/spontaneous');
		await configureMockProvider(page);
		await page.locator('input[type="file"]').setInputFiles([
			{ name: 'a.png', mimeType: 'image/png', buffer: TINY_PNG },
			{ name: 'b.png', mimeType: 'image/png', buffer: TINY_PNG },
		]);
		await page.getByRole('button', { name: /^Generate recipe$/ }).click();
		await expect(page.getByLabel('Name', { exact: true })).toHaveValue('Mock Pasta');
	});

	test('rejects more than 5 photos', async ({ page }) => {
		await page.goto('/spontaneous');
		const files = Array.from({ length: 6 }, (_, i) => ({
			name: `${i}.png`,
			mimeType: 'image/png',
			buffer: TINY_PNG,
		}));
		await page.locator('input[type="file"]').setInputFiles(files);
		await expect(page.getByText(/At most 5 photos allowed/)).toBeVisible();
	});

	test('restores the provider config and collapses AI settings on revisit', async ({ page }) => {
		await page.goto('/spontaneous');
		await configureMockProvider(page);
		await page.getByLabel(/ingredients/i).fill('flour\neggs');
		await page.getByRole('button', { name: /^Generate recipe$/ }).click();
		await expect(page.getByLabel('Name', { exact: true })).toHaveValue('Mock Pasta');
		await page.getByRole('button', { name: /^(Save|Speichern)$/ }).click();
		await expect(page).toHaveURL(/\/meals/);
		await page.getByRole('link', { name: 'Spontaneous cooking' }).click();
		await expect(page).toHaveURL(/\/spontaneous$/);
		await expect(page.getByText(/Model: mock-model/)).toBeVisible();
		await expect(page.locator('select').first()).toBeHidden();
		await expect(page.getByLabel(/ingredients/i)).toBeVisible();
		await page.getByRole('button', { name: /^Change$/ }).click();
		await expect(page.locator('select').first()).toBeVisible();
	});

	test('cooks the edited draft without persisting it', async ({ page }) => {
		await page.goto('/spontaneous');
		await configureMockProvider(page);
		await page.getByLabel(/ingredients/i).fill('flour\neggs');
		await page.getByRole('button', { name: /^Generate recipe$/ }).click();
		await expect(page.getByLabel('Name', { exact: true })).toHaveValue('Mock Pasta');
		// Edits made in the form must carry over into cooking.
		await page.getByLabel('Name', { exact: true }).fill('Cooked Draft');
		await page.getByRole('button', { name: 'Cook now' }).click();
		await expect(page).toHaveURL(/\/spontaneous\/cook$/);
		await expect(page.locator('.cooking-view__name')).toHaveText('Cooked Draft');
		await expect(page.locator('.cooking-view__ingredient-list')).toContainText('flour');
		// Nothing was persisted.
		let res = await page.request.get('/api/meals');
		expect((await res.json()) as unknown[]).toHaveLength(0);
		// Leaving the flow forgets the draft: the spontaneous page is fresh.
		await page.goto('/spontaneous');
		await expect(page.locator('.generate-draft')).toHaveCount(0);
		res = await page.request.get('/api/meals');
		expect((await res.json()) as unknown[]).toHaveLength(0);
	});
});
```

- [ ] **Step 4: Run the workflow suite**

Run: `cd tests && npm test -- --grep "Generate meal"` (the suite auto-builds the binary and starts the mock; expects the release/dev binary to build first — use `cargo build` first if the 60s webServer timeout is tight).

Expected: all 7 tests pass. If the mock's chat-completion response shape is rejected by `genai`'s OpenAI adapter, inspect `tests/test-results/` and adjust `mock-llm.mjs` (e.g. adding a `content` field) until the draft fills the form — do not weaken the assertions.

- [ ] **Step 5: Run the full workflow suite to check for regressions**

Run: `cd tests && npm test`
Expected: all prior specs (add-meal, view-meals, edit-meal, delete-meal, search-meals, meal-images, planner, i18n) still pass alongside the new spec.

- [ ] **Step 6: Commit**

```bash
git add tests/e2e/mock-llm.mjs tests/playwright.config.ts tests/e2e/generate-meal.spec.ts tests/e2e/i18n.spec.ts
git commit -m "test: e2e coverage for meal generation with mock LLM"
```

---

### Task 10: Full verification

**Files:** none (verification only).

- [ ] **Step 1: Rust gates**

Run: `cargo fmt --all -- --check` — clean.
Run: `cargo clippy --all-targets --all-features -- -D warnings` — clean.
Run: `cargo test` — all tests pass (existing 269 + new backend tests).

- [ ] **Step 2: Frontend gates**

Run: `cd web && npm run check` — 0 errors.
Run: `cd web && npm test` — all pass (147 tests).

- [ ] **Step 3: E2E suites**

Run: `cd web && npm run test:e2e` — visual/styling suite (6 tests) still passes (requires the release binary; build with `cargo build --release` first if stale).
Run: `cd tests && npm test` — workflow suite (92 tests total, including the 7 new generate tests + extended German audit) passes.

- [ ] **Step 4: Manual smoke (optional but recommended)**

Run `cargo run`, open `http://localhost:11341`, click "Spontaneous cooking" in the top bar, configure a real provider (or point the custom provider at the mock), verify: generation fills the form, nothing is persisted before save, saving creates the meal, and the meals page shows it.

---

## Self-Review

**Spec coverage** (against `.spec/spontaneous-on-the-fly-meal-generation.md`):

| Spec item | Task |
|---|---|
| FR-001 top-bar entry point, all pages | 7 (layout link) + 9 (e2e) |
| FR-002 ingredient text and/or ≤5 photos, ≥1 required | 2 (backend), 4/5 (frontend), 7 (route) |
| FR-003 reuse provider/model selection + custom endpoints | 6 (LlmConfigPicker) |
| FR-004 backend endpoint returning recipe draft | 1, 2 |
| FR-005 prompt: base ingredients, staples only, preserve quantities | 1 (`GENERATE_SYSTEM_PROMPT`) |
| FR-006 no DB writes until explicit save | 2 (handler), 9 (e2e asserts 0 meals pre-save) |
| FR-007 editable draft, same validation as meal creation | 7 (fills `MealForm`; save path unchanged) |
| FR-008 photos require vision-capable model | 8 (`generateImagesHint` copy) + existing error mapping surfacing provider/model errors |
| FR-009 i18n de/en + existing error codes | 8; 1/2 reuse `AppError::Llm` codes |
| SC-001…SC-005 | 9 (e2e), 2 (route validation tests), 4 (client validation) |
| SC-006 German string audit | 8 (i18n.spec extension) |
| Edge cases: empty input, >5 photos, >20000-char ingredients, >20 MB image | 2 (route tests), 4/5 (client), 7 (button disabled) |

**Placeholder scan:** all steps contain concrete code; no TODOs, no "similar to Task N", no unspecified types. The one flagged uncertainty (Task 5 radius token, Task 7 CSS) has an explicit fallback instruction.

**Type consistency:** `generateMeal(model, ingredients, images, baseUrl?, apiKey?)` is defined in Task 3 and called identically in Task 7. `GenerateImageInput`'s `files`/`disabled`/`onerror` props (Task 5) match Task 7's usage. `LlmConfigPicker`'s four bindable props (Task 6) match Task 7's usage. `generate_meal_via_llm(model, ingredients, images, base_url, api_key)` (Task 1) matches Task 2's call. `validateGenerateImage` returns `string | null` (Task 4) and is consumed accordingly in Task 5. The `MealForm` `submitLabel`/`oncook` props consumed by Task 7's inline draft are produced by the 2026-08-13 plan's Task 1.
