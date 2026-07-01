# Feature Specification: Minify and Simplify Dependencies

**Created**: 2026-07-01
**Status**: Approved
**Input**: minify / simplify dependencies, reduce features

## Goal
Reduce the Rust and frontend dependency tree by removing dead dependencies and trimming Cargo feature-flags to the minimum each retained crate actually requires. No application feature or behavior is removed, changed, or degraded — the only observable effects are a smaller transitive crate tree, faster compiles, and a leaner lockfile.

## User Scenarios

### Scenario 1 - Developer trims Cargo feature-flags (P1)
A developer building the project wants the smallest dependency surface that still compiles and passes every test. They edit `Cargo.toml` to drop feature-flags that no source file exercises, then verify the build and test suite are green.

**Acceptance**
1. Given the current `Cargo.toml` with tokio `["macros", "rt-multi-thread", "net", "sync"]` When the developer removes `"sync"` from the tokio feature list Then `cargo build --release` succeeds and `cargo test` passes, because tokio's `sync` feature is pulled transitively by sqlx via tokio-stream and is not referenced directly in `src/`.
2. Given the current `sqlx` features `["runtime-tokio", "sqlite", "macros", "migrate", "chrono"]` When the developer replaces `"macros"` with `"derive"` Then `cargo build --release` succeeds, because `derive` enables the `FromRow` derive macro (used in `model.rs` and `db.rs`) and activates the `sqlx-macros` optional dependency that `migrate!()` delegates to, while dropping the unused `query!`/`query_as!`/`query_scalar!` compile-time query checking and `offline` mode.
3. Given the current axum features `["json", "query", "macros", "tokio", "http1", "multipart"]` When the developer removes `"macros"` Then `cargo build --release` succeeds, because the codebase uses no `#[debug_handler]`, `#[debug_middleware]`, or `FromRequest` derive — all other extractors (`Path`, `Query`, `State`, `Multipart`, `DefaultBodyLimit`) are available without the `macros` feature.

### Scenario 2 - Developer removes dead frontend dependency (P2)
A developer notices `@sveltejs/adapter-auto` is listed as a devDependency but the project's `vite.config.ts` imports `@sveltejs/adapter-static`. The adapter-auto package is never referenced anywhere in `web/src/` and adds dead weight to `node_modules` and the lockfile.

**Acceptance**
1. Given `web/package.json` lists `@sveltejs/adapter-auto` under `devDependencies` When the developer removes that line and runs `npm install` Then `npm run build` and `npm run check` succeed, because `vite.config.ts` imports `adapter-static` and adapter-auto is never imported by any source file.
2. Given adapter-auto has been removed When the developer runs `npm test` Then the Vitest suite passes, confirming the adapter was not referenced by any test.

## Functional Requirements

- **FR-001**: The `tokio` dependency in `Cargo.toml` MUST specify `features = ["macros", "rt-multi-thread", "net"]` — the `"sync"` feature MUST be removed. Rationale: `#[tokio::main]` needs `macros`; the multi-threaded runtime needs `rt-multi-thread`; `tokio::net::TcpListener` in `main.rs` needs `net`. No source file references `tokio::sync::*`; sqlx activates `sync` transitively via tokio-stream.
- **FR-002**: The `sqlx` dependency in `Cargo.toml` MUST specify `features = ["runtime-tokio", "sqlite", "migrate", "chrono", "derive"]` — the `"macros"` feature MUST be replaced with `"derive"`. Rationale: `derive` enables `#[derive(sqlx::FromRow)]` (used in `model.rs` lines 18, 56 and `db.rs` lines 22, 48) and activates the `sqlx-macros` optional dependency; `sqlx::migrate!("./migrations")` (`db.rs` line 69) expands to `sqlx_macros::migrate!()`, which requires `sqlx-macros` to be present. The `macros` feature additionally enables `query!`/`query_as!` compile-time checking and `offline` mode — neither used (the code uses runtime `sqlx::query()`).
- **FR-003**: The `axum` dependency in `Cargo.toml` MUST specify `features = ["json", "query", "tokio", "http1", "multipart"]` — the `"macros"` feature MUST be removed. Rationale: the `macros` feature gates `#[debug_handler]`, `#[debug_middleware]`, and the `FromRequest` derive macro; the codebase uses none (verified). All extractors in use (`Json`, `Path`, `Query`, `State`, `Multipart`, `DefaultBodyLimit`) are available without it.
- **FR-004**: The `@sveltejs/adapter-auto` entry MUST be removed from `web/package.json` `devDependencies`. Rationale: `vite.config.ts` imports `@sveltejs/adapter-static`; adapter-auto is never imported anywhere in `web/src/` and serves no purpose.
- **FR-005**: No other dependency's feature list MAY be changed. Specifically: `image` keeps `["jpeg", "png", "gif", "bmp", "webp"]` (all five codecs retained — no behavior change); `chrono` keeps `["serde", "clock"]` (both in use); `reqwest` keeps `["rustls", "webpki-roots", "json", "form"]` (`.json()` and `.form()` both used in `bring.rs`).
- **FR-006**: No direct dependency crate MAY be removed from `Cargo.toml` or `package.json`. Every current direct dependency serves an active application feature (genai→LLM import, recipe-scraper/scraper/ammonia→recipe import, reqwest→bring/llm/recipe HTTP, image→meal images, base64→LLM image encoding, rand→seed/planner, etc.).

