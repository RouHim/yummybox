# E2E Tests

Playwright-based end-to-end tests for YummyBox.

Cargo's Rust integration test convention (`tests/*.rs`) does **not** apply here — all files are under `tests/e2e/`, and the `package.json`/`playwright.config.ts` live at the `tests/` level. No Rust tests are in this directory.

## Setup

```bash
just e2e-install    # installs Playwright + Chromium (one-time)
```

## Running

```bash
just e2e            # headless (full suite)
just e2e-headed     # with visible browser
just e2e-ui         # Playwright interactive UI
```

Or directly:

```bash
cd tests && npx playwright test
```

## Architecture

- `e2e/_helpers.ts` — shared helpers: `resetMeals()` (clears all meals via API) and `createMeal()`.
- `playwright.config.ts` — webServer starts `cargo run` with `YUMMYBOX_PORT=11342` and `YUMMYBOX_DATA_DIR=./.e2e-db`, health-checks `GET /api/meals`.

Each test file calls `resetMeals()` in `beforeEach` for isolation. The `.e2e-db/` directory is gitignored.
