---
name: run-tests-after-feature
description: "After implementing a feature, run the project's full test suite including E2E tests before yielding"
condition: "(test result:|cargo test|npm test|test:e2e|all.*pass|test suite|smoke test)"
scope: "text"
---

After implementing a non-trivial feature, you MUST run the full test suite before yielding — not just unit tests, but also E2E tests when they exist. The project's AGENTS.md lists the commands: `cargo test`, `cd web && npm test`, `cd web && npm run test:e2e`, `cd tests && npm test`. Run at minimum `cargo test` and at least one E2E suite. Never claim verification is complete without running the tests.