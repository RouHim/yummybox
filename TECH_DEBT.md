# YummyBox Tech-Debt Audit — 2026-07-30

Method: manual review of all 15 Rust sources and key frontend files, two parallel audit passes (frontend; tests/CI), `svelte-check` execution, and `cargo tree` dependency analysis. Every finding cites the code it was read from. Line numbers reflect the working tree on 2026-07-30.

## P0 — Broken quality gates

### 1. `svelte-check` is red (10 errors) and CI never runs it

**Evidence:** `cd web && ./node_modules/.bin/svelte-check --tsconfig ./tsconfig.json` → `10 ERRORS 10 WARNINGS 7 FILES_WITH_PROBLEMS`. `.github/workflows/ci.yml` `frontend-tests` job runs only `npm ci` + `npm test` — no `npm run check` anywhere in the workflow, so the red state is invisible to CI.

The 10 errors decompose into three bugs:

- **1a. Missing i18n key `plannerChangeWeek`** — used at `web/src/routes/planner/+page.svelte:309` (`aria-label={t('plannerChangeWeek')}`) but absent from the `TranslationKey` union (`web/src/lib/i18n/types.ts`) and from `en.ts`/`de.ts`. `t()` falls back to returning the raw key string (`web/src/lib/i18n/state.svelte.ts:24-27`), so screen readers announce "plannerChangeWeek".
  **Fix:** add `'plannerChangeWeek'` to the union and both dictionaries.
- **1b. Test fixtures not updated for `portions`** — `web/src/lib/api.test.ts` lines 48, 64, 82, 93, 104, 118, 127 construct `Meal` objects without the required `portions` field (added to the interface in `web/src/lib/types.ts:14`).
  **Fix:** add `portions: null` to the 7 fixtures.
- **1c. Null-safety errors in cooking view** — `web/src/routes/meals/[id]/+page.svelte:254` and `:261` use `meal.portions!` where `meal` is `Meal | null`.
  **Fix:** narrow with `{#if meal}` / a local non-null variable instead of `!`.

**Fix (gate):** add a `npm run check` step to the `frontend-tests` CI job after `npm ci`.

## P1 — Dead code & data

### 2. Dead code hidden behind `#[allow(dead_code)]` (violates the repo's own YAGNI rule)

- `src/error.rs:8` — `#[allow(dead_code)]` on the whole `AppError` enum hides the unused variant `UnprocessableEntity`, which is constructed only inside error.rs's own test (`src/error.rs:189`). **Fix:** delete the variant, its match arm (`src/error.rs:53-55`), its test, and the enum-level allow.
- `src/db.rs:565-566` — `#[allow(dead_code)] pub fn week_of_date` — its only callers are itself (recursion) and its own test (`src/db.rs:1854`). **Fix:** delete function + test + allow.

### 3. 28 dead i18n keys

Verified: no reference anywhere outside `web/src/lib/i18n/` (types.ts union + en.ts + de.ts):

`appSubtitle`, `fieldIngredientsPlaceholder`, `sectionAllMeals`, `validationIngredientsTooLong`, `errorRequestFailed`, `errorPlanFailed`, `errorPlanEmpty`, `errorPlanNotFound`, `plannerYearPrev`, `plannerYearNext`, `plannerMonthLabel`, `plannerDayEmpty`, `plannerOpen`, `plannerCurrentWeek`, `fieldImageLabel`, `fieldImageCurrent`, `imagePasteHint`, `imageDropPrompt`, `cookingViewTitle`, `currentWeekTitle`, `currentWeekEmpty`, `llmHintLabel`, `llmImageLabel`, `importBulkProgress`, `polishLoading`, `importZipErrorArchiveTooLarge`, `importZipErrorTooManyRecipes`, `cookingViewScaledQuantity`.

Notably `validationIngredientsTooLong` still carries stale text referencing the removed "ingredients ≤ 5000 chars" rule (`en.ts:40`, `de.ts:40`) — the backend now validates per-line limits.

**Fix:** delete each key from the union and both dictionaries; re-grep each key before deleting (dynamic key construction could create false positives).

### 4. Duplicate `TranslationKey` union members

`web/src/lib/i18n/types.ts` — `ingredientCount`, `ingredientCountOne`, `lastPlanned`, `lastPlannedNever` each appear twice (once near the top of the union, once mid-file after the `bringSend` block). The file also mixes tab and space indentation. **Fix:** remove the 4 duplicates, normalize indentation.

## P1 — Duplication

### 5. Multipart meal parsing duplicated between create and update handlers

`src/routes.rs` — `create_meal` (lines ~100-179) and `update_meal` (lines ~195-281) each contain an identical ~80-line multipart-parsing block: same field loop, same error strings (compare routes.rs:110/205, 115/210, 121/216, 127/222, 133/228, 139/234). **Fix:** extract one `parse_meal_multipart(multipart) -> Result<ParsedMealForm, AppError>` helper used by both handlers.

### 6. `focusTrap` copy-pasted into 4 components

