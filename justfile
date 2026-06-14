# Default recipe: list available recipes
default:
	@just --list

# Build the mealme binary
build:
	cargo build

# Install Playwright + Chromium (one-time per checkout)
e2e-install:
	cd tests && npm install
	cd tests && npx playwright install --with-deps chromium

# Run the full E2E test suite (builds first; assumes e2e-install was run)
e2e: build
	cd tests && npx playwright test

# Run tests with a visible browser for local debugging
e2e-headed: build
	cd tests && npx playwright test --headed

# Open Playwright's interactive UI mode
e2e-ui: build
	cd tests && npx playwright test --ui
