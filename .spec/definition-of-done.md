# Feature Specification: Definition of Done — Release Hygiene, Pipeline, Branding, and Public Launch

**Created**: 2026-06-28
**Status**: Approved
**Input**: Reach DoD: Renovate + semantic-release pipeline + up-to-date minimal deps + (arm64?) + logo/banner + README + public repo + secret hygiene, using RouHim/strandgut as the infra/repo reference.

## Goal

Bring MealMe to the same infra/repo standard as the reference repository `RouHim/strandgut`: automated dependency updates via Renovate, automated semver releases via semantic-release with multilib (x86_64 + aarch64) static musl binaries attached to GitHub Releases, a CI pipeline mirroring strandgut's `ci.yml` job structure, minimal and up-to-date dependencies, the full strandgut meta-file set, a branded README with a tastefully generated logo and banner, and confirmed secret hygiene before the repository is flipped from private to public.

Strandgut is the reference template for every infra/repo artifact this spec introduces. Its `.github/workflows/ci.yml`, `.releaserc`, `renovate.json`, README badge row, `.github/readme/banner.svg`, and meta-file set (`LICENSE`, `CHANGELOG.md`, `CONTRIBUTING.md`, `SECURITY.md`, `.github/ISSUE_TEMPLATE/*`, `.github/PULL_REQUEST_TEMPLATE.md`) define the shape. Strandgut's container-image build job is explicitly OUT of scope — MealMe's deploy story is the single static binary, not a container. Strandgut's musl-static multilib binary build pattern IS in scope.

## User Scenarios

### Scenario 1 - Maintainer pushes a conventional commit to main (P1)

A maintainer pushes a commit message following Conventional Commits (e.g. `feat: add meal export`) to `main`. CI runs the full validation matrix: lint-format, rust-tests, and e2e-tests. If all jobs pass and the commit warrants a release, the create-release job runs semantic-release, which derives the next version from commit history, tags `vX.Y.Z`, bumps `Cargo.toml` version, appends to `CHANGELOG.md`, commits those changes back to `main`, and creates the GitHub Release. The build-binaries job then compiles static musl binaries for `x86_64-unknown-linux-musl` and `aarch64-unknown-linux-musl` inside `rust-cross` containers and attaches both binaries to the just-created release.

**Acceptance**

1. Given a push to `main` with a `feat:` commit When CI completes Then all of lint-format, rust-tests, and e2e-tests pass.
2. Given the three test jobs passed and `main` received a releasable conventional commit When the create-release job completes Then a new `vX.Y.Z` git tag exists, `Cargo.toml`'s `version` field equals that version, and `CHANGELOG.md` contains a new dated section for it.
3. Given a new release was published When the build-binaries job completes Then the GitHub Release for `vX.Y.Z` has two attached assets named `mealme-x86_64-unknown-linux-musl` and `mealme-aarch64-unknown-linux-musl`, each verified as statically linked.
4. Given a push to `main` with only `chore:` commits When the create-release job completes Then a patch release is created (the `chore → patch` release rule), not skipped.
5. Given a push to a non-`main` branch When CI completes Then no release is created and no binaries are built.

### Scenario 2 - Renovate opens a dependency-update PR (P1)

Renovate detects an outdated Cargo, npm, or GitHub Actions dependency, waits the configured minimum-release-age, and opens a branch PR with the bump. CI runs the full validation matrix against the PR. When all jobs are green, Renovate auto-merges the branch.

**Acceptance**

1. Given a Cargo dependency with a newer version released ≥7 days ago When Renovate next runs Then a PR is opened bumping that dependency.
2. Given a Renovate PR When the CI matrix completes green Then the PR is auto-merged by Renovate (automergeType `branch`).
3. Given a Renovate PR When any CI job fails Then the PR is NOT auto-merged.

### Scenario 3 - Visitor lands on the public repository (P1)

