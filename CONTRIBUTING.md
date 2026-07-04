# Contributing to YummyBox

Thanks for considering contributing!

## Development workflow

### Prerequisites

- **Rust** 1.85+ (with Cargo)
- **Node.js** 26+ (build-time only)
- **`just`** (optional, for E2E workflow)

### Setup

```bash
git clone https://github.com/RouHim/yummybox.git
cd yummybox
```

Building the first time will automatically install frontend dependencies and compile the SPA:

```bash
cargo build
```

### Running tests

```bash
# Rust unit + integration tests
cargo test

# Frontend Vitest tests
cd web && npm test

# E2E tests (Playwright)
cd tests && npm ci && npx playwright install --with-deps chromium && just e2e
```

### Linting

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo deny check
cd web && npm run check
```

## Commit conventions

This project uses [Conventional Commits](https://www.conventionalcommits.org/). Semantic-release automates versioning and changelog generation based on commit messages.

Valid types: `feat`, `fix`, `chore`, `docs`, `refactor`, `test`, `ci`.

## Architecture

See [AGENTS.md](AGENTS.md) for project architecture, data flow, and code conventions.

## Pull requests

1. Fork the repo and create a branch from `main`.
2. Run tests and lint before submitting.
3. Ensure your PR title follows Conventional Commits format.
4. Reference any related issues.
