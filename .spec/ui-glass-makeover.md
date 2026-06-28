# Feature Specification: Liquid Glass UI Makeover

**Created**: 2026-06-22
**Status**: Approved
**Input**: Give MealMe's UI a makeover applying contemporary design trends — liquid glass, micro-animations — inspired by Apple's 2025 Liquid Glass, adapted to the meal-manager app, without AI slop.

## Goal
Give the MealMe frontend a visual and interaction makeover that adopts Apple's Liquid Glass design language (translucent, content-adaptive surfaces; specular highlights; fluid morphs) together with a bold refreshed food-themed palette and an ambient background worth refracting — light appearance only, no dark mode — so the app feels contemporary and delightful while remaining accessible, performant, and free of AI slop (no glass-on-everything, no generic gradient soup). Scope is purely CSS + Svelte + the existing `motion.ts`: no API, schema, route, or behavioral changes.

## User Scenarios

### Scenario 1 - Browsing meals (P1)
A visitor browses the meal list and immediately perceives a contemporary, lively surface: a sticky header and search bar float in translucent glass above a subtle warm ambient background, while the meal cards themselves stay opaque and content-sharp. Adding a new meal slides the card in with a 200–300ms ease-out entrance and reflows the list with a layout morph; deleting a meal reverses this with an exit animation.

**Acceptance**
1. Given the meals page is open When the page renders Then the sticky header, nav, and search bar render as translucent glass surfaces that refract a subtle warm ambient background behind them, and the meal cards render as opaque surfaces with sharp text.
2. Given a meal card exists When the user adds a new meal Then the new card animates in (fade + slide, 200–300ms, ease-out) and surrounding cards reflow with a layout morph, and the final list order matches the API response.
3. Given a meal card exists When the user deletes it Then the card animates out (fade + slide, 200–300ms, ease-in) and remaining cards reflow with a layout morph, and the deleted meal is absent from subsequent loads.
4. Given the meal list is long When the user scrolls Then the page remains smooth (no jank); no per-card `backdrop-filter` is applied (cards stay opaque by design).

### Scenario 2 - Opening an overlay (P1)
A user opens the meal picker, the image lightbox, or a delete confirmation. The overlay appears as a glass layer refracting content behind it, opens with a fluid entrance, and closes with a fluid exit. On reduced-motion or low-end devices the overlay still appears instantly and is fully usable — only the animation is removed.

**Acceptance**
1. Given the user triggers a modal (meal picker, lightbox, or delete-confirm) When it opens Then the modal renders as a translucent glass surface with a subtle entrance animation (200–300ms, ease-out).
2. Given a modal is open When the user dismisses it (Esc, backdrop click, or action button) Then the modal animates out (200–300ms, ease-in) before being removed from the DOM.
3. Given the user's OS has `prefers-reduced-motion: reduce` set When any overlay opens or closes Then the overlay appears/disappears instantly with no transition, and all interactive content remains reachable and correctly focused.

### Scenario 3 - Navigating the planner (P2)
A planner visitor navigates weeks. The year-nav controls and plan-detail panel use floating-glass controls, and selecting/deselecting a week morphs the week-grid button between states fluidly.

**Acceptance**
1. Given the planner is open When the year-nav and plan-detail render Then they render as translucent glass control layers floating above the warm ambient background.
2. Given a week button is unselected When the user selects it Then the button morphs to its selected state with a 200–300ms transition, and selecting a different week reverses the morph on the first button.
3. Given `prefers-reduced-motion: reduce` When the user selects a week button Then the state change is instant, with no motion.

### Scenario 4 - Low-power / constrained device (P2)
A user on a coarse-pointer device or a device flagged as low-power opens the app. Glass blur is reduced or substituted with solid translucency, and all animation timings are shortened or removed, but content and reachability are identical to the default path.

**Acceptance**
1. Given the device reports a coarse pointer or limited memory (via `prefers-reduced-motion`, `(any-pointer: coarse)`, or a `navigator.deviceMemory` guard) When the app renders glass surfaces Then blur strength is reduced or replaced with solid translucency and no decorative animation runs.
2. Given the degraded path is active When the user performs any core task (search, add, edit, delete, plan) Then every task succeeds and renders identically to the default path except for the absence or reduction of motion/blur.

## Functional Requirements