`web/src/lib/DeleteConfirmDialog.svelte:25`, `web/src/routes/meals/+page.svelte:409` (comment literally says "matches DeleteConfirmDialog.svelte focusTrap"), `web/src/routes/meals/[id]/+page.svelte:110`, `web/src/routes/planner/+page.svelte:190`. **Fix:** extract a single Svelte action (e.g. `web/src/lib/focusTrap.ts`) and import it in all four.

### 7. `__REQUEST_FAILED__` magic-string error sentinel

Thrown as a sentinel message in `web/src/lib/api.ts:13` and again in the duplicated inline error-extraction block in `getPlan` (`api.ts:100-110`, which also bypasses the shared `request<T>` helper and throws plain `Error`, not `ApiError`). Compared against at 8+ call sites: `routes/+page.svelte:33`, `routes/meals/+page.svelte:103,132,193,220,343,400`, `lib/MealForm.svelte:113`, `routes/planner/+page.svelte:119`. **Fix:** add a discriminating field to `ApiError` (e.g. `isGeneric`/`code`) instead of a magic message; route `getPlan` through the shared error-extraction path.

### 8. Duplicate PNG builders in E2E specs

`tests/e2e/planner.spec.ts:6` (`buildMiniPng`) vs `tests/e2e/meal-images.spec.ts:9` (`buildPng`) — same IHDR/IDAT/IEND construction logic. **Fix:** extract `tests/e2e/_png.ts` and import from both.

### 9. `page-validation.test.ts` is a strict subset of `validation.test.ts`

`web/src/lib/page-validation.test.ts` — 8 cases, all present among the 12 in `web/src/lib/validation.test.ts`; identical helpers; no unique coverage. **Fix:** delete the file.

### 10. Duplicate section header

`src/llm_import.rs:198` and `:235` both read `// Main import function`. **Fix:** delete the stray header at :198.

## P1 — Backend design

### 11. N+1 ingredient queries on every list/search

`src/db.rs:273-280` — `hydrate_meals` runs one `SELECT` per meal; `list_meals` calls it after every list/search query. **Fix:** one batched query (`WHERE mi.meal_id IN (...)` over the loaded ids), grouped by `meal_id` in Rust. Low user impact at personal scale; pure efficiency debt.

### 12. `AppError::Database` leaks internals; `"code": null` noise on every error body

`src/error.rs:56` — the raw sqlx error text (schema details, constraint names) is shipped in the 500 JSON body. `src/error.rs:71` — every non-LLM error body carries `"code": null`. **Fix:** log the sqlx error server-side (`tracing::error!`), return a generic `database error` message to the client; omit `code` when `None`.

### 13. `unwrap`/`expect` in non-test code (repo rule: none)

`src/db.rs:539` and `:554` (`NaiveDate::from_ymd_opt(...).unwrap()`), `src/recipe.rs:109` (`Selector::parse(...).expect`), `src/static_assets.rs:38,40,45` (Response builder + embedded index), `src/routes.rs:494` (`hint.as_deref().unwrap()` right after an `is_some_and` check). **Fix:** routes.rs:494 refactors cleanly to `if let Some(h) = ...`; the rest are invariant-protected — convert to `expect` with an explicit invariant message or restructure to infallible construction.

### 14. Lint suppressions instead of fixes

`src/db.rs:1` file-wide `#![allow(clippy::explicit_auto_deref)]`; `src/db.rs:698` `#[allow(clippy::type_complexity)]` (a type alias fixes it); `src/model.rs:128` `#[allow(unused_imports)]` in tests. **Fix:** remove or narrow each; verify with `cargo clippy --all-targets --all-features -- -D warnings`.

### 15. Monolith files breach the repo's "one file = one concern" rule

`src/db.rs` (2560 lines: meals + plans + images + week math + weighted selection + ingredient aggregation) and `src/routes.rs` (~2600 lines: meals, image, 4 import modes, LLM proxy, plans, Bring, version). On the frontend: `web/src/routes/planner/+page.svelte` (1284 lines) and `web/src/routes/meals/+page.svelte` (1111 lines). **Fix (bounded):** extract `src/plan.rs` (week math + plan CRUD) and `src/import.rs` (import/LLM handlers) as flat modules — the flat-layout rule allows more files; a frontend component split needs its own design pass and is not specified here.

## P2 — Tests / CI / tooling

### 16. No npm caching in CI

All 7 `actions/setup-node@v4` steps (`.github/workflows/ci.yml:20,36,50,68,120,168,232`) lack `cache: 'npm'`; every job downloads ~100+ packages from scratch. **Fix:** add `cache: 'npm'` + `cache-dependency-path` (both `web/package-lock.json` and `tests/package-lock.json`) to each.

### 17. Container CI job installs `web/` deps it never uses

`build-container` job runs `cd tests && npm ci && cd ../web && npm ci`, but only ever runs `cd tests && npm test` (the web/e2e suite never runs against the container). **Fix:** drop `cd ../web && npm ci` from that job.

### 18. Playwright configs vs pinned cargo target — binary path mismatch