A visitor opens the now-public repo. The README renders a centered banner SVG, a badge row (CI/CD, license MIT, arch x86_64 | arm64, renovate-enabled), a features list, a quickstart (both `cargo run` and a `curl`-from-release one-liner), the API reference table, the LLM Import section, and links to Contributing, Security, and License. Issue and PR templates are available under `.github/`.

**Acceptance**

1. Given the repo is public When a visitor opens the README Then the centered banner SVG from `.github/readme/` renders and all four badges display.
2. Given a visitor wants the latest binary When they follow the README quickstart `curl` command Then they obtain a working `mealme` binary for their architecture (x86_64 or aarch64).
3. Given a visitor clicks "New Issue" When the issue template picker renders Then `bug_report` and `feature_request` templates are offered.
4. Given a visitor opens a PR When the PR editor loads Then `.github/PULL_REQUEST_TEMPLATE.md` content pre-populates the body.

### Scenario 4 - Contributor hits the secret-hygiene gate (P1)

Before the public flip, Gitleaks scans the full git history. Any committed API key (LLM provider keys, tokens) blocks the public flip and forces rotation. After the flip, `.env.example` documents every env var a contributor needs, and `.gitignore` prevents any future `.env*`/`.pem`/`.key` commit.

**Acceptance**

1. Given the repo history When the Gitleaks CI job runs (full-history scan) Then it reports either zero secrets or, if any are found, the repo remains private until they are rotated and the offending history is scrubbed.
2. Given the repo is about to flip public When Gitleaks has not passed on `main` Then the flip does not proceed.
3. Given a contributor copies `.env.example` to `.env` When they commit Then `.gitignore` blocks the `.env` file from being committed.

## Functional Requirements