- **FR-001**: Apply translucent "Liquid Glass" surfaces to floating/control layers only — sticky header + nav, search bar, modals (meal picker, image lightbox, delete-confirm), plan-detail, and the meal-detail page wrapper — consistent with Apple's principle that glass is a functional layer above content, not decoration.
- **FR-002**: Keep meal-list cards opaque and content-sharp; no `backdrop-filter` applied per card in the list.
- **FR-003**: Introduce a refreshed food-themed palette extending the existing `:root` design tokens (warm cream → amber → primary orange), including new tokens for glass surfaces (translucency, border, scrim, specular highlight) and an ambient background layer.
- **FR-004**: Add a subtle food-themed ambient background (soft warm gradient, optionally with faint ambient orbs) that the glass surfaces refract; keep it subtle — not a maximalist multi-color gradient soup.
- **FR-005**: Implement a micro-animation vocabulary covering: (a) state feedback — press/hover/focus scale, error shake; (b) entrance/exit — cards, list items, modal open/close, plan-detail expand — 200–300ms, ease-out for entrances / ease-in for exits; (c) layout morphs — week-grid button press, form-mode/tab switches, card reflow after add/delete (e.g., FLIP-style).
- **FR-006**: Use CSS transitions/animations as the primary mechanism; extend the existing `web/src/lib/motion.ts` only for JS-orchestrated animation (staggered entrances, FLIP reflow). Do not introduce any animation runtime library.
- **FR-007**: Honor `@media (prefers-reduced-motion: reduce)` (already present in `app.css`): all decorative animation removed; enter/exit instant; layout, content, and reachability unchanged.
- **FR-008**: Detect constrained devices (`(any-pointer: coarse)` and/or `navigator.deviceMemory` ≤ 4) to reduce `backdrop-filter` blur strength or substitute solid translucency, and to shorten or remove decorative motion. This degradation is additive to, not a replacement for, the reduced-motion path.
- **FR-009**: Provide an `@supports` fallback where `backdrop-filter` is unsupported: glass surfaces render as solid translucent surfaces with the same border/scrim tokens, preserving contrast and hierarchy.
- **FR-010**: Maintain WCAG AA text contrast (≥ 4.5:1 for body text, ≥ 3:1 for large text) on all text rendered over glass surfaces, using a scrim/backdrop behind text where blur alone is insufficient.
- **FR-011**: Provide visible, persistent focus rings on all interactive glass controls; focus states must not rely on motion alone.
- **FR-012**: Preserve all existing functional behavior: routes (`/`, `/meals`, `/meals/:id`, `/planner`), API contracts, data schema, i18n keys, validation rules, server-driven search, and planner logic remain unchanged.
- **FR-013**: Update existing frontend tests only where they assert on markup or CSS classes affected by the makeover; no behavior-changing test additions are required by this spec.
- **FR-014**: Avoid AI slop: glass must be a deliberate layer above content (not applied to opaque content cards or used as decoration), ambient background must remain food-themed and subtle, and animations must be purposeful (state changes, navigation, focus) rather than decorative-for-decoration's-sake.

## Key Entities

- **Glass surface**: a translucent UI layer (header/nav, search, modal, plan-detail, detail wrapper) rendered with `backdrop-filter` blur + semi-translucent background + subtle border + specular-highlight highlight; falls back to solid translucency on unsupported/degraded paths.
- **Ambient background**: a subtle, food-themed background layer (soft warm gradient, optional faint orbs) that the glass surfaces refract; sits behind all content.
- **Motion token**: a named entrance/exit/morph animation with a documented duration (200–300ms default) and easing (ease-out entrances, ease-in exits), coordinated via `web/src/lib/motion.ts` for JS-orchestrated cases; CSS-first otherwise.
- **Degradation tier**: a runtime classification (default / reduced-motion / low-power) selecting animation timing and glass blur strength while leaving content and reachability identical.

## Edge Cases

- `backdrop-filter` not supported or disabled by the browser: glass surfaces fall back to solid translucent surfaces via `@supports` (FR-009); content and contrast preserved.
- Long meal lists (many cards): no per-card `backdrop-filter` (cards opaque by design, FR-002) — no scroll jank and no GPU pressure amplification.
- Text rendered over glass on a low-contrast ambient patch: scrim/backdrop behind text enforces WCAG AA (FR-010), so readability never depends on blur alone.
- Reduced-motion path: layout, content, and reachability are unchanged — only animation timing is set to ~0 (existing `@media (prefers-reduced-motion: reduce)` rule in `app.css` continues to apply).
- Mid-transaction request cancellation / aborted fetch (the request-cancelled-during-DB-query case already observed in the app): any loading or overlay state must surface the aborted or error state explicitly rather than masking it behind an entrance/exit animation.
- Planner week-grid rapid selection: consecutive selections must morph cleanly without animation overlap or visual lag; selecting the already-selected week is a no-op visually.
- Custom `empty-meals` icon and Feather-style icons used across the app: icon stroke weights and visual rhythm must remain consistent with the existing Feather-inspired palette; the makeover must not regress the icon-set visual cohesion.

