# Feature Specification: Unified Meal Image Input Control

**Created**: 2026-07-03
**Status**: Approved
**Input**: Improve the image insert control in the add/edit meal dialog so it adds an image in three ways (clipboard, URL, local upload) unified in one pretty, reusable Svelte component.

## Goal

Replace the loose vertical stack of a file input, a paste handler, and a URL field currently inline in `web/src/lib/MealForm.svelte` with one reusable Svelte 5 component: a single drop surface that accepts images by drag-and-drop, click-to-browse, clipboard paste, and URL load. The component shows a staged-image preview with replace/remove actions, converges all input methods on one staging path, and reuses the existing backend endpoints (`loadImageFromUrl`, the meal multipart upload) unchanged. No backend changes and no new npm dependency are introduced.

## User Scenarios

### Scenario 1 - Stage an image by drag-and-drop (P1)

A user has an image file in their file manager. They drag the file onto the image area of the add/edit meal form. The area highlights to show it will accept the drop. On drop, the image is staged and its thumbnail preview appears inside the surface.

**Acceptance**

1. Given the meal form is open and no image is staged, when the user drags an image file over the image control, then the control shows a visible drop-active state (e.g. highlighted border).
2. Given an image file is being dragged over the control, when the user releases it over the control, then the file is staged, a thumbnail preview is shown, and the drop-active state clears.
3. Given a staged image already exists, when the user drags a new image file and drops it, then the previously staged image is replaced by the new one (its object URL is revoked) and the new preview is shown.

### Scenario 2 - Stage an image by clicking to browse (P1)

A user clicks the image control. The browser's file picker opens filtered to images. They select a file and it is staged with a preview.

**Acceptance**

1. Given the meal form is open, when the user clicks anywhere on the drop surface, then the native file picker opens with `accept="image/*"`.
2. Given the file picker is open, when the user selects a valid image file and confirms, then the file is staged and its preview replaces any prior staged or current image inside the surface.

### Scenario 3 - Stage an image from clipboard paste (P1)

A user has copied an image to the clipboard (e.g. a screenshot). While focus is within the image control, they press Ctrl/Cmd+V. The image is staged with a preview. Pasting non-image content does nothing.

**Acceptance**

1. Given the meal form is open and the image control is focused, when the user pastes and the clipboard contains an image file, then the image is staged and its preview is shown.
2. Given the image control is focused, when the user pastes and the clipboard contains only text or non-image content, then nothing is staged and no error is shown.

### Scenario 4 - Stage an image from a URL (P1)

A user has a direct image URL. They paste it into the URL field beneath the drop surface and click Load. The backend fetches and converts the image; the form stages it and shows a preview. On failure, a clear inline error appears.

**Acceptance**

1. Given the meal form is open, when the user enters a URL into the URL field and clicks Load (or submits the URL field), then the existing `loadImageFromUrl` API is called and, on success, the returned base64 image is converted to a `File` and staged with a preview.
2. Given the URL load fails because the host is unreachable or returns a non-2xx status, then an inline error using the existing `imageErrorUrlUnreachable` message is shown.
3. Given the URL load fails because the response is not a recognizable image or is corrupt, then an inline error using the existing `imageErrorUrlNotImage` message is shown.
4. Given the URL load fails for any other reason, then the generic `imageErrorUrlGeneric` message is shown.

### Scenario 5 - Preview, replace, or remove a staged image (P1)

After staging, the user sees a thumbnail of the staged image inside the surface. They can replace it by any of the three methods above, or mark it for removal (in edit mode). The control also renders the meal's current image when editing a meal that already has one.

**Acceptance**

1. Given an image is staged, when the surface is rendered, then a thumbnail preview of the staged image is shown inside the surface.
2. Given an image is staged, when the user drags, browses, pastes, or loads a new image, then the old staged image is replaced and its object URL is revoked.
3. Given the form is in edit mode and the meal has an image and no new image is staged and removal is not marked, when the surface is rendered, then the existing meal image is shown via `mealImageUrl(editingMeal.id)` with a Remove control.
4. Given the form is in edit mode and the meal has an image, when the user activates Remove, then the image is marked for removal (`removeImage = true`), the preview is cleared, and the removal status is shown.
5. Given removal was marked and no new image is staged, when the user stages a new image by any method, then the removal mark is cleared (`removeImage = false`) and the new image is staged.

### Scenario 6 - Non-image file rejected inline (P1)

A user drops, pastes, or selects a file that is not an image (e.g. a PDF or text file). Nothing is staged and a clear inline error indicates only image files are accepted.

**Acceptance**

1. Given the meal form is open, when the user drops a non-image file onto the control, then the file is not staged and an inline error indicates only image files are accepted.
2. Given the image control is focused, when the user pastes a file whose MIME type does not start with `image/`, then the file is not staged and the same inline error is shown.
3. Given the file picker is open, when the user selects a non-image file despite the `accept="image/*"` filter, then the file is not staged and the same inline error is shown.

## Functional Requirements

