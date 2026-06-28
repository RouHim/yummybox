# MealMe

A single-binary local-first web application for managing a personal collection of meals. Built with Rust (axum + rusqlite) and Svelte 5, all frontend assets embedded in the binary.

## Quickstart

```bash
cargo run --release
```

Then open **http://127.0.0.1:11341** in your browser.

The server listens on `127.0.0.1:11341` and persists data in `./data/meals.db` (SQLite, auto-created on first run; override the directory with the `MEALME_DATA_DIR` env var).

## Requirements

- **Rust** 1.85+ (with Cargo)
- **Node.js** 26+ (build-time only — `build.rs` runs `npm install && npm run build` in `web/`)
- **`just`** (optional, for E2E workflow — install via `cargo install just` or your package manager)

No Docker, nginx, or Node.js runtime needed after compilation.

## Development

```bash
# Run all Rust tests
cargo test

# Run all frontend tests
cd web && npm test

# Build release binary
cargo build --release

# Run with debug logging
RUST_LOG=debug cargo run
```

### E2E tests

```bash
# One-time setup
just e2e-install

# Run the suite
just e2e

# Debug locally
just e2e-headed
just e2e-ui
```

## API

| Method | Path | Description |
|--------|------|-------------|
| GET | `/api/meals` | List all meals (ordered by most recent) |
| GET | `/api/meals?search=term` | Search meals by name or ingredients (case-insensitive) |
| POST | `/api/meals` | Create a meal `{"name":"...","ingredients":[{"name":"...","quantity":null}]}` |
| GET | `/api/meals/:id` | Get a single meal |
| PUT | `/api/meals/:id` | Update a meal |
| DELETE | `/api/meals/:id` | Delete a meal |
| POST | `/api/plans` | Generate a plan `{"year":2026,"week_number":1,"meal_count":3}` |
| POST | `/api/import/url` | Import a recipe from URL `{"url":"https://..."}` |
| POST | `/api/import/paste` | Import a recipe from raw HTML/JSON-LD `{"content":"..."}` |
| POST | `/api/import/llm` | Import a recipe via vision LLM (multipart: `model`, `hint?`, `image?`) |
| GET | `/api/plans?year=&week=` | Get a specific plan with meals and ingredient summary |
| GET | `/api/plans?year=` | List all plans for a year |
| PUT | `/api/plans/:year/:week` | Update plan meals `{"meal_ids":[1,2,3]}` |
| DELETE | `/api/plans/:year/:week` | Delete a plan |

**Ingredients** are now stored as normalized rows: each meal has one or more ingredients with optional quantity text (e.g. `"200g"`, `"2 cups"`, `"a pinch"`). Plans aggregate ingredients — identical ingredients are merged and numeric quantities summed. The `/planner` route provides a week calendar for generating and managing weekly meal plans.

Validation: meal name (1–200 chars), ingredient name (1–100 chars per line), ingredient quantity (0–50 chars), max 100 ingredient lines. All API paths are under `/api`; everything else serves the SPA frontend.


## LLM Recipe Import

The app can parse recipes from photos or text descriptions using a vision-capable LLM (e.g. GPT‑4o).

### Setup

Set an API key for your provider as an environment variable:

| Provider | Env var | Example model |
|----------|---------|---------------|
| OpenAI | `OPENAI_API_KEY` | `gpt-4o-mini` |
| Anthropic | `ANTHROPIC_API_KEY` | `claude-sonnet-4-20250514` |
| Google | `GOOGLE_API_KEY` | `gemini-2.5-flash` |
| Groq | `GROQ_API_KEY` | `llama-4-maverick-17b-128e-instruct` |
| Ollama (local) | _(none)_ | `llama3.2-vision` |

```bash
OPENAI_API_KEY=sk-... cargo run --release
```

Then open the app, switch to the **Meals** page, and use the **"From photo / text"** tab:

1. Enter a model name (e.g. `gpt-4o-mini`)
2. Optionally describe the dish in the hint field
3. Optionally attach a photo of the dish or recipe
4. Click **"Parse with AI"** — review the extracted recipe, then save

At least one of hint or image is required. The LLM returns a structured recipe draft (name, ingredients, instructions) that flows into the same review-and-save workflow used by URL and paste imports.

### How it works

The backend sends a single chat completion request with a tool definition (`extract_recipe`) that constrains the LLM to return structured JSON. The response is mapped to the standard `ImportDraft` shape the frontend already understands. Requests time out after 60 seconds.