- **FR-001**: A single GitHub Actions workflow at `.github/workflows/ci.yml` replaces the existing `.github/workflows/e2e.yml` (clean cutover — the old file is deleted, no orphan workflow). It defines five jobs: `lint-format`, `rust-tests`, `e2e-tests`, `create-release` (needs the three test jobs, gated to `main`), and `build-binaries` (needs `create-release`, gated on `new_release_published == 'true'`).
- **FR-002**: The `lint-format` job runs `cargo fmt --all -- --check`, `cargo clippy -- -D warnings`, and `cargo deny check` (advisories + licenses + bans).
- **FR-003**: The `rust-tests` job runs `cargo test` then `cargo build --release`.
- **FR-004**: The `e2e-tests` job reproduces the existing `e2e.yml` behavior verbatim (installs Playwright + deps, builds the release binary, runs `just e2e`, uploads report artifacts) so the current suite keeps passing under the new workflow. The binary listens on `127.0.0.1:11341` and the health/root path returns HTTP 200.
- **FR-005**: A `.releaserc` file at repo root mirrors strandgut's: `branches: ["main"]`, `tagFormat: "v${version}"`, plugins `@semantic-release/commit-analyzer` (with a `releaseRules` entry mapping `chore → patch`), `@semantic-release/release-notes-generator`, `@semantic-release/changelog`, `@semantic-release/git`, `@semantic-release/github`.
- **FR-006**: The `create-release` job installs `semantic-release` and the five `@semantic-release/*` plugins (plus `conventional-changelog-conventionalcommits`) globally via npm, then runs `npx semantic-release` using a `RELEASE_TOKEN` secret (a PAT with `contents:write` and `repo` scopes, because the default `GITHUB_TOKEN` cannot push back to `main`). The job exposes `new_release_published` and `new_release_version` outputs by comparing the latest `v*` tag before and after the release.
- **FR-007**: The `build-binaries` job uses a matrix of two targets — `x86_64-unknown-linux-musl` and `aarch64-unknown-linux-musl` — each built inside the matching `ghcr.io/rust-cross/rust-musl-cross:{x86_64-musl,aarch64-musl}` container. Each produced binary is verified as statically linked (`file` shows `statically linked`/`static-pie`, and `ldd` reports `not a dynamic executable`). Binaries are uploaded to the release via `softprops/action-gh-release@v3` with the `GITHUB_TOKEN`.
- **FR-008**: The cross-compile job produces `web/build/` before running `cargo build` so `build.rs` (which executes `npm install && npm run build` in `web/`) can complete inside the musl container. The chosen mechanism (install Node 26 in-container, or pre-build `web/build` and let `rust-embed` embed it) is resolved in the plan, but the constraint is non-negotiable: the embedded SPA must be present in every static binary.
- **FR-009**: A `renovate.json` at repo root mirrors strandgut's: `minimumReleaseAge: "7 days"` and a `packageRules` entry matching update types `["major","minor","patch","pin","digest"]` with `automerge: true` and `automergeType: "branch"`. Renovate scope covers Cargo, npm, and GitHub Actions.
- **FR-010**: Repository meta-files are added, matching strandgut's set: `LICENSE` (MIT), `CHANGELOG.md` (seeded empty; `@semantic-release/changelog` appends to it on release), `CONTRIBUTING.md`, `SECURITY.md`, `.github/PULL_REQUEST_TEMPLATE.md`, `.github/ISSUE_TEMPLATE/bug_report.yml`, `.github/ISSUE_TEMPLATE/feature_request.yml`.
- **FR-011**: A branded `README.md` mirrors strandgut's structure: centered `<img src=".github/readme/banner.svg" width="600">`, a badge row with four badges (CI/CD workflow badge, MIT license badge, `arch x86_64 | arm64` badge, `renovate-enabled` badge), then sections `## Features`, `## Quick start` (both `cargo run` and a `curl -L -o mealme <release-url>` one-liner per architecture), `## API` (the existing API reference table, preserved verbatim), `## LLM Recipe Import` (the existing section, preserved verbatim), `## Development`, `## Contributing`, `## Security`, `## License`.
- **FR-012**: A tasteful logo (`.github/readme/logo.svg`) and banner (`.github/readme/banner.svg`) are generated using the `design-taste-frontend` skill. Direction: minimal, fork-and-knife motif, clean line work, no AI slop. The banner is the asset referenced by the README hero `<img>`.
- **FR-013**: Dependency hygiene: `cargo-machete` is run to drop unused crates and feature flags; every crate is bumped to the latest stable version; `Cargo.lock` is committed; `cargo deny` runs in CI (advisories, licenses, bans).
- **FR-014**: Secret hygiene: `.gitignore` gains `.env*`, `*.pem`, and `*.key` patterns; a committed `.env.example` documents every LLM provider env var the backend reads at runtime (`OPENAI_API_KEY`, `ANTHROPIC_API_KEY`, `GEMINI_API_KEY`, `GROQ_API_KEY`, `DEEPSEEK_API_KEY`, `XAI_API_KEY`, `COHERE_API_KEY`, `TOGETHER_API_KEY`, `FIREWORKS_API_KEY`, `NEBIUS_API_KEY`, `MIMO_API_KEY`, `MINIMAX_API_KEY`, `ZAI_API_KEY`, `BIGMODEL_API_KEY`, `ALIYUN_API_KEY`, `BAIDU_API_KEY`, `MOONSHOT_API_KEY`, `AIHUBMIX_API_KEY`, `OPEN_ROUTER_API_KEY`, `OLLAMA_API_KEY` — sourced from `src/llm_import.rs:130-151`); a Gitleaks GitHub Action runs in CI on every push and PR with a full-history scan gate before the public flip.
- **FR-015**: The repository's visibility is flipped from private to public as the final step, only after Gitleaks passes on `main` (full history) and any discovered secret has been rotated and its history scrubbed.

## Key Entities

- **Static release binary**: a single musl-statically-linked Linux binary named `mealme-<target>` (e.g. `mealme-x86_64-unknown-linux-musl`), produced by `cargo build --release --target <target>` inside the matching `rust-cross` container, with the SPA embedded via `rust-embed`. Attached as a GitHub Release asset on every `vX.Y.Z` tag.
- **Semantic release**: a `vX.Y.Z` git tag created by semantic-release on `main` from Conventional Commits history, accompanied by an auto-generated GitHub Release (notes from `@semantic-release/release-notes-generator`), a `CHANGELOG.md` append (from `@semantic-release/changelog`), and a `Cargo.toml` version bump committed back to `main` (from `@semantic-release/git`).

## Edge Cases

- **build.rs in the musl container**: strandgut has no build-time Node dependency; MealMe does (`build.rs` runs `npm install && npm run build`). The cross-compile container must either install Node 26 before `cargo build` (the musl cross images do not ship Node) or the pipeline must pre-build `web/build/` on the host and pass it into the container build so `rust-embed` embeds it. The plan must commit to one mechanism and prove it with a green cross-compile.
- **Renovate automerge bypassing the matrix**: `automergeType: "branch"` opens a PR-style branch that CI runs against; automerge fires only on green. Renovate must never merge a dep bump its architecture matrix hasn't validated.
- **`RELEASE_TOKEN` scope**: the default `GITHUB_TOKEN` cannot push the `Cargo.toml`+`CHANGELOG.md` commit back to `main`; a PAT named `RELEASE_TOKEN` with `contents:write` and `repo` scopes is required, mirroring strandgut. If a new PAT cannot be created by the implementer, semantic-release's `@semantic-release/git` step is blocked — this is a prerequisite the user must provision.
- **Gitleaks full-history scan**: a committed-then-deleted key is the failure mode. The pre-flip scan must cover full history, not just the latest diff. If such a key is found, the repo stays private, the key is rotated at the provider, and the history is scrubbed (`git filter-repo` or BFG) before re-scan and flip.
- **cargo-deny license conflicts**: the current crate set is MIT/Apache-2.0 compatible. If `cargo deny check licenses` fails on any crate, the spec does NOT auto-ban or auto-replace the crate — it escalates to the user to decide between (a) accept the license, (b) replace the crate, or (c) drop the feature.
- **e2e relocation**: the existing `.github/workflows/e2e.yml` is deleted in favor of the `e2e-tests` job in `ci.yml`. The Playwright invocation, port (`11341`), artifacts (`playwright-report`, `junit-results`), and the `just e2e` entrypoint must remain identical so the suite stays green.
- **`chore` commits releasing**: strandgut's `.releaserc` maps `chore → patch`, so every `chore:` commit on `main` produces a patch release. This is intentional (keeps the changelog honest about dependency chores) and is not a bug to "fix."
- **arm64 host availability**: GitHub-hosted `ubuntu-latest` runners are x86_64; the arm64 binary is cross-compiled (not natively built) inside the `rust-cross` container, so no `ubuntu-24.04-arm` runner is required for the binary job. (Strandgut uses a native arm runner only for its container build, which is out of scope here.)

## Research Notes

- `RouHim/strandgut` `.github/workflows/ci.yml` — proven reference for the five-job CI structure (lint-format / rust-tests / e2e-tests / create-release / build-binaries), the `rust-cross/rust-musl-cross` containers for x86_64 and aarch64 musl builds, `softprops/action-gh-release@v3` for asset upload, and the before/after-tag-diff technique for detecting a new release. <https://github.com/RouHim/strandgut>
- `RouHim/strandgut` `.releaserc` — minimal proven semantic-release config (`branches: ["main"]`, `tagFormat: "v${version}"`, the five plugins, `chore → patch` release rule). <https://github.com/RouHim/strandgut/blob/main/.releaserc>
- `RouHim/strandgut` `renovate.json` — minimal proven Renovate config (`minimumReleaseAge: "7 days"`, automerge branch on all update types). <https://github.com/RouHim/strandgut/blob/main/renovate.json>
- `semantic-release-cargo` (crates.io v2.4.84, 2025-12-06) — a purpose-built CLI for bumping `Cargo.toml`'s version within a semantic-release workflow; an alternative to letting `@semantic-release/git` only touch `CHANGELOG.md` and relying on the tag for the version. Strandgut does NOT use it (it relies on changelog+git+github plugins and lets the tag carry the version). Whether to adopt `semantic-release-cargo` for an explicit `Cargo.toml` bump is deferred to the plan. <https://crates.io/crates/semantic-release-cargo>
- Renovate configuration docs — `minimumReleaseAge` + `automergeType: "branch"` is the documented stable pattern for delayed, CI-gated auto-merging. <https://docs.renovatebot.com/configuration-options/>

