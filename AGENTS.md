# Repository Guidelines

## Project Overview

**MealMe** is a single-binary local-first web application for managing a personal collection of meals. A Rust backend (axum + rusqlite) serves a REST API and an embedded Svelte 5 SPA frontend — no Node.js runtime needed after compilation. The binary listens on `127.0.0.1:11341` and persists data to `./meals.db` (SQLite, auto-created on first run).

## Architecture & Data Flow

```
Browser (SPA) ──fetch──▶ axum (Rust) ──sync──▶ SQLite (meals.db)
                    │
                    ├── /api/*   → route handlers → db functions
                    └── /*       → spa_fallback → embedded web/build/
```

- **SPA frontend** (Svelte 5 runes, adapter-static, SSR disabled) makes `fetch` calls to `/api/*` on the same origin.
- **API routes** live under `/api/meals` and `/api/meals/:id`. All other paths fall back to `index.html` (SPA client-side routing).
- **State** is a single `Arc<AppState>` containing a `tokio::sync::Mutex<rusqlite::Connection>` — shared across all route handlers.
- **Frontend assets** are embedded at compile time via `rust-embed` from `web/build/`, which `build.rs` produces by running `npm install && npm run build` in `web/`.

## Key Directories

| Directory | Purpose |
|-----------|---------|
| `src/` | Rust source — flat files, no nested modules |
| `src/main.rs` | Binary entry point, routing, server startup |
| `src/db.rs` | SQLite operations (init, CRUD, validation, search) |
| `src/routes.rs` | Axum handler functions + `#[cfg(test)]` integration tests |
| `src/model.rs` | Serde DTOs: `Meal`, `NewMeal`, `MealPatch` |
| `src/error.rs` | `AppError` enum (thiserror) + `IntoResponse` impl |
| `src/state.rs` | `AppState` struct holding the DB connection |
| `src/static_assets.rs` | SPA fallback via `rust-embed` |
| `web/` | SvelteKit project (frontend) |
| `web/src/routes/` | Svelte page components (`+page.svelte`, `+layout.svelte`) |
| `web/src/lib/` | Shared modules: `api.ts` (fetch client), `validation.ts`, `types.ts` |
| `web/build/` | Build output (embedded into binary at compile time) |

## Development Commands

```bash
# Build and run (production)
cargo run --release

# Build release binary only
cargo build --release

# Run all Rust tests (~40 tests)
cargo test

# Run all frontend tests (~22 tests)
cd web && npm test

# Type-check frontend
cd web && npm run check

# Run with debug logging
RUST_LOG=debug cargo run

# Format + lint Rust
cargo fmt
cargo clippy

# Dev server (frontend only, for UI work)
cd web && npm run dev
```

**Build-time requirement**: Node.js 26+ (only during `build.rs` execution; not needed at runtime).

## Code Conventions & Common Patterns

### Rust

**File structure**: Flat single files (no nested `mod` directories). Each file has a single responsibility. Module declarations live in `main.rs`.

**Error handling**: Use `thiserror`-derived `AppError` enum. Every variant maps to an HTTP status code and JSON body `{"error": "..."}` via `IntoResponse`. No `unwrap`/`expect` in non-test code. Convert `rusqlite::Error` with `#[from]`, `serde_json::Error` with a manual `From` impl.

**Async pattern**: Handlers are `async fn` returning `Result<Json<T>, AppError>` or `Result<(StatusCode, Json<T>), AppError>`. DB access goes through `state.conn.lock().await` using `tokio::sync::Mutex` (not `std::sync::Mutex` — would block the executor).

**State injection**: Axum `State(Arc<AppState>)` extractor. `AppState` holds the `Mutex<Connection>`.

**Logging**: `tracing` with `#[instrument(skip(state))]` on handlers. `tracing-subscriber` with `EnvFilter` (default `info`, overridable via `RUST_LOG`).

**Validation**: `db::validate_meal()` — name: 1–200 chars, ingredients: 1–5000 chars after trim. Both backend and frontend enforce the same limits. Validation runs inside `insert_meal` and `update_meal` before touching the DB.

**Testing**: 
- DB tests: synchronous `#[test]`, use `tempfile::TempDir` for isolated databases. 
- Route tests: `#[tokio::test]`, use `tower::ServiceExt::oneshot` to send `Request` objects to the router. Helper `TestCtx` struct holds the app and temp directory.
- Naming convention: `given_<precondition>_when_<action>_then_<expected_result>`.

**Dependencies**: Minimal crates — `axum` (http1+json+query+macros), `rusqlite` (bundled), `serde`/`serde_json`, `chrono`, `tokio`, `tracing`/`tracing-subscriber`, `thiserror`, `rust-embed`. Dev: `tempfile`, `tower` (util).