`.cargo/config.toml` pins `target = "x86_64-unknown-linux-gnu"`, so `cargo build --release` produces `target/x86_64-unknown-linux-gnu/release/yummybox`. But `web/playwright.config.ts:12` runs `./target/release/yummybox` (fails locally — no fallback), and `tests/playwright.config.ts:24` checks `target/release/yummybox` first then falls back to `cargo run --quiet` (works, but pays cargo startup every run). CI only works because artifact download flattens the path. **Fix:** check the triple-prefixed path first in both configs.

### 19. Local E2E race: `fullyParallel` + shared DB

`tests/playwright.config.ts:5,8` — `fullyParallel: true` with `workers` unset locally (CI forces 1), while all tests share one `YUMMYBOX_DATA_DIR=./.e2e-db`. Parallel `resetMeals` + CRUD can race locally. **Fix:** `workers: 1` unconditionally (matches CI), or per-worker DB directories.

### 20. Hard wait in planner spec

`tests/e2e/planner.spec.ts:241` — `await page.waitForTimeout(500)` for lazy-loaded thumbnails. **Fix:** replace with a locator visibility assertion.

### 21. `cooking-view.spec.ts` breaks the reset convention

`tests/e2e/cooking-view.spec.ts:6,27` calls `resetMeals(request)` inside test bodies; the other ~20 specs use `test.beforeEach`. **Fix:** move to `beforeEach`.

### 22. No declared Node version

No `engines` in `web/package.json` or `tests/package.json`, no `.nvmrc`; Node 26 exists only in CI YAML and AGENTS.md. **Fix:** add `"engines": { "node": ">=26" }` to both package.json files and a `.nvmrc` containing `26`.

### 23. Duplicate `windows-sys` in Cargo.lock

`windows-sys 0.52.0` (via `ring` → rustls) and `0.61.2` (via mio/errno/tokio etc.) — `cargo deny` (`multiple-versions = "warn"` in `deny.toml`) reports it. `ring` has not moved to 0.61. **Fix:** try `cargo update -p ring`; if 0.52 remains, add an explicit `skip` entry in `deny.toml` with a comment noting the crate is Windows-only and unreachable on the pinned Linux target. Advisory-level only.

### 24. Inconsistent loopback addresses in Playwright configs

`web/playwright.config.ts:8` uses `http://127.0.0.1:11341`; `tests/playwright.config.ts:15` uses `http://localhost:11342` (IPv6 resolution differences possible). **Fix:** unify on one form in both.

## P3 — Documentation drift

### 25. AGENTS.md describes a project that no longer exists

Verified mismatches:

- Says **rusqlite** + `tokio::sync::Mutex<Connection>` — actual: **sqlx 0.9** `SqlitePool` (`src/state.rs`, `src/db.rs:60-77`, `migrations/` with 4 migration files).
- Lists **7 src files** — actual **15** (missing: `image.rs`, `recipe.rs`, `jsonld.rs`, `llm_import.rs`, `bring.rs`, `export_import.rs`, `seed.rs`, `data_dir.rs`).
- Says `web/src/routes/+page.svelte` is the "entire app UI" — actual: 4 route components (home, planner, meals, meals/[id]) + `MealForm.svelte`, `ImageInput.svelte`, i18n/theme/llm-config libs.
- Wrong validation limits ("name 1–200, ingredients 1–5000") — actual: name ≤200, instructions 1–20000, ≤100 ingredient lines (name ≤100, quantity ≤50), portions 1–10000 (`src/db.rs:96-160`).
- Says port is hardcoded — actual: `YUMMYBOX_PORT` env override (`src/main.rs`).
- Omits entire features: planner, portions, meal images, i18n, theme, LLM import + polish, bulk/zip import, zip export, Bring! integration, `seed` subcommand, `/api/version`.
- Stale test counts and E2E spec tables (22 spec files in `tests/e2e/`, 8 listed).

**Fix:** rewrite the affected AGENTS.md sections during the next code-change pass.

### 26. Bind address mismatch — DECIDED

AGENTS.md says "listens on `127.0.0.1:11341`"; `src/main.rs` binds `0.0.0.0:{port}`. **Decision (2026-07-30): keep `0.0.0.0`** (containers require it; LAN access by default is accepted). **Fix when next touched:** correct AGENTS.md to state the 0.0.0.0 default.

## Checked — clean (no action)

- No E2E duplication between `web/e2e/` (visual) and `tests/e2e/` (workflows); DB/port isolation between suites is correct.
- Node 26 consistent across all CI jobs; Gitleaks job present (`ci.yml:330`); Playwright browser cache present.
- No hardcoded UI strings bypass i18n (only the photographer attribution proper nouns — intentional).
- `web/src/lib/validation.ts` correctly mirrors current backend limits.
- Svelte state is `$state`-based throughout; no stores.
- `build.rs`, `vite.config.ts`, `vitest.config.ts` minimal; no dead code.
- The Bring! API key in `src/bring.rs:6` is Bring's public Android client credential (public knowledge, passes Gitleaks) — accepted as-is.
- `web/src/lib/theme.ts` barrel and `$lib/i18n` structure are the established import conventions — not debt.
- Structurally identical `IngredientQuantity`/`NewIngredientLine` in `types.ts` — intentional request/response separation.