## Research Notes

- https://www.apple.com/newsroom/2025/06/apple-introduces-a-delightful-and-elegant-new-software-design/ — Apple's Liquid Glass is a translucent material that reflects/refracts surroundings, transforms dynamically to bring focus to content, and is applied as a functional layer above content (controls, toolbars, nav, modals), not as decoration. The spec's "glass on floating layers only, opaque content cards" split follows this principle and yields both fidelity and a performance win.
- https://playground.halfaccessible.com/blog/glassmorphism-design-trend-implementation-guide — backing `backdrop-filter` has ~95% browser support but is GPU-intensive; always include the `-webkit-` prefix, provide `@supports` fallbacks, maintain WCAG contrast via scrims behind text, and apply glass sparingly. The spec encodes these as explicit FRs (reduced blur on low-end, `@supports` fallback, scrims for contrast, glass on control layers only).
- https://netcodesign.com/micro-interactions-that-feel-magical-best-practices-code-snippets/ and https://acodez.in/micro-interactions-motion-design/ — micro-animations live in the 200–400ms range; ease-out for exits and ease-in for entrances; `prefers-reduced-motion` is standard; progressive enhancement (reducing or simplifying animations) on lower-powered devices is recommended. The spec's motion token covers 200–300ms defaults and the dedicated low-power degradation tier (FR-008) follows this guidance.

## Assumptions

- The existing multi-route SvelteKit structure (`/`, `/meals`, `/meals/:id`, `/planner`, each a single `+page.svelte`) is preserved; no new routes are added.
- The existing design tokens in `web/src/app.css` are extended/refactored rather than discarded.
- No new runtime dependencies are introduced; CSS plus the existing `web/src/lib/motion.ts` are the only mechanisms.
- Light appearance only — a dark-mode token set, persistence, and toggle are explicitly out of scope for this spec.
- No backend changes (API, schema, validation, search, planner logic) are in scope; the makeover touches CSS, Svelte markup, and `motion.ts` only.
- Existing frontend tests are updated only where they assert on markup or classes affected by the makeover; no new web-search-second-opinion step is required for the implementation since research is captured above.
- The warm food-themed palette (#fff8f0 / #c2410c family) is evolved rather than replaced — the new tokens stay on-theme.

## Success Criteria

- **SC-001**: On `/meals`, `/meals/:id`, and `/planner`, all sticky header/nav, search bar, modal, plan-detail, and detail-wrapper surfaces render as translucent glass that visibly refracts a subtle warm ambient background, while meal-list cards remain opaque, in a Chromium browser supporting `backdrop-filter`.
- **SC-002**: Adding and deleting a meal triggers an entrance/exit animation (200–300ms, ease-out/ease-in) plus a layout morph of the remaining cards, and the final DOM order matches the API response.
- **SC-003**: Opening and closing the meal picker, lightbox, and delete-confirm modal animates with a 200–300ms entrance (ease-out) and exit (ease-in), and all content remains keyboard-reachable with visible focus rings throughout.
- **SC-004**: With `prefers-reduced-motion: reduce` active, all decorative animation is absent, enter/exit is instant, and layout, content, and reachability are identical to the default path — verified by manual toggle of the OS reduced-motion setting.
- **SC-005**: On a coarse-pointer device or `navigator.deviceMemory` ≤ 4 device, glass blur is reduced or substituted with solid translucency and decorative motion is shortened or removed, while every core task (search, add, edit, delete, plan) still succeeds and renders identically except for the missing motion/blur.
- **SC-006**: In a Chromium browser with `backdrop-filter` off/disabled, glass surfaces fall back to solid translucent surfaces via `@supports`, preserving contrast, hierarchy, and content.
- **SC-007**: All text rendered over glass surfaces meets WCAG AA contrast against its effective background (verified with a contrast checker tool), with scrims behind text where blur alone is insufficient.
- **SC-008**: `cd web && npm run check` passes (type-check), and `cd web && npm test` passes with no regressions beyond tests intentionally updated for affected markup/classes.
- **SC-009**: The makeover does not read as AI slop on inspection — glass is visibly a deliberate functional layer above content (not on opaque cards or used as decoration), the ambient background is subtle and food-themed (not a maximalist gradient soup), and animations are purposeful (state changes, navigation, focus) rather than decorative.
