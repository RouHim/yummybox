# Feature Specification: i18n Support — English & German

**Created**: 2026-06-13
**Status**: Approved
**Input**: i18n en,de

## Goal
Make every user-facing string in the Meal Manager frontend display in either English (en) or German (de). The user's language is auto-detected from the browser on first visit and can be overridden via a manual toggle whose choice persists across sessions. German locale formatting for numbers and dates is included. Zero new npm dependencies; translations are stored as typed TypeScript objects. Backend error messages remain in English.

## User Scenarios

### Scenario 1 — First-visit language auto-detection (P1)
A user whose browser is set to German opens the app for the first time. The entire UI — headings, labels, placeholders, buttons, validation messages, and empty/no-results states — renders in German without any manual action. A user whose browser is set to English (or any non-German language) sees the UI in English.

**Acceptance**
1. Given the browser's primary language is `de` (or `de-DE`, `de-AT`, `de-CH`) and no prior language choice is stored, when the app loads, then all user-facing strings are displayed in German.
2. Given the browser's primary language is `en` (or `fr`, `es`, or any language other than German) and no prior language choice is stored, when the app loads, then all user-facing strings are displayed in English.
3. Given the browser's primary language is `de-CH` (a German regional variant), when the app loads, then the UI is displayed in German.

### Scenario 2 — Manual language toggle (P1)
A user wants to switch the UI language regardless of their browser setting. They click a language toggle control in the page header. The UI immediately re-renders all strings in the selected language. The choice persists across page reloads and browser restarts.

**Acceptance**
1. Given the UI is displayed in English, when the user activates the language toggle to switch to German, then all user-facing strings immediately change to German without a full-page reload.
2. Given the UI is displayed in German, when the user activates the language toggle to switch to English, then all user-facing strings immediately change to English.
3. Given the user has manually selected German, when they close and reopen the app, then the UI is displayed in German (the manual choice overrides auto-detection).
4. Given the user has manually selected a language, when they clear their browser's localStorage, then on next load the app reverts to auto-detection.

### Scenario 3 — Missing translation fallback (P2)
A developer adds a new English string to the translation dictionary but has not yet added the German translation. When the UI is in German mode, that string falls back to the English value rather than displaying a raw translation key, a blank space, or throwing an error.

**Acceptance**
1. Given the UI is in German mode and a translation key exists in the English dictionary but is missing from the German dictionary, when that key is rendered, then the English string is displayed.
2. Given the UI is in German mode and a translation key is missing from both dictionaries, when that key is referenced, then the raw key string is displayed (visible to developers) rather than throwing an unhandled exception.

### Scenario 4 — Locale-aware number and date formatting (P2)
When the UI is in German mode, any displayed numbers or dates follow German locale conventions. When in English mode, they follow English conventions.

**Acceptance**
1. Given the UI is in German mode, when a number is formatted for display (e.g., ingredient count or meal count), then it uses German grouping/decimal conventions (`.` as thousands separator, `,` as decimal separator — e.g., `1.234,56`).
2. Given the UI is in German mode, when a date is formatted for display, then it uses the German `DD.MM.YYYY` format.
3. Given the UI is switched from German to English, when numbers or dates are displayed, then they adapt to English conventions immediately.

## Functional Requirements

- **FR-001**: The system MUST define two supported locales: `en` (English) and `de` (German).
- **FR-002**: All user-facing strings in the frontend MUST be stored in typed translation dictionaries — one per locale — as plain TypeScript objects in `$lib/i18n/`. Zero new npm dependencies.
- **FR-003**: Every user-facing string currently hardcoded in the frontend MUST be replaced with a translation lookup. This includes at minimum: page headings, button labels, form field labels and placeholders, section headings, empty-state and no-results messages, card action labels, client-side validation error messages, API error fallback messages, the delete confirmation prompt, and the search aria-label.
- **FR-004**: On first visit with no stored language preference, the system MUST auto-detect the user's language from `navigator.language` (or equivalent browser API). Browser language tags starting with `de` (e.g., `de`, `de-DE`, `de-AT`, `de-CH`) MUST resolve to `de`. All other language tags MUST resolve to `en`.
- **FR-005**: The system MUST provide a visible language toggle control in the UI that allows the user to switch between English and German.
- **FR-006**: The user's language choice (manual or auto-detected) MUST be persisted to `localStorage` and restored on subsequent visits. A manually chosen language MUST take precedence over auto-detection.
- **FR-007**: When a translation key is missing from the active locale's dictionary, the system MUST fall back to the English dictionary value for that key.
- **FR-008**: When a translation key is missing from both dictionaries, the system MUST display the raw key string rather than throwing an unhandled exception.
- **FR-009**: The system MUST use the browser's `Intl.NumberFormat` and `Intl.DateTimeFormat` APIs (or equivalent) to format numbers and dates according to the active locale's conventions.
- **FR-010**: The language toggle control MUST have an accessible name and role so that assistive technology can identify it and its current state.
- **FR-011**: Changing the active language MUST re-render all visible translated strings immediately without a full-page navigation or reload.
- **FR-012**: The static pre-rendered HTML (from the SvelteKit static adapter) MUST default to English strings. Client-side hydration MUST then replace strings with the detected or stored language if different.
- **FR-013**: The backend (`src/`) MUST NOT be modified. Backend error messages returned via the API remain in English and are not translated on the server.
- **FR-014**: All existing E2E tests MUST pass after i18n changes, with any necessary selector or assertion updates to accommodate translated content.

