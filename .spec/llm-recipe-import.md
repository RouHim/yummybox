# Feature Specification: LLM Recipe Import

**Created**: 2026-06-28
**Status**: Approved
**Input**: LLM-based OpenAPI-compatible recipe import using rust-genai; parse loose photo/text input into structured meal data

## Goal

Add a third import modality to MealMe that accepts unstructured input — a photo of a meal, free-form text, or both — and uses an LLM to parse it into a structured recipe draft. The LLM is constrained via function calling with a JSON Schema tool definition so its output always conforms to the existing `ImportDraft` shape. The user reviews the parsed draft before saving, reusing the same frontend flow as the existing URL and paste import endpoints.

## User Scenarios

### Scenario 1 - Photo import (P1)

A user has taken a photo of a meal they cooked or ate. They open MealMe, select "Import from photo", upload the image, and optionally type a hint (e.g., "Thai green curry"). The backend sends the image to a vision-capable LLM with a tool schema describing the recipe structure. The LLM returns parsed name, ingredients, and instructions. The user reviews the draft in the familiar import review UI, corrects anything, and saves.

**Acceptance**
1. Given the user provides a valid photo and a supported model name, when the import request is sent, then the response is an `ImportDraft` with non-empty `name` and at least one ingredient.
2. Given the user provides a photo of a meal that the LLM cannot identify, when the import request is sent, then the response is a 422 error with message "could not parse a recipe from input".
3. Given the user provides an image larger than 20 MB, when the import request is sent, then the response is a 413 error.

### Scenario 2 - Text import (P2)

A user has a rough text description of a meal — e.g., copied from a chat message or written from memory: "Spaghetti carbonara with pancetta, egg yolks, pecorino, black pepper". They paste the text and trigger the LLM import. The LLM extracts structured name, ingredients (with quantities where inferable), and instructions.

**Acceptance**
1. Given the user provides only a text hint (no photo) with a supported model name, when the import request is sent, then the response is an `ImportDraft` parsed from the text.
2. Given the user provides empty or whitespace-only text and no photo, when the import request is sent, then the response is a 400 error indicating at least one of image or hint is required.

### Scenario 3 - Photo + text hint (P3)

A user provides both a photo and a short text hint (e.g., the dish name). The LLM receives both in a single conversation turn and uses the text to disambiguate or supplement what it sees in the image.

**Acceptance**
1. Given the user provides both a photo and a text hint, when the import request is sent, then both inputs are included in the LLM request and the returned `ImportDraft` reflects the combined information.

## Functional Requirements

- **FR-001**: The system MUST expose a `POST /api/import/llm` endpoint accepting `multipart/form-data` with fields: `model` (text, required), `hint` (text, optional), `image` (file, optional, max 20 MB).
- **FR-002**: The system MUST reject requests where both `image` and `hint` are absent or empty with HTTP 400 and a descriptive error message.
- **FR-003**: The system MUST resolve the LLM provider from the `model` field using rust-genai's model-name-prefix resolution (e.g., `gpt-4o-mini` → OpenAI, `claude-haiku-4-5` → Anthropic, `gemini-2.0-flash` → Gemini).
- **FR-004**: The system MUST define a tool with JSON Schema constraining the LLM output to: `name` (string, required), `ingredients` (array of objects with `name` string and optional `quantity` string, required), `instructions` (string, optional).
- **FR-005**: The system MUST send the image (if provided) as a binary content part and the hint (if provided) as a text content part in a single user message to the LLM.
- **FR-006**: The system MUST extract the tool-call arguments from the LLM response and map them to an `ImportDraft` struct.
- **FR-007**: The system MUST return the `ImportDraft` as JSON with HTTP 200, using the same shape (`name`, `ingredients`, `instructions`, `imageBase64`) as `/api/import/url` and `/api/import/paste`.
- **FR-008**: The system MUST set `imageBase64` to `null` in the returned draft (the LLM does not download images; it only parses text from them).
- **FR-009**: The system MUST read API keys from environment variables following rust-genai conventions (e.g., `OPENAI_API_KEY`, `ANTHROPIC_API_KEY`, `GEMINI_API_KEY`).
- **FR-010**: The system MUST time out the LLM call after 60 seconds and return HTTP 500 with a descriptive error message.
- **FR-011**: The system MUST return HTTP 500 when the resolved provider's API key is not configured, with a message naming the missing environment variable.
- **FR-012**: The system MUST return HTTP 422 when the LLM responds without a tool call or with a tool call containing an empty `name` field.
- **FR-013**: The system MUST include a system message instructing the LLM to extract recipe data from the user's input and to always call the provided tool.
- **FR-014**: The system MUST validate that the parsed `name` is non-empty and at least one ingredient is present; if not, return HTTP 422.