## Edge Cases

- **Cargo feature unification**: If removing `tokio/sync` from the manifest causes a compile failure (because a transitive dependency relied on the application crate to enable it rather than declaring it itself), restore `"sync"` to the tokio feature list. Expected outcome: sqlx already enables `sync` via tokio-stream, so removal should succeed; the contingency is there in case feature unification behaves unexpectedly.
- **sqlx `migrate!` macro breakage**: If replacing `macros` with `derive` breaks the `migrate!()` macro, it means `sqlx-macros` was not activated. In that case, restore `"macros"` — the `derive` feature alone activates `sqlx-macros` (per sqlx 0.9 `Cargo.toml`: `derive = ["sqlx-macros/derive"]`), so this should not occur.
- **axum handler compilation**: If removing `macros` causes handler signature compile errors, restore `"macros"`. Expected: all extractors used are non-macro; `#[instrument]` (from tracing, not axum) is used for logging.
- **Frontend lockfile**: Removing adapter-auto requires `npm install` to regenerate `package-lock.json`. If any transitive devDependency depended on adapter-auto, `npm run build` or `npm run check` would fail — in that case restore it (expected: adapter-auto is a leaf devDependency with no dependents).

## Research Notes

- sqlx 0.9 `Cargo.toml` (crate source at `~/.cargo/registry/.../sqlx-0.9.0/Cargo.toml`): `derive = ["sqlx-macros/derive"]`; `macros = ["derive", "sqlx-macros/macros", "sqlx-core/offline", ...]` — confirms `macros` implies `derive`, and `derive` alone activates `sqlx-macros`.
- sqlx 0.9 `src/macros/mod.rs` lines 861-871: `#[cfg(feature = "migrate")] #[macro_export] macro_rules! migrate { ... $crate::sqlx_macros::migrate!(...) }` — confirms `migrate!()` delegates to `sqlx-macros`, which requires `derive` or `macros` to be activated.
- axum docs (docs.rs/axum): `#[debug_handler]` is "available on crate feature `macros` only" — confirms `macros` feature gates only debug-handler/debug-middleware/FromRequest derive.
- Baseline measurement: `cargo tree` reports 16 direct Rust deps → 278 unique transitive crates; `genai` 0.6 is the single largest contributor (~365 transitive crates) and remains because the LLM image import feature is retained.

## Assumptions

- No application features are removed or modified — the brief refers exclusively to cargo/npm dependency minification (user explicitly corrected the framing: "i was talking about cargo crates not removing features").
- The SQL layer stays on `sqlx` 0.9 (user chose "Keep sqlx" over reverting to `rusqlite`); AGENTS.md's documentation of `rusqlite` is stale and out of scope for this spec.
- The `image` crate retains all five codec features (`jpeg`, `png`, `gif`, `bmp`, `webp`) — zero behavior change for users uploading any browser-supported image format (user chose "Keep all five").
- The dependency trimming is verified by the existing test suite: `cargo test` (Rust unit/integration), `cd web && npm test` (Vitest), `cd web && npm run check` (frontend type-check), and `cargo build --release` (production build including the `build.rs` frontend embed). No new tests are written.

## Success Criteria

- **SC-001**: `cargo build --release` succeeds with the trimmed `Cargo.toml` feature-flags (tokio without `sync`, sqlx with `derive` instead of `macros`, axum without `macros`).
- **SC-002**: `cargo test` passes with the trimmed feature-flags — all existing Rust unit and integration tests remain green, confirming no behavioral regression.
- **SC-003**: `cd web && npm install` succeeds after removing `@sveltejs/adapter-auto` from `web/package.json`, confirming it was a dead dependency with no dependents.
- **SC-004**: `cd web && npm run build` and `cd web && npm run check` succeed after the removal, confirming `adapter-static` (not `adapter-auto`) is the active adapter.
- **SC-005**: `cd web && npm test` passes after the removal, confirming no test referenced `adapter-auto`.
- **SC-006**: The unique transitive crate count (measured via `cargo tree | grep -oE '^[│ ]*[├└]── [a-z0-9_-]+' | sort -u | wc -l`) is lower than the pre-change baseline of 278, confirming the dependency tree shrank.