## Key Entities

- **Translation Dictionary**: A typed TypeScript object mapping string keys to their English or German string values. The English dictionary is the source of truth for available keys. Dictionaries support basic interpolation (e.g., `"Hello {name}"`).
- **Locale**: One of `"en"` or `"de"`, representing the active display language. Determined by auto-detection on first visit, then stored and potentially overridden by manual user choice.

## Edge Cases

- **Corrupted or unavailable localStorage**: If `localStorage` is unavailable or its stored value is corrupted, the system falls back to auto-detection from `navigator.language` without error.
- **Browser language is a German regional variant**: `de-AT`, `de-CH`, `de-LI`, `de-LU`, and any future `de-*` tag match to `de`.
- **Browser language is a non-German, non-English language**: Any language other than those starting with `de` defaults to English — no partial translation or mixed-language UI.
- **Interpolated values containing special characters**: Template strings with interpolated values (e.g., a meal name containing `<`, `>`, or `&`) must not break the translation substitution or introduce XSS vectors.
- **Rapid language toggling**: Quickly switching between languages multiple times must not produce mixed-language UI state, flickering, or stale strings from a previous locale.
- **Static pre-render flash**: The static HTML pre-renders in English. On hydration, if the detected language is German, the switch must happen before the user can interact with the UI, or produce at most a single-frame flash.
- **Translation key naming collisions**: If a raw translation key string coincidentally matches user-entered data (e.g., a meal named "Save"), the translation lookup must not incorrectly substitute it.

## Research Notes

- **Poly I18n** (poly-i18n.vercel.app) — Demonstrates a <100-line DIY i18n pattern using plain typed dictionaries with type-safe keys, fallback chaining, and zero dependencies. The pattern is framework-agnostic and adapts cleanly to Svelte 5 `$state` runes. Our ~29 strings make this approach preferable to npm packages like `svelte-i18n` or Paraglide.
- **Svelte 5 runes** (svelte.dev/docs/svelte/$state) — `$state` provides reactive primitives that replace the Svelte 4 store pattern. A reactive locale `$state` with `$derived` translation lookups avoids the SSR shared-state race conditions that affect `svelte-i18n` and `sveltekit-i18n`.
- **Intl API** (MDN: Intl.NumberFormat, Intl.DateTimeFormat) — Built into all modern browsers. `new Intl.NumberFormat('de-DE').format(1234.56)` produces `"1.234,56"`. `new Intl.DateTimeFormat('de-DE').format(date)` produces `"13.6.2026"`. Zero dependencies, runtime-configurable.
- **`navigator.language`** (MDN) — Returns the browser's primary language tag (e.g., `"de"`, `"en-US"`). Available in all modern browsers. Suitable for first-visit auto-detection in a client-rendered SPA.

## Assumptions

- English is the source/fallback language; all translation keys are defined first in the English dictionary.
- The language toggle is a simple control (button, toggle, or select) placed in the page header — not a separate settings page or modal.
- No right-to-left (RTL) language support is needed (only `en` and `de`, both LTR).
- Translation keys follow a naming convention derived from English strings (e.g., `"addMeal"`, `"emptyStateTitle"`). No auto-extraction or key-generation tooling is introduced.
- The SvelteKit static adapter pre-renders the single page in English; the client hydrates and immediately applies the detected/stored language.
- Pluralization (e.g., "1 meal" vs "2 meals") is out of scope unless a string in the current UI already contains a dynamic count — currently no such strings exist.
- The backend (`src/`) receives no i18n changes. Rust error messages, API response bodies, and database content remain in English.
- Existing E2E tests may need selector updates to match translated content; test logic and assertions remain intact.
- The app remains a single-page application served from one route. No locale-specific URL paths (e.g., `/de/`, `/en/`) are introduced.

## Success Criteria

- **SC-001**: Opening the app with a browser set to German (`de`) and no stored language preference displays all UI strings in German on first paint after hydration.
- **SC-002**: Opening the app with a browser set to French (`fr`) and no stored language preference displays all UI strings in English.
- **SC-003**: Activating the language toggle switches every visible user-facing string to the selected language within 100ms of the toggle action, with no full-page reload.
- **SC-004**: Selecting German via the toggle, closing the tab, and reopening the app displays the UI in German (persisted choice).
- **SC-005**: Clearing localStorage and reloading the app reverts to auto-detection.
- **SC-006**: Defining a key in the English dictionary but not in the German dictionary displays the English value when the UI is in German mode — no blank space, raw key, or error.
- **SC-007**: With German active, `new Intl.NumberFormat('de-DE').format(1234.56)` produces `"1.234,56"` when rendered in the UI.
- **SC-008**: With German active, date formatting produces the `DD.MM.YYYY` pattern when rendered in the UI.
- **SC-009**: All existing E2E tests pass — zero functional regressions from the i18n changes.
- **SC-010**: `web/package.json` contains no new entries in `dependencies` or `devDependencies` beyond those present before i18n work began.