### Frontend (Svelte 5 + TypeScript)

**Framework**: Svelte 5 runes mode (`$state`, `$effect`, `$props`). SvelteKit with `adapter-static` (SPA fallback mode). SSR disabled (`ssr: false` in `vite.config.ts`).

**State management**: Component-local `$state` variables. No stores, no external state library.

**API client**: `web/src/lib/api.ts` — thin `fetch` wrapper. Generic `request<T>()` handles JSON parsing, error extraction from `{"error":"..."}`, and 204 No Content. Exported functions: `listMeals`, `createMeal`, `updateMeal`, `deleteMeal`.

**Validation**: `web/src/lib/validation.ts` mirrors backend constraints. Returns discriminated union `{ ok: true } | { ok: false; field; message }`.

**Testing**: `vitest` with `@sveltejs/kit/vite` plugin. Tests in `src/**/*.test.ts`. Mock `globalThis.fetch` with `vi.fn()`. Test files use BDD-style `describe`/`it` blocks.

**Styling**: Single `app.css` — system-ui font, 720px max-width centered layout, minimal clean styling. No CSS framework.

**TypeScript**: Strict mode, `moduleResolution: "bundler"`. Types in `web/src/lib/types.ts`: `Meal` interface and `MealPayload`.

## Important Files

| File | Role |
|------|------|
| `Cargo.toml` | Binary name `mealme`, edition 2024, Rust 1.85+ |
| `build.rs` | Auto-runs `npm install && npm run build` in `web/` |
| `src/main.rs` | Router assembly, port binding, logging init |
| `src/db.rs` | All SQL queries, validation, `init_db` schema creation |
| `src/routes.rs` | Five handlers: list, get, create, update, delete |
| `src/error.rs` | `AppError` → HTTP response mapping |
| `src/state.rs` | `AppState` (Mutex-wrapped Connection) |
| `src/static_assets.rs` | MIME type mapping, SPA fallback logic |
| `web/package.json` | Scripts: `dev`, `build`, `test`, `check` |
| `web/vite.config.ts` | SvelteKit plugin, adapter-static, SSR disabled |
| `web/vitest.config.ts` | Test include pattern: `src/**/*.test.ts` |
| `web/src/routes/+page.svelte` | Entire app UI — form, list, search, edit, delete |
| `web/src/lib/api.ts` | Fetch client for all API endpoints |

## Runtime/Tooling Preferences

- **Rust**: 1.85+ (edition 2024). Use `rustfmt` and `clippy`.
- **Node.js**: 26+ (build-time only, for SvelteKit/Vite compilation).
- **Package manager**: npm (no pnpm/yarn/bun).
- **Port**: `127.0.0.1:11341` (hardcoded in `main.rs`).
- **Database**: SQLite via `rusqlite` with `bundled` feature — no system SQLite needed.
- **TLS**: `rustls` (project rule: no OpenSSL), though no TLS endpoints exist yet.
- **No CI config**: No `.github/workflows/`, `Jenkinsfile`, or `Makefile` exists at time of writing.

## Testing & QA

### Rust tests (`cargo test`)

- **Location**: Inline `#[cfg(test)] mod tests` within each source file.
- **DB layer** (`db.rs`, ~18 tests): Unit tests for CRUD, validation edge cases (empty strings, boundary lengths, whitespace-only), search filtering (by name, ingredients, both, whitespace search term).
- **Route layer** (`routes.rs`, ~7 tests): Integration tests using `tower::ServiceExt::oneshot`. Verify status codes, response bodies, search filtering, 404 for missing resources.
- **Error layer** (`error.rs`, ~6 tests): Verify each `AppError` variant maps to correct status code and JSON body.
- **Model layer** (`model.rs`, ~3 tests): Serde round-trip and field deserialization.
- **Static assets** (`static_assets.rs`, ~3 tests): Verify SPA fallback returns index.html, correct MIME types.
- **Isolation**: Every test uses `tempfile::TempDir` for fresh databases. No shared state between tests.

### Frontend tests (`cd web && npm test`)

- **Framework**: Vitest 4 with `@sveltejs/kit/vite` plugin.
- **API tests** (`api.test.ts`, ~6 tests): Mock `fetch`, verify correct URL/method/body/headers for each API function, test error message extraction.
- **Validation tests** (`validation.test.ts`, ~8 tests): Boundary and edge-case validation (empty, whitespace, exact-limit, one-over-limit).
- **Page validation tests** (`page-validation.test.ts`, ~7 tests): Same validation logic imported via `$lib` alias (duplicate coverage from page level).

### E2E tests

E2E tests are specified in `spec.md` (Playwright-based, headless Chromium, auto-starts/terminates binary, fresh DB per run) but **not yet implemented**.
