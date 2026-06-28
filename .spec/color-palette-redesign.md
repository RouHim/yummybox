# Feature Specification: Color Palette Redesign

**Created**: 2026-06-23
**Status**: Approved
**Input**: Redesign the entire color palette — current one is ugly — avoid AI-slop generic design

## Goal

Replace the current monochromatic warm palette (all cream/orange/brown) with a restrained, dual-theme color system inspired by Apple's Liquid Glass design philosophy. Translucent glass surfaces sit over a rich ambient background image; color is used sparingly for functional signals only. The result is a polished, distinctive look that feels intentional — not a generic warm-food-app template, and not AI-generated.

## User Scenarios

### Scenario 1 - Light mode browsing (P1)
A user opens the app during the day. The ambient background image shows through translucent glass surfaces. Color accents are restrained — the primary color appears only on interactive elements (buttons, links, focus rings). The background image carries the visual richness, not the UI chrome.

**Acceptance**
1. Given the user's system is set to light mode, when they load any page, then glass surfaces are light-translucent and the ambient background image is visible through them.
2. Given the light theme, when measuring any text against its background, then the contrast ratio is at least 4.5:1 for normal text and 3:1 for large text (WCAG AA).
3. Given the light theme, when the user scans the page, then color communicates function (primary action, danger, focus) — no decorative color-washing of surfaces.

### Scenario 2 - Dark mode cooking view (P1)
A user opens a meal detail page at night while cooking. The system is in dark mode. The ambient background image is subdued; glass surfaces are dark-translucent. Text is legible without eye strain; accent colors are desaturated to avoid optical vibration on dark backgrounds.

**Acceptance**
1. Given the user's system is set to dark mode, when they load any page, then glass surfaces are dark-translucent and the ambient background image is toned down.
2. Given the dark theme, when measuring any text against its background, then the contrast ratio meets WCAG AA (4.5:1 normal, 3:1 large).
3. Given the dark theme, when accent colors appear on interactive elements, then they are desaturated by approximately 20 points relative to their light-mode counterparts (no optical vibration).

### Scenario 3 - Semantic color recognition (P2)
A user encounters a danger action (delete), a validation error, or a success confirmation. Each state has a distinct, recognizable color that works in both themes.

**Acceptance**
1. Given a danger action button, when rendered in either theme, then it uses `--color-danger` tokens with sufficient contrast.
2. Given a validation error message, when rendered in either theme, then it uses `--color-error` tokens with a distinct background.
3. Given a success confirmation, when rendered in either theme, then it uses a dedicated success color token.

## Functional Requirements

- **FR-001**: Light + dark theme support via `prefers-color-scheme` media query. Every color token MUST have a value defined in both `:root` (light) and `@media (prefers-color-scheme: dark)` blocks.
- **FR-002**: All text/background pairs MUST meet WCAG AA contrast minimums (4.5:1 for normal text, 3:1 for large text ≥18px or bold ≥14px).
- **FR-003**: Ambient background MUST use a CSS `background-image` (image file, not CSS gradients) behind the `.app-ambient` layer. The image SHALL be theme-adaptive: a light-appropriate variant and a dark-appropriate variant. Fallback to a solid/gradient in the same tonal range when the image fails to load.
- **FR-004**: Glass surface tokens (`--glass-bg`, `--glass-bg-strong`, `--glass-border`, `--glass-border-inner`, `--glass-highlight`, `--glass-scrim`, `--glass-scrim-dark`) SHALL have per-theme values. Light theme: white-based translucent. Dark theme: dark-based translucent.
- **FR-005**: Accent palette SHALL be restrained: at most 4 accent hues total (primary, danger, success, plus one optional secondary/informational). Primary SHALL NOT be generic warm orange — it MUST include an unexpected, tasteful complement color.
- **FR-006**: The following currently-undefined CSS custom properties MUST be resolved with real values: `--color-danger`, `--color-danger-soft`, `--color-border-light`.
- **FR-007**: Total color token count SHALL NOT exceed 20. Each token SHALL have a documented role (see Key Entities).
- **FR-008**: Color SHALL be used for functional signals only (actions, errors, focus indicators, selection). Surface colors (backgrounds, cards, glass) SHALL be neutral or near-neutral — the ambient background image and meal content provide visual richness.
- **FR-009**: Existing CSS classes that reference color tokens (`.btn`, `.glass`, `.meal-card`, `.form-error`, `.search`, `.empty-state`, `.no-results`, `.app-ambient`, etc.) SHALL continue to work without markup changes. Only token values change.
- **FR-010**: `accent-color` and `::selection` SHALL inherit the primary color.
- **FR-011**: Existing degradation paths (`prefers-reduced-transparency`, `prefers-reduced-motion`, `prefers-contrast`, coarse-pointer classes, `.low-power`, `.no-glass-blur`) SHALL NOT be broken by the new token values.

## Key Entities