- **FR-001**: A new reusable Svelte 5 component SHALL encapsulate the image input. It is consumed by `web/src/lib/MealForm.svelte` only. The LLM-import image tab is out of scope.
- **FR-002**: The component SHALL render a single drop surface that simultaneously accepts drag-and-drop, click-to-browse, and clipboard paste. A URL text field with a Load button SHALL be rendered beneath the surface. All four input methods converge on one internal `stageImage(file: File | null)` path.
- **FR-003**: Drag-and-drop SHALL use native `DragEvent` handlers (`dragenter`, `dragover`, `dragleave`, `drop`) with a local `$state` boolean for the drop-active visual. `dragover` MUST call `e.preventDefault()` to allow drops. Only drops containing files (checked via `e.dataTransfer?.files`) are considered; drops with no files are ignored.
- **FR-004**: Click on the drop surface SHALL trigger the hidden `<input type="file" accept="image/*">`. The file input MUST be visually hidden but keyboard-accessible.
- **FR-005**: Clipboard paste SHALL be handled by an `onpaste` handler on the surface container (which MUST be focusable via `tabindex="0"`). The handler inspects `e.clipboardData?.files`, stages the first item whose `type` starts with `image/`, and calls `e.preventDefault()` only when an image is found. Non-image pastes are a no-op.
- **FR-006**: URL load SHALL call the existing `loadImageFromUrl(url)` from `$lib/api`. On success, the returned `imageBase64` MUST be decoded to bytes and wrapped as a `File` named `imported.jpg` with type `image/jpeg` (same mapping as the current `onLoadImageUrl`), then staged. The Load button and URL field MUST be disabled while loading. The URL field MUST be cleared on success.
- **FR-007**: URL-load error mapping SHALL be reused verbatim from the current implementation: message containing `unreachable` or `HTTP` → `imageErrorUrlUnreachable`; `not a recognizable` or `corrupt` → `imageErrorUrlNotImage`; otherwise → `imageErrorUrlGeneric`.
- **FR-008**: Client-side MIME validation SHALL reject any staged file whose `type` does not start with `image/` with an inline error (new i18n key `imageErrorNotImage`). Non-image files MUST NOT be staged, MUST NOT clear an existing staged/current image, and MUST NOT set `removeImage`.
- **FR-009**: A staged image preview SHALL be rendered from an object URL (`URL.createObjectURL`). The object URL MUST be revoked when the staged image is replaced, cleared, or the component unmounts, using a `$effect` cleanup that does not read the preview URL reactively (preserving the fix from commit `4697b88`).
- **FR-010**: In edit mode, when the meal has an image and no new image is staged and `removeImage` is false, the component SHALL render the existing image via `mealImageUrl(editingMeal.id)` with a Remove control. Activating Remove SHALL set `removeImage = true`, clear the staged file, and show a removal-status message.
- **FR-011**: The component SHALL expose a callback interface (Svelte 5 `$props`) carrying the current staged `File | null`, the `removeImage` boolean, and the `editingMeal` (when in edit mode). `MealForm.svelte` SHALL use these to drive its existing `onsubmit` payload (`image: File | null`, `removeImage: boolean`) without changing the payload shape or the API calls.
- **FR-012**: The component SHALL honor `prefers-reduced-motion` (no animated drop-active transitions) and `prefers-reduced-transparency` (solid fallback for any translucent surface). Hover/drop-active visual effects MUST NOT activate on coarse-pointer (touch) devices via `@media (pointer: fine)` gating.
- **FR-013**: The component SHALL meet WCAG AA contrast for all text, controls, borders, and error messages against their backgrounds in both light and dark themes. The drop surface, URL field, Load button, and error text all fall under this requirement.
- **FR-014**: All visible strings SHALL be internationalized via the i18n system (`$lib/i18n`). Existing image-related keys (`fieldImageLabel`, `fieldImageChoose`, `fieldImageReplace`, `fieldImageCurrent`, `fieldImageRemove`, `fieldImageUrlLoad`, `fieldImageUrlLoading`, `fieldImageUrlPlaceholder`, `imageErrorUrlGeneric`, `imageErrorUrlNotImage`, `imageErrorUrlUnreachable`, `imageStaged`, `imageStagedRemove`) SHALL be reused. New keys SHALL be added for: the new non-image rejection message (`imageErrorNotImage`), the paste hint (`imagePasteHint`, replacing the hardcoded `Ctrl+V to paste from clipboard` string), and the drop-surface prompt (`imageDropPrompt`). Each new key MUST be defined in both `en` and `de` translation tables and added to the `TranslationKey` union type.
- **FR-015**: Styling SHALL use plain CSS (no framework), matching the existing project tokens (`--space-*`, `--color-*`, `--radius-*`). The drop surface SHALL be the single primary affordance; the URL row sits beneath it. No new CSS file is introduced.
- **FR-016**: No backend, API, type, or Cargo changes SHALL be made. No new npm dependency SHALL be added. All image processing (format conversion, downscale, JPEG quality 82) remains server-side via existing endpoints.

## Key Entities

