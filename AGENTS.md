# Repository Guidelines

## Project Overview

**YummyBox** is a single-binary local-first web application for managing a personal collection of meals. A Rust backend (axum + sqlx) serves a REST API and an embedded Svelte 5 SPA frontend — no Node.js runtime needed after compilation. The binary listens on `0.0.0.0:{port}` (default `11341`, override with `YUMMYBOX_PORT`) and persists data to a SQLite database (auto-created on first run; override the data directory with `YUMMYBOX_DATA_DIR`).

## Architecture & Data Flow

```
Browser (SPA) ──fetch──▶ axum (Rust) ──async──▶ SQLite (via SqlitePool)
                    │
                    ├── /api/*   → route handlers → db/plan/import/bring functions
                    └── /*       → spa_fallback → embedded web/build/
```

- **SPA frontend** (Svelte 5 runes, adapter-static, SSR disabled) makes `fetch` calls to `/api/*` on the same origin.
- **API routes** live under `/api/` covering meals, import/LLM, plans, Bring! shopping, export/import, version, and meal images. All other paths fall back to `index.html` (SPA client-side routing).
- **State** is a single `Arc<AppState>` containing a `SqlitePool` — shared across all route handlers.
- **Frontend assets** are embedded at compile time via `rust-embed` from `web/build/`, which `build.rs` produces by running `npm install && npm run build` in `web/`.

| Directory | Purpose |
|-----------|---------|
| `src/` | Rust source — flat files, no nested modules |
| `src/main.rs` | Binary entry point, routing, server startup |
| `src/db.rs` | SQLite operations (init, CRUD, validation, search, ingredient hydration) |
| `src/routes.rs` | Axum handler functions: meals, plans, Bring!, version, images + integration tests |
| `src/plan.rs` | Week math, plan CRUD, ingredient aggregation, weighted meal selection |
| `src/import.rs` | Import handlers: URL, LLM, paste, bulk; LLM config helpers |
| `src/llm_import.rs` | LLM-based recipe extraction and parsing |
| `src/recipe.rs` | HTML recipe scraping and ingredient extraction |
| `src/model.rs` | Serde DTOs: `Meal`, `NewMeal`, `MealPatch`, `Plan`, etc. |
| `src/error.rs` | `AppError` enum (thiserror) + `IntoResponse` impl |
| `src/state.rs` | `AppState` struct holding the `SqlitePool` |
| `src/static_assets.rs` | SPA fallback via `rust-embed` |
| `src/image.rs` | Image conversion/validation |
| `src/jsonld.rs` | JSON-LD (schema.org Recipe) serialization |
| `src/bring.rs` | Bring! shopping list integration |
| `src/export_import.rs` | ZIP export/import of meals |
| `src/seed.rs` | Database seeding with sample meals (`yummybox seed`) |
| `src/data_dir.rs` | Data directory resolution and creation |
| `migrations/` | SQLx migration files for schema evolution |
| `web/` | SvelteKit project (frontend) |
| `web/src/routes/` | Svelte page components: `+page.svelte` (home), `meals/` (list/create), `meals/[id]/` (detail/edit/cooking), `planner/` (weekly planner) |
| `web/src/lib/` | Shared modules: `api.ts` (fetch client), `validation.ts`, `types.ts`, i18n, components |
| `web/build/` | Build output (embedded into binary at compile time) |

## Development Commands

```bash
# Build and run (production)
# Run all Rust tests (286 tests)
cargo test

# Run all frontend tests (147 tests)
cd web && npm test

# Type-check frontend
cd web && npm run check

# Run with debug logging
RUST_LOG=debug cargo run

# Format + lint Rust
cargo fmt
cargo clippy
# Run E2E tests (visual/styling — requires release binary)
cd web && npm run test:e2e

# Run E2E tests (workflows — auto-builds via cargo run, isolated DB on :11342)
cd tests && npm test

# Run all E2E tests
cd web && npm run test:e2e && cd ../tests && npm test

# Run E2E tests headed (debugging)
cd tests && npx playwright test --headed

# Dev server (frontend only, for UI work)
cd web && npm run dev
```

**Build-time requirement**: Node.js 26+ (only during `build.rs` execution; not needed at runtime). The `build.rs` step is a hard prerequisite of every `cargo build`; do not skip it via `cargo build --no-default-features` or by stubbing out `web/build/`.

## Code Conventions & Common Patterns

For the higher-level engineering principles (SOLID, YAGNI, error surfacing) see `## Engineering Principles` below; this section covers the file-level mechanics.

### Rust

**File structure**: Flat single files (no nested `mod` directories). Each file has a single responsibility. Module declarations live in `main.rs`.