| Token | Role | Light Theme | Dark Theme |
|---|---|---|---|
| `--color-bg` | Page background (behind ambient layer) | Off-white/cream | Near-black/dark charcoal |
| `--color-surface` | Card/form background | White/near-white | Dark gray |
| `--color-surface-2` | Secondary surface (hover, alt rows) | Slightly darker than surface | Slightly lighter than surface |
| `--color-primary` | Primary actions, links, focus rings | Distinctive, saturated accent | Desaturated variant (~20pts less) |
| `--color-primary-hover` | Primary hover/press state | Darker primary | Lighter primary |
| `--color-primary-soft` | Primary focus ring background | Very light primary tint | Very dark primary tint |
| `--color-text` | Primary body text | Near-black | Near-white (not pure white) |
| `--color-text-secondary` | Secondary/description text | Medium gray | Medium-light gray |
| `--color-text-muted` | Placeholder, disabled, icons | Light gray | Dim gray |
| `--color-border` | Card/input borders | Light gray | Dark gray (lighter than surface) |
| `--color-border-strong` | Active/focus borders, dividers | Medium gray | Medium-dark gray |
| `--color-accent` | Optional secondary accent | Distinct complement to primary | Desaturated variant |
| `--color-error` | Error text, destructive actions | Red, AA contrast | Desaturated red |
| `--color-error-bg` | Error message background | Very light red | Very dark red |
| `--color-danger` | Danger/destructive button text | Red, AA contrast | Desaturated red |
| `--color-danger-soft` | Danger hover background | Very light red | Very dark red |
| `--color-success` | Success confirmations | Green, AA contrast | Desaturated green |
| `--color-success-bg` | Success message background | Very light green | Very dark green |
| `--color-border-light` | Subtle inner borders | Very light gray | Very dark gray |

## Edge Cases

- **Background image fails to load**: CSS `background-color` fallback on `.app-ambient` in the same tonal range as the image. Glass surfaces still render correctly.
- **`prefers-reduced-transparency: reduce`**: Glass tokens degrade to opaque surfaces with matching background colors. Already handled by existing `.no-glass-blur` and `@supports` rules — new token values must not break this.
- **`prefers-contrast: more`**: Text and border contrast must increase. This may mean bumping text to pure black/white and thickening borders. Implementation chooses the right values.
- **Print**: Not in scope. No `@media print` styles required.
- **Image-heavy meal cards with glass overlay**: The glass scrim (`--glass-scrim`, `--glass-scrim-dark`) must provide sufficient contrast for overlaid text. Test against actual meal images.
- **System theme change while app is open**: CSS media query handles this automatically — no JS required.
- **Browser without `backdrop-filter` support**: Already handled. Glass degrades to opaque `--glass-bg-strong`.

## Research Notes

- Apple Liquid Glass HIG (2025): *"Be judicious with your use of color in controls and navigation so they stay legible and allow your content to infuse them and shine through."* — https://developer.apple.com/documentation/technologyoverviews/liquid-glass
- Apple Adopting Liquid Glass: *"Avoid overusing Liquid Glass effects. Liquid Glass seeks to bring attention to the underlying content."* — Glass is a functional navigation layer, not decoration. — https://developer.apple.com/documentation/technologyoverviews/adopting-liquid-glass
- Dark mode best practices (Atmos): Desaturate colors ~20 points on dark backgrounds, avoid pure black surfaces, avoid pure white text, communicate elevation with lighter surfaces. — https://atmos.style/blog/dark-mode-ui-best-practices
- WCAG 2.1 AA contrast: 4.5:1 for normal text, 3:1 for large text (≥18px or bold ≥14px). — https://www.w3.org/WAI/WCAG21/Understanding/contrast-minimum.html
- Distinctive food app palettes (Figma, Dribbble analysis): The most polished food apps avoid monochromatic warm palettes. Mediterranean combinations (terracotta + teal, olive + clay) feel sophisticated without being generic. An unexpected complement color is the hallmark of designed-vs-generated palettes.

## Assumptions

- No theme toggle UI in v1 — system preference (`prefers-color-scheme`) only. A manual toggle can be added later without token restructuring.
- Exact hex values are chosen by the implementer with design taste. This spec defines token roles, contrast targets, palette direction, and Liquid Glass principles — not specific color codes.
- The ambient background image is a single abstract/blurred image (food-related texture or composition), not a slideshow or user-customizable. Generated or sourced during implementation.
- Existing spacing, radius, motion, and typography tokens are unchanged.
- The glass-morphism CSS infrastructure (`.glass`, `.app-ambient`, degradation paths) is preserved — only token values change.
- Favicon SVG color (`#ff3e00`) is out of scope.
- Meal images (user-uploaded photos) are out of scope.

## Success Criteria

- **SC-001**: All 18 color tokens have light and dark values defined in `app.css`.
- **SC-002**: Every text/background pair in the app meets WCAG AA contrast in both themes (measured with a contrast checker, not estimated).
- **SC-003**: The ambient background renders as an image (not a gradient) behind glass surfaces in both themes.
- **SC-004**: `--color-danger`, `--color-danger-soft`, and `--color-border-light` resolve to real values and are used where previously broken.
- **SC-005**: The primary accent color is not a generic warm orange/brown — it is visually distinctive from the current `#c2410c`.
- **SC-006**: Dark mode glass surfaces are dark-translucent (not white-translucent).
- **SC-007**: All existing tests pass without modification (colors are values-only changes; no markup or class name changes required).
- **SC-008**: The five existing degradation paths (reduced motion, reduced transparency, increased contrast, coarse pointer / low-power, no-glass-blur) still function correctly.