## Key Entities

- **ImportDraft** (existing): `name: String`, `ingredients: Vec<NewIngredientLine>`, `instructions: String`, `imageBase64: Option<String>`. Returned by all import endpoints as the intermediate representation before the user saves a meal.

## Edge Cases

- LLM returns ingredient names the user didn't provide (hallucination) — acceptable limitation; user can edit in the review step.
- Provider returns non-English recipe data — the LLM response is used as-is; no translation layer.
- Image format unsupported by the chosen model — rust-genai handles format detection; errors surface as HTTP 500.
- Network partition between MealMe and the LLM provider — surfaces as HTTP 500 after timeout or connection error.
- Very large text hints — accept up to 5000 characters; truncate with HTTP 400 if exceeded.
- Model name not recognized by rust-genai — surfaces as an error from the genai client; return HTTP 400 with available provider list.

## Research Notes

- rust-genai v0.6.5 `Tool::with_schema(serde_json::Value)` accepts arbitrary JSON Schema for tool parameter definitions — sufficient to constrain output to `ImportDraft` fields. Source: https://docs.rs/genai/latest/genai/chat/struct.Tool.html
- rust-genai supports image input via `ContentPart::from_binary_file` (local files) and `ContentPart::from_binary` (in-memory bytes) for OpenAI, Anthropic, and Gemini. Source: https://github.com/jeremychone/rust-genai/blob/main/examples/c07-image.rs
- The existing `POST /api/import/url` and `POST /api/import/paste` endpoints already return `ImportDraft`, establishing the contract for the new endpoint to follow. No frontend import-flow changes needed beyond adding the new import source option.

## Assumptions

- The user provides their own LLM API keys via environment variables; no UI for key management in this iteration.
- Synchronous HTTP response is acceptable for a local-first single-user application where LLM latency (5–30s) is tolerable.
- The user is responsible for selecting a vision-capable model when providing photo input (the system does not validate model capabilities).
- The existing `ImportDraft` fields (name, ingredients, instructions, imageBase64) are sufficient; no new fields are needed.
- rust-genai is added as a project dependency; its transitive dependencies (reqwest, tokio, serde_json, etc.) are compatible with the existing dependency tree.

## Success Criteria

- **SC-001**: Sending a photo of a recognizable meal (e.g., a plated pasta dish) to the endpoint with a vision-capable model returns an `ImportDraft` with a plausible name and at least one ingredient within 60 seconds.
- **SC-002**: Sending a text hint "Chicken tikka masala with basmati rice, yogurt, garam masala" without a photo returns an `ImportDraft` with name "Chicken Tikka Masala" and at least 3 ingredients.
- **SC-003**: Sending a request with neither image nor hint returns HTTP 400 with a message indicating at least one input is required.
- **SC-004**: Sending a request with a model name whose provider has no API key configured returns HTTP 500 naming the missing environment variable.
- **SC-005**: The response JSON shape matches the existing `ImportDraft` contract exactly, so the frontend can render it without changes to the review/save logic.
- **SC-006**: An image exceeding 20 MB returns HTTP 413 before any LLM call is made.