**Error handling**: Use `thiserror`-derived `AppError` enum. Every variant maps to an HTTP status code and JSON body `{"error": "..."}` via `IntoResponse`. No `unwrap`/`expect` in non-test code. Convert `sqlx::Error` with `#[from]`, `serde_json::Error` with a manual `From` impl.

**Async pattern**: Handlers are `async fn` returning `Result<Json<T>, AppError>` or `Result<(StatusCode, Json<T>), AppError>`. DB access uses `SqlitePool` with `sqlx::query!` / `sqlx::query_as!` macros — no manual locking needed.

**State injection**: Axum `State(Arc<AppState>)` extractor. `AppState` holds a `SqlitePool`.

**Logging**: `tracing` with `#[instrument(skip(state))]` on handlers. `tracing-subscriber` with `EnvFilter` (default `info`, overridable via `RUST_LOG`).

**Validation**: `db::validate_meal()` enforces: name 1–200 chars, instructions 1–20000 chars, 1–100 ingredient lines (name ≤100 chars, quantity ≤50 chars), portions 1–10000. Both backend and frontend enforce the same limits. Validation runs inside `insert_meal` and `update_meal` before touching the DB.

**Testing**: 
- DB tests: `#[tokio::test]` (async) for operations touching the DB, `#[test]` for pure validation/string helpers. Use `tempfile::TempDir` for isolated databases.
- Route tests: `#[tokio::test]`, use `tower::ServiceExt::oneshot` to send `Request` objects to the router. Helper `TestCtx` struct holds the app and temp directory.
- Naming convention: `given_<precondition>_when_<action>_then_<expected_result>`.