## Assumptions

- The repository is currently private; the public flip is the final gate, applied only after Gitleaks passes on `main` (full history) and any discovered secret is rotated.
- The MIT license (matching strandgut) is acceptable for MealMe; the user has not specified otherwise.
- musl-static Linux binaries for `x86_64` and `aarch64` are the sufficient target set for this DoD; macOS and Windows binaries are out of scope and may be added later.
- The logo and banner SVGs are generated via the `design-taste-frontend` skill (tasteful, minimal, fork-and-knife motif, no AI slop) and committed under `.github/readme/`, mirroring strandgut's `banner.svg` location. They are not vendored from third-party asset libraries.
- The container-image build (strandgut's `build-container` job and its GHCR push) is out of scope — MealMe's deploy story is the single static binary, not a container image.
- The default `GITHUB_TOKEN` is sufficient for uploading binaries to the GitHub Release; a PAT named `RELEASE_TOKEN` (with `contents:write` + `repo` scopes) is required for semantic-release's push-back of `Cargo.toml`+`CHANGELOG.md` to `main`, matching strandgut. Provisioning this PAT is a user prerequisite.
- The `just` command-runner (used by the existing `e2e.yml` for `just e2e`) continues to be the E2E entrypoint in the new `ci.yml` `e2e-tests` job.

## Success Criteria

- **SC-001**: Pushing a `feat:` commit to `main` produces, within the same CI run, a new `vX.Y.Z` tag, a `Cargo.toml` version equal to `X.Y.Z`, a new `CHANGELOG.md` section, a GitHub Release, and two attached statically-linked binaries named `mealme-x86_64-unknown-linux-musl` and `mealme-aarch64-unknown-linux-musl`.
- **SC-002**: Pushing any commit to a non-`main` branch runs lint-format, rust-tests, and e2e-tests but creates no release and builds no binaries.
- **SC-003**: A Renovate PR for a Cargo, npm, or GitHub Actions dependency is auto-merged if and only if the full CI matrix is green; a failing matrix blocks the merge.
- **SC-004**: `cargo machete` reports zero unused dependencies/features after the audit; every crate in `Cargo.toml` is at the latest stable version; `Cargo.lock` is committed.
- **SC-005**: `cargo deny check` passes on advisories, licenses, and bans in CI (no unpatched advisory, no banned crate, no incompatible license).
- **SC-006**: The public README renders the centered banner and the four badges; the quickstart `curl` command downloads a working binary for the visitor's architecture.
- **SC-007**: `.github/ISSUE_TEMPLATE/bug_report.yml`, `.github/ISSUE_TEMPLATE/feature_request.yml`, and `.github/PULL_REQUEST_TEMPLATE.md` exist and are selectable when creating issues/PRs.
- **SC-008**: `.gitignore` contains `.env*`, `*.pem`, `*.key`; `.env.example` exists and lists every runtime LLM-provider env var the backend reads; committing a `.env` file is blocked.
- **SC-009**: A Gitleaks full-history scan on `main` reports zero secrets before the repo is flipped to public; the flip is blocked until this passes.
- **SC-010**: The repository's visibility is `public` and the old `.github/workflows/e2e.yml` no longer exists (replaced by `ci.yml`).