- **StagedImageFile**: The `File | null` currently held by the component, surfaced to `MealForm` via callback. Source-agnostic: any of the four input methods produces it.
- **removeImage**: Boolean flag, true when the user explicitly removed the existing image in edit mode. Surfaced to `MealForm` and included in the submit payload unchanged.

## Edge Cases

- **Drag with no files** (e.g. text dragged from another page): `dragover`/`drop` inspect `e.dataTransfer?.files`; if empty, the event is ignored and no drop-active visual persists.
- **Paste with no files** (text-only clipboard): `onpaste` finds no `image/*` item; nothing is staged, no error shown, no `preventDefault` called (default paste proceeds).
- **Non-image file via the hidden file input** (user overrides the `accept` filter): client-side MIME check in `stageImage` rejects it with the `imageErrorNotImage` message; nothing is staged or cleared.
- **URL field empty or whitespace-only**: Load is a no-op (button disabled when the trimmed value is empty, matching current behavior).
- **URL load race**: while a URL load is in-flight, all input methods remain enabled; successfully staging via another method cancels the prior staged result when it lands. The URL result still sets its own preview on success.
- **Object URL leak**: the `$effect` capturing `formImage` creates and revokes the object URL on every change and on unmount, so replacing or removing a staged image never leaks (regression guard for `4697b88`).
- **Replace-then-remove on edit mode**: if the user stages a new image then clicks Remove, `removeImage` is set and the staged file cleared; a subsequent re-stage clears `removeImage`.
- **Coarse-pointer (touch) devices**: drop-active hover effects are gated to `@media (pointer: fine)`; touch users still get click-to-browse, paste (when supported), and URL load.
- **`prefers-reduced-motion`**: drop-active border transition collapses to an instant color swap.

## Research Notes

- [github.com/saabi/svelte-image-input](https://github.com/saabi/svelte-image-input) — a Svelte component handling drag-and-drop, clipboard paste, and click-to-browse in a single surface with a data-URL callback. Validates the unified-surface pattern; confirms no library is required.
- [dev.to/artxe2/implementing-drag-and-drop-using-svelte-5-767](https://dev.to/artxe2/implementing-drag-and-drop-using-svelte-5-767) — Svelte 5 drag-and-drop guide using `$state` for an `isDragging` flag and `dragenter`/`dragover`/`dragleave`/`drop` handlers; the approach this spec mandates.
- [sv5ui.vercel.app/docs/components/file-upload](https://sv5ui.vercel.app/docs/components/file-upload) — reference Svelte 5 file-upload component with image previews and DnD; confirms the single-surface plus-preview pattern is idiomatic.

## Assumptions

- No backend changes are required: all four input methods produce a staged `File`, and upload happens on Save exactly as today.
- No new npm dependency is needed; native browser APIs (`DragEvent`, `ClipboardEvent`, `URL.createObjectURL`, `FileReader`/`atob`) suffice.
- The component is reusable but only `MealForm.svelte` consumes it in this spec; the LLM-import image tab is explicitly out of scope.
- The existing URL-load error-message mapping (substring matching on `unreachable`/`HTTP`, `not a recognizable`/`corrupt`, else generic) is reused verbatim.
- `MealForm.svelte`'s `onsubmit` payload shape (`{ name, ingredients, instructions, image, removeImage }`) is unchanged; the component only changes how the `image` and `removeImage` values are produced in the UI.
- The hidden file input remains the accessibility path for browse; the drop surface click forwards to it.
- Existing i18n keys cover most visible strings; only three new keys are introduced (see FR-014).

## Success Criteria

- **SC-001**: Dragging an image file onto the drop surface stages the image and shows a thumbnail preview; releasing a non-image file over the surface shows an inline error and stages nothing.
- **SC-002**: Clicking the drop surface opens the native file picker (image-only filter); selecting a valid image stages and previews it.
- **SC-003**: With the image control focused, pasting an image from the clipboard stages and previews it; pasting non-image content is a no-op.
- **SC-004**: Entering a reachable image URL and clicking Load stages and previews the image; an unreachable URL shows the `imageErrorUrlUnreachable` error; a non-image response shows the `imageErrorUrlNotImage` error.
- **SC-005**: In edit mode on a meal with an image, the existing image is previewed; clicking Remove marks removal and clears the preview; staging a new image afterwards clears the removal mark.
- **SC-006**: Replacing a staged image revokes the previous object URL (no leak); unmounting the component revokes any outstanding object URL.
- **SC-007**: Saving a meal with a staged image uploads it through the existing multipart endpoint and the meal displays a thumbnail in the list; saving with `removeImage` set clears the meal's image without deleting the meal.
- **SC-008**: All new visible strings are translated in both `en` and `de`; the hardcoded `Ctrl+V to paste from clipboard` string is gone.
- **SC-009**: `cd web && npm run check` and `cd web && npm test` pass with no regressions; the existing E2E image suite (`tests/e2e/meal-images.spec.ts`) passes unchanged.
- **SC-010**: The drop surface and all its controls meet WCAG AA contrast in both light and dark themes; the surface is keyboard-focusable and accepts paste while focused.