**Dependencies**:
- Keep external crates as low as possible; prefer `std` and built-in features (e.g., `sqlx`'s `SqlitePool` is already in the tree — use its connection pool, not a standalone connection manager).
- Before adding a third-party crate, evaluate the trade-off: maintenance burden, transitive deps, MSRV impact, and whether `std` or an existing dep already covers it.
- Pin the very latest stable version available on crates.io at the time of introduction; bump via `cargo upgrade` and review changelogs for breaking changes.
- Current set: `axum` (http1+json+query+multipart+macros), `sqlx` (sqlite+runtime-tokio+chrono+migrate+macros), `serde`/`serde_json`, `chrono`, `tokio`, `tracing`/`tracing-subscriber`, `thiserror`, `rust-embed`, `reqwest` (rustls+json+form), `image` (jpeg+png+gif+bmp+webp), `scraper`, `recipe-scraper`, `genai`, `base64`, `rand`, `ammonia`, `zip`. Dev: `tempfile`, `tower` (util only).

### Frontend (Svelte 5 + TypeScript)

**Framework**: Svelte 5 runes mode (`$state`, `$effect`, `$props`). SvelteKit with `adapter-static` (SPA fallback mode). SSR disabled (`ssr: false` in `vite.config.ts`).

**State management**: Component-local `$state` variables. No stores, no external state library.

**API client**: `web/src/lib/api.ts` — thin `fetch` wrapper. Generic `request<T>()` handles JSON parsing, error extraction from `{"error":"..."}`, and 204 No Content. Exported functions: `listMeals`, `createMeal`, `updateMeal`, `deleteMeal`.

**Validation**: `web/src/lib/validation.ts` mirrors backend constraints. Returns discriminated union `{ ok: true } | { ok: false; field; message }`.

**Testing**: `vitest` with `@sveltejs/kit/vite` plugin. Tests in `src/**/*.test.ts`. Mock `globalThis.fetch` with `vi.fn()`. Test files use BDD-style `describe`/`it` blocks.

**Styling**: Single `app.css` — system-ui font, 720px max-width centered layout, minimal clean styling. No CSS framework.

**TypeScript**: Strict mode, `moduleResolution: "bundler"`. Types in `web/src/lib/types.ts`: `Meal` interface and `MealPayload`.

## Engineering Principles

### SOLID

One-line summary: each principle maps directly to a project convention — follow them to keep the codebase maintainable and testable.
- **Single Responsibility**: one file = one concern (already encoded in the file-structure rule above).
- **Open/Closed**: extend behavior by adding new route handlers or DB functions rather than mutating existing ones whose tests are green.
- **Liskov Substitution**: handlers and DB functions are called through their concrete return types; no subtype-swap tricks. Listed for completeness — not a current concern.
- **Interface Segregation**: prefer narrow function signatures over fat `AppState` accessors; pass only the needed value (e.g., a `&SqlitePool` reference) into helpers.
- **Dependency Inversion**: handlers depend on `AppState` (a concrete struct); DB-free logic lives in pure functions that take their inputs by value, keeping them testable without a DB.

### YAGNI

Don't add functionality, configuration knobs, abstractions, or `mod` directories until a concrete caller needs them.
If a function has no call site, delete it — no commented-out scaffolds, no `#[allow(dead_code)]` to keep a future placeholder.

### UX & Error Surfacing

- Handle user interactions gracefully: every HTTP error path returns a structured JSON `{"error": "..."}` body with the correct status code; the frontend renders it inline — never an uncaught `unwrap` panic surfaces a 500 with no body.
- Surface actionable errors to the UI: messages name the offending field and the constraint (e.g., `name must be 1–200 characters`), not a bare `invalid input`.
- Validation runs in `db::validate_meal` before any DB write; the frontend calls the same `validateMeal()` helper from `$lib/validation.ts` to mirror the constraint.

## Important Files

| File | Role |
|------|------|
| `Cargo.toml` | Binary name `yummybox`, edition 2024, Rust 1.85+, sqlx with compile-time checked queries |
| `build.rs` | Auto-runs `npm install && npm run build` in `web/` |
| `src/main.rs` | Router assembly, port binding, logging init, subcommand dispatch |
| `src/db.rs` | SQL queries, validation, `init_db` schema creation, ingredient hydration |
| `src/routes.rs` | Handlers: meals (list/get/create/update/delete), meal images, plans CRUD, Bring!, version |
| `src/plan.rs` | Week math, plan CRUD, ingredient aggregation, weighted meal selection |
| `src/import.rs` | Import handlers: URL, LLM, paste, bulk; LLM config helpers |
| `src/error.rs` | `AppError` → HTTP response mapping |
| `src/state.rs` | `AppState` (`SqlitePool`) |
| `src/static_assets.rs` | MIME type mapping, SPA fallback logic |
| `migrations/` | SQLx migration files for schema evolution |
| `web/package.json` | Scripts: `dev`, `build`, `test`, `check` |
| `web/vite.config.ts` | SvelteKit plugin, adapter-static, SSR disabled |
| `web/vitest.config.ts` | Test include pattern: `src/**/*.test.ts` |
| `web/src/routes/` | Svelte page components: home, meals list/detail/cooking, weekly planner |
| `web/src/lib/api.ts` | Fetch client for all API endpoints |
| `web/playwright.config.ts` | E2E config: visual tests on :11341 (release binary) |
| `tests/playwright.config.ts` | E2E config: workflow tests on :11342 (auto-build, isolated DB) |
| `tests/e2e/_helpers.ts` | Shared E2E helpers: `createMeal`, `resetMeals`, `setLocale` |

## Workflow Practices

1. **Web search second opinion**: when planning a non-trivial feature, do not rely solely on training data — run a web search (or `mcp__context_query_docs` for libraries) to confirm current best practices, latest API shape, and any deprecations since the model's cutoff. Record the source in the plan's Research Notes if it changes the design.
2. **Lint before done**: CI enforces these gates on every push/PR: `cargo fmt --all -- --check`, `cargo clippy --all-targets --all-features -- -D warnings`, and `cd web && npm run check`. Run them locally before opening a PR to avoid round-trips.
3. **Consistent style**: match the existing file's formatting (4-space indent, no trailing whitespace, `rustfmt` defaults), so a reviewer can read the diff rather than the new file.

## Runtime/Tooling Preferences

- **Rust**: 1.85+ (edition 2024). Use `rustfmt` and `clippy`.
- **Node.js**: 26+ (build-time only, for SvelteKit/Vite compilation).
- **Port**: `0.0.0.0:11341` by default, overridable via `YUMMYBOX_PORT` env var.
- **CI**: `.github/workflows/ci.yml` — lint-format + frontend-tests + rust-tests (parallel) → e2e-tests (both suites) → create-release → build-binaries + build-container.
- **Database**: SQLite via `sqlx` (bundled) — no system SQLite needed. Schema managed via `migrations/`.
- **TLS**: `rustls` only (project rule: no OpenSSL / no `native-tls`). No TLS endpoints today, but any future TLS feature must use a `rustls`-based crate (e.g., `rustls`, `reqwest` with `rustls-tls` feature).

## Testing & QA

Follow **TDD**: write the failing test first, watch it fail, then implement the smallest change to make it pass; refactor only after green. This applies to both Rust (`#[cfg(test)]` and route integration tests) and the frontend (`*.test.ts`).
All tests are written in **BDD** style: name them by behavior, not implementation. Rust: `given_<precondition>_when_<action>_then_<expected_result>`. Frontend: `describe('<unit>', () => { it('<observable behavior>', ...) })`. BDD names double as living documentation.

### Rust tests (`cargo test`)

- **Location**: `#[cfg(test)]` modules — inline within each source file, or in flat sibling `*_tests.rs` modules declared in `main.rs` (e.g. `routes_tests.rs`, `db_tests.rs`).
- **DB layer** (`db.rs`): Unit tests for CRUD, validation edge cases (empty strings, boundary lengths, whitespace-only), search filtering, week math, weighted meal selection, ingredient aggregation.
- **Route layer** (`routes.rs`): Integration tests using `tower::ServiceExt::oneshot`. Verify status codes, response bodies, search filtering, 404 for missing resources, plan CRUD, import endpoints, Bring! handlers.
- **Plan, import, seed, image** (`plan.rs`, `import.rs`, `seed.rs`, `image.rs`): Unit tests for week math, weighted selection determinism, seeding idempotency, image conversion.
- **Error layer** (`error.rs`): Verify each `AppError` variant maps to correct status code and JSON body.
- **Model layer** (`model.rs`): Serde round-trip and field deserialization.
- **Static assets** (`static_assets.rs`): Verify SPA fallback returns index.html, correct MIME types.
- **Isolation**: Every test uses `tempfile::TempDir` for fresh databases. No shared state between tests.
- **TDD workflow**: for any new DB function or route handler, add the failing test inside the same `#[cfg(test)] mod tests` block, run `cargo test -- <test_name>`, watch red, then implement.
- **No unwrap/expect in non-test code** is the production rule; tests may use them freely, but prefer `assert_eq!` / `assert!` with a failure message naming the precondition.

### Frontend tests (`cd web && npm test`)

- **Framework**: Vitest 4 with `@sveltejs/kit/vite` plugin.
- **API tests** (`api.test.ts`): Mock `fetch`, verify correct URL/method/body/headers for each API function, test error message extraction.
- **Validation tests** (`validation.test.ts`): Boundary and edge-case validation (empty, whitespace, exact-limit, one-over-limit).
- **Component tests**: Form rendering, focus trap behavior, i18n coverage.
- **TDD workflow**: write the failing Vitest case first (`it(...)` with the observable behavior in the name), run `cd web && npm test -- -t <name>` to confirm red, then implement.

### E2E tests

E2E tests use Playwright with headless Chromium. Each config auto-starts the `yummybox` binary on its own port with an isolated database, so no manual server setup is needed.

**Two test suites** with separate Playwright configs:

| Suite | Config | Port | Binary | DB | Command |
|-------|--------|------|--------|----|---------|
| Visual/styling | `web/playwright.config.ts` | `:11341` | `target/release/yummybox` | shared `./data/yummybox.db` | `cd web && npm run test:e2e` |
| Workflows | `tests/playwright.config.ts` | `:11342` | `cargo run --quiet` | isolated `.e2e-db/` | `cd tests && npm test` |

#### Visual/styling suite (`web/e2e/ambient-background.spec.ts`, 6 tests)

- `.app-ambient` element exists and covers the viewport
- `background-image` CSS resolves to a real image (HTTP 200, ≥1 KB)
- Pixel variance analysis via canvas (solid-color ratio < 95%, std dev > 2)
- Footer attribution: photographer credit link
- Dark mode image variance (URL contains `ambient-dark`, pixel variance)
- Dark mode `backgroundColor` — each RGB channel < 80

#### Workflow suite (`tests/e2e/`, 31 tests across 8 spec files)

| File | Tests | Coverage |
|------|-------|----------|
| `add-meal.spec.ts` | 3 | Happy path, empty-name validation, empty-ingredients validation |
| `view-meals.spec.ts` | 2 | Empty state message, meal name + ingredient previews |
| `edit-meal.spec.ts` | 2 | Pre-populated form values, update reflects in list |
| `delete-meal.spec.ts` | 2 | Confirm delete removes meal, cancel keeps it |
| `search-meals.spec.ts` | 3 | Filter by name (case-insensitive), by ingredient, clear search restores all |
| `meal-images.spec.ts` | 7 | Upload shows thumbnail, edit-to-add sets `has_image`, replace image, remove image, non-image error inline, no-image meals have no `<img>`, oversized PNG downscaled to ≤3840×2160 JPEG |
| `planner.spec.ts` | 9 | Future week click, past-week CSS class, past year all muted, meal count defaults to 3, count resets on new week, no lang toggle, navigator language → no localStorage |
| `i18n.spec.ts` | 3 | `de-DE` → German strings, `fr-FR` → English fallback, full German UI string audit |

Shared helper (`tests/e2e/_helpers.ts`): `setLocale(page, locale)`, `resetMeals(request)`, `createMeal(page, name, ingredients, instructions?)`.
