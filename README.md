# MealMe

A single-binary local-first web application for managing a personal collection of meals. Built with Rust (axum + rusqlite) and Svelte 5, all frontend assets embedded in the binary.

## Quickstart

```bash
cargo run --release
```

Then open **http://127.0.0.1:11341** in your browser.

The server listens on `127.0.0.1:11341` and persists data in `./meals.db` (SQLite, auto-created on first run).

## Requirements

- **Rust** 1.85+ (with Cargo)
- **Node.js** 26+ (build-time only — `build.rs` runs `npm install && npm run build` in `web/`)
- **`just`** (optional, for E2E workflow — install via `cargo install just` or your package manager)

No Docker, nginx, or Node.js runtime needed after compilation.

## Development

```bash
# Run all Rust tests (40 tests)
cargo test

# Run all frontend tests (22 tests)
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
| POST | `/api/meals` | Create a meal `{"name":"...","ingredients":"..."}` |
| GET | `/api/meals/:id` | Get a single meal |
| PUT | `/api/meals/:id` | Update a meal |
| DELETE | `/api/meals/:id` | Delete a meal |

Validation: name (1–200 chars), ingredients (1–5000 chars). All API paths are under `/api`; everything else serves the SPA frontend.
