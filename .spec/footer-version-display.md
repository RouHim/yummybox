# Feature Specification: Footer application version display

**Created**: 2026-07-03
**Status**: Approved
**Input**: Show the version of the application on the right side of the footer.

## Goal
Display the running application's version string on the right side of the page footer, sourced from the shipped Rust binary's `CARGO_PKG_VERSION`, so users can identify the exact version they are running. The version links to the project's GitHub releases page.

## User Scenarios
### Scenario 1 - View version in footer (P1)
A user opens any page of the MealMe app and glances at the footer. They see the bare semver version string (e.g. `0.1.0`) on the right side, after the existing photo attribution.

**Acceptance**
1. Given the app is running When the user views any page Then the footer's right side displays the bare semver version (e.g. `0.1.0`) sourced from the backend.
2. Given the version is displayed When the user clicks it Then the browser navigates to `https://github.com/RouHim/mealme/releases` in a new tab.

### Scenario 2 - Version reflects the shipped binary (P1)
The version shown must match the `version` field in `Cargo.toml` of the actually-running binary — not a frontend-only constant.

**Acceptance**
1. Given the backend binary is built from Cargo.toml version `X.Y.Z` When the frontend requests `/api/version` Then the response body is `{"version": "X.Y.Z"}`.
2. Given the backend is running When the frontend fetches `/api/version` on app mount Then the footer displays the returned version string.

### Scenario 3 - Backend unreachable / fetch failure (P2)
If the version endpoint cannot be reached or returns an error, the footer must remain intact and simply omit the version text/link.

**Acceptance**
1. Given `/api/version` returns a non-200 status or the fetch rejects When the frontend handles the response Then no version text is rendered and the footer's other content (attribution, bringError) is unaffected.

## Functional Requirements
- **FR-001**: The backend SHALL expose a `GET /api/version` route returning `200 OK` with JSON body `{"version": "<CARGO_PKG_VERSION>"}`, where `<CARGO_PKG_VERSION>` is the compile-time cargo package version string.
- **FR-002**: The `/api/version` handler SHALL NOT acquire the database lock; it returns a compile-time constant.
- **FR-003**: The `/api/version` route SHALL be registered in the application router alongside existing `/api/*` routes.
- **FR-004**: The frontend SHALL fetch `/api/version` once on layout mount and render the returned bare semver string on the right side of `.site-footer`.
- **FR-005**: The version string SHALL be wrapped in an anchor element linking to `https://github.com/RouHim/mealme/releases` with `target="_blank"` and `rel="noopener"`.
- **FR-006**: The version display SHALL use the existing `.site-footer` muted text styling; the footer's `justify-content: space-between` layout keeps attribution on the left and version on the right.
- **FR-007**: If the `/api/version` fetch fails or returns a non-200 status, the frontend SHALL render no version element and SHALL NOT surface an error to the user.

## Key Entities
- **AppVersion**: the compile-time `CARGO_PKG_VERSION` string exposed via `GET /api/version` as `{"version": "<string>"}`.

## Edge Cases
- `/api/version` fetch fails or returns non-200: omit version text entirely; footer attribution and bringError span are unaffected.
- Version string is empty or malformed: rendered as-is (no transformation); link still points to the releases page.
- bringError span is present (middle of footer): version remains on the right; flexbox `justify-content: space-between` keeps the three regions (left attribution, middle error, right version) separated.

## Assumptions
- `/api/version` is unauthenticated — MealMe has no authentication system.
- No response caching is required on the backend; the value is a compile-time constant. The frontend fetches once on mount.
- No i18n key is needed — the version is a bare semver string with no localizable label.
- The releases link target is the releases list page (`/releases`), not a tag-specific URL, because no release tags exist yet.
- The version source is the Rust binary's `CARGO_PKG_VERSION` (single source of truth = the shipped binary), not the frontend `package.json` version.

## Success Criteria
- **SC-001**: `curl -s http://127.0.0.1:11341/api/version` returns `{"version":"0.1.0"}` (matching `Cargo.toml`) with HTTP 200.
- **SC-002**: In the running app, the footer's right side shows `0.1.0` as a clickable link to `https://github.com/RouHim/mealme/releases` opening in a new tab.
- **SC-003**: Stopping the backend (or returning 500 from `/api/version`) causes the footer to render without the version text while the attribution and bringError regions remain intact.
