---
name: new-feature-needs-new-e2e
description: "New user-facing features MUST include at least one new E2E test covering the happy path"
condition: "(all (\\d+ )?tests pass(ing)?)|(All \\d+ (Rust|frontend|E2E).*pass)"
scope: "text"
---

When you add a new user-facing feature (not just an internal refactor), you MUST add at least one new E2E test covering its happy path before declaring the test suite complete. Existing tests passing is necessary but not sufficient — they don't exercise the new behavior. The E2E smoke-test scenarios listed in the plan are not optional; they are part of the deliverable.

A CSS/layout redesign that preserves all existing interaction paths, selectors, data flow, and API calls is a refactor, not a 'new user-facing feature'. This rule gates on functional behavior changes — new API endpoints, new form submissions, new navigation paths, new data mutations. Visual polish reusing the same click targets and API calls does not warrant new E2E tests. If the existing E2E suite still passes with the same selectors, coverage is sufficient.