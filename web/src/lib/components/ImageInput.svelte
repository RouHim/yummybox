<script lang="ts">
	import { t } from '$lib/i18n';
	import Icon from '$lib/Icon.svelte';
	import { fade, scale } from 'svelte/transition';
	import { mealImageUrl, loadImageFromUrl, ApiError } from '$lib/api';
	import type { Meal } from '$lib/types';

	let {
		editingMeal = null,
		editMode = false,
		initialImage = null,
		onchange,
		onerror,
	}: {
		editingMeal?: Meal | null;
		editMode?: boolean;
		initialImage?: File | null;
		onchange: (file: File | null, removeImage: boolean) => void;
		onerror: (error: string | null) => void;
	} = $props();

	let formImage = $state<File | null>(initialImage);
	let removeImage = $state(false);
	let stagedImageUrl = $state<string | null>(null);
	let imageUrl = $state('');
	let imageUrlLoading = $state(false);
	let imageUrlError = $state<string | null>(null);
	let imageError = $state<string | null>(null);
	let dragDepth = $state(0);
	const isDragging = $derived(dragDepth > 0);
	let urlRowOpen = $state(false);

	// Object URL for staged-image thumbnail preview.
	$effect(() => {
		const file = formImage;
		const url = file ? URL.createObjectURL(file) : null;
		stagedImageUrl = url;
		return () => {
			if (url) URL.revokeObjectURL(url);
		};
	});

	// Listen for paste at document level: the paste event only fires reliably
	// on editable contexts (input/textarea/contenteditable), not on a plain
	// focused div. Attaching to document catches Ctrl+V/Cmd+V while the dialog
	// is mounted. See https://web.dev/patterns/clipboard/paste-images/.
	$effect(() => {
		const handler = (e: ClipboardEvent) => onPaste(e);
		document.addEventListener('paste', handler);
		return () => document.removeEventListener('paste', handler);
	});

	function looksLikeImage(file: File): boolean {
		if (file.type.startsWith('image/')) return true;
		const ext = file.name.split('.').pop()?.toLowerCase();
		return ['png', 'jpg', 'jpeg', 'webp', 'gif', 'avif', 'bmp', 'tiff'].includes(ext ?? '');
	}

	function stageImage(file: File | null) {
		if (file && !looksLikeImage(file)) {
			imageError = t('imageErrorNotImage');
			onerror(imageError);
			return;
		}
		imageError = null;
		onerror(null);
		if (file) {
			formImage = file;
			removeImage = false;
		}
		onchange(formImage, removeImage);
	}

	// --- DnD handlers (whole component surface) ---

	function isFileDrag(dt: DataTransfer | null): boolean {
		if (!dt) return false;
		// Browsers protect the file list during dragenter/dragover, so it can
		// be empty even when the user is dragging files. The `types` list is
		// the reliable signal.
		if (dt.files.length > 0) return true;
		// dt.types is a DOMStringList; convert to array so .includes() works
		// both in real browsers and in synthetic test DataTransfer objects.
		const types = Array.from(dt.types);
		if (types.includes('Files')) return true;
		return types.includes('text/uri-list') || types.includes('text/plain');
	}

	function onDragEnter(e: DragEvent) {
		if (isFileDrag(e.dataTransfer)) {
			dragDepth++;
		}
		e.preventDefault();
	}

	function onDragOver(e: DragEvent) {
		e.preventDefault();
	}

	function onDragLeave(_e: DragEvent) {
		dragDepth = Math.max(0, dragDepth - 1);
	}

	function readDraggedUrl(dt: DataTransfer | null): string | null {
		if (!dt) return null;
		// Standard MIME type for URI drags; Firefox/Chrome both populate it
		// for image/link/tab drags. URI lists may contain multiple URLs
		// newline-separated with '#' comment lines — take the first non-comment.
		const uriList = dt.getData('text/uri-list');
		if (uriList) {
			for (const line of uriList.split(/\r?\n/)) {
				const trimmed = line.trim();
				if (trimmed && !trimmed.startsWith('#')) return trimmed;
			}
		}
		// Firefox-specific: "URL\ntitle" on two lines.
		const mozUrl = dt.getData('text/x-moz-url');
		if (mozUrl) {
			const firstLine = mozUrl.split(/\r?\n/)[0]?.trim();
			if (firstLine) return firstLine;
		}
		// Fallback: plain text (some drags only populate this).
		const plain = dt.getData('text/plain').trim();
		if (plain) return plain;
		return null;
	}

	function onDrop(e: DragEvent) {
		e.preventDefault();
		dragDepth = 0;
		const files = e.dataTransfer?.files;
		if (files && files.length > 0) {
			const file = files[0];
			if (file) stageImage(file);
			return;
		}
		// Cross-window browser drag: payload is a URL string, not a File.
		const url = readDraggedUrl(e.dataTransfer);
		if (url) {
			loadAndStageFromUrl(url).catch((err) => {
				if (err instanceof ApiError) {
					const msg = err.message || '';
					if (msg.includes('unreachable') || msg.includes('HTTP')) {
						imageError = t('imageErrorUrlUnreachable');
					} else if (msg.includes('not a recognizable') || msg.includes('corrupt')) {
						imageError = t('imageErrorUrlNotImage');
					} else {
						imageError = t('imageErrorUrlGeneric');
					}
				} else {
					imageError = t('imageErrorUrlGeneric');
				}
				onerror(imageError);
			});
		}
	}

	// --- Upload tile (click-to-browse) ---

	let fileInput: HTMLInputElement | undefined = $state();

	function onBrowseClick() {
		fileInput?.click();
	}


	function onFileInputChange(e: Event) {
		const target = e.target as HTMLInputElement;
		const file = target.files?.[0] ?? null;
		stageImage(file);
		target.value = '';
	}

	// --- Paste tile ---

	function onPaste(e: ClipboardEvent) {
		const cd = e.clipboardData;
		if (!cd) return;

		// Chrome/Edge populate clipboardData.files for image pastes.
		if (cd.files.length > 0) {
			const imageFile = Array.from(cd.files).find((f) => f.type.startsWith('image/'));
			if (imageFile) {
				e.preventDefault();
				stageImage(imageFile);
				return;
			}
		}

		// Firefox (Linux/macOS): clipboardData.files is empty; DataTransferItemList
		// has image items with kind='file'.
		const items = cd.items;
		for (let i = 0; i < items.length; i++) {
			const item = items[i];
			if (item.kind === 'file' && item.type.startsWith('image/')) {
				const file = item.getAsFile();
				if (file) {
					e.preventDefault();
					stageImage(file);
					return;
				}
			}
		}

		// Firefox (Windows): OS screenshots arrive as text/html with an embedded
		// <img> tag containing a data: URI (e.g. Win+Shift+S).
		const html = cd.getData('text/html');
		if (html) {
			const doc = new DOMParser().parseFromString(html, 'text/html');
			const img = doc.querySelector('img');
			if (img?.src) {
				const src = img.src;
				if (src.startsWith('data:')) {
					e.preventDefault();
					const comma = src.indexOf(',');
					if (comma === -1) return;
					const header = src.slice(0, comma);
					const mime = header.split(':')[1]?.split(';')[0] || 'image/png';
					const b64 = src.slice(comma + 1);
					try {
						const binary = atob(b64);
						const bytes = new Uint8Array(binary.length);
						for (let j = 0; j < binary.length; j++) {
							bytes[j] = binary.charCodeAt(j);
						}
						stageImage(new File([bytes], 'pasted.png', { type: mime }));
					} catch {
						// Invalid base64 — ignore.
					}
					return;
				}
				if (src.startsWith('blob:')) {
					e.preventDefault();
					fetch(src)
						.then((r) => r.blob())
						.then((blob) => {
							stageImage(new File([blob], 'pasted.png', { type: blob.type || 'image/png' }));
						})
						.catch(() => {});
					return;
				}
			}
		}
	}

	// Click handler for the Paste tile: reads from the system clipboard via the
	// async Clipboard API. Requires a user gesture (click), which is satisfied.
	async function onPasteClick() {
		try {
			const clipboardItems = await navigator.clipboard.read();
			for (const item of clipboardItems) {
				for (const type of item.types) {
					if (type.startsWith('image/')) {
						const blob = await item.getType(type);
						stageImage(new File([blob], 'pasted.png', { type }));
						return;
					}
				}
			}
		} catch {
			// Clipboard read not supported or denied — silent no-op.
		}
	}

	function onPasteKeyDown(e: KeyboardEvent) {
		if (e.key === 'Enter' || e.key === ' ') {
			e.preventDefault();
			onPasteClick();
		}
	}

	// --- URL load ---

	async function loadAndStageFromUrl(url: string): Promise<void> {
		const resp = await loadImageFromUrl(url);
		const bytes = Uint8Array.from(atob(resp.imageBase64), (c) => c.charCodeAt(0));
		const file = new File([bytes], 'imported.jpg', { type: 'image/jpeg' });
		stageImage(file);
	}

	async function onLoadImageUrl() {
		imageUrlError = null;
		const url = imageUrl.trim();
		if (!url) return;
		imageUrlLoading = true;
		try {
			await loadAndStageFromUrl(url);
			imageUrl = '';
			urlRowOpen = false;
		} catch (err) {
			if (err instanceof ApiError) {
				const msg = err.message || '';
				if (msg.includes('unreachable') || msg.includes('HTTP')) {
					imageUrlError = t('imageErrorUrlUnreachable');
				} else if (msg.includes('not a recognizable') || msg.includes('corrupt')) {
					imageUrlError = t('imageErrorUrlNotImage');
				} else {
					imageUrlError = t('imageErrorUrlGeneric');
				}
			} else {
				imageUrlError = t('imageErrorUrlGeneric');
			}
		} finally {
			imageUrlLoading = false;
		}
	}

	function toggleUrlRow() {
		urlRowOpen = !urlRowOpen;
	}

	// --- Remove / restore existing image ---

	function onRemoveImageClick() {
		removeImage = true;
		formImage = null;
		onchange(null, true);
	}

	function onRemoveImageClickStop(e: MouseEvent) {
		e.stopPropagation();
		onRemoveImageClick();
	}

	function onRestoreImage() {
		removeImage = false;
		onchange(formImage, removeImage);
	}

	// --- Derived state ---

	const hasExisting = $derived(editMode && editingMeal?.has_image && !removeImage && !formImage);
	const hasStaged = $derived(stagedImageUrl !== null);
	const isInteractive = $derived(!removeImage);
</script>

	<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
	class="image-input"
	ondragenter={isInteractive ? onDragEnter : undefined}
	ondragover={isInteractive ? onDragOver : undefined}
	ondragleave={isInteractive ? onDragLeave : undefined}
	ondrop={isInteractive ? onDrop : undefined}
>
	{#if isDragging && isInteractive}
		<div class="image-input__drop-zone" aria-hidden="true" transition:fade={{ duration: 150 }}>
			<div class="image-input__drop-zone-inner">
				<Icon name="image-down" size={40} />
				<span>{t('imageImportDragDrop')}</span>
			</div>
		</div>
	{/if}
	<input
		type="file"
		accept="image/*"
		style="display:none"
		bind:this={fileInput}
		onchange={onFileInputChange}
		aria-label={formImage ? t('fieldImageReplace') : t('fieldImageChoose')}
	/>

	<!-- Preview stage: staged image -->
	{#if hasStaged}
		<div class="image-preview" transition:scale={{ duration: 200, start: 0.96, opacity: 0 }}>
			<img src={stagedImageUrl!} alt="" class="staged-image-preview" />
			<div class="image-badge">{t('imageStaged')}</div>
			<div class="image-preview__overlay">
				<button
					type="button"
					class="image-preview__action-btn btn btn--ghost"
					onclick={onRemoveImageClickStop}
					aria-label={t('fieldImageRemove')}
				>
					<Icon name="trash-2" size={16} />
				</button>
			</div>
		</div>
	{:else if hasExisting}
		<!-- Preview stage: existing image (edit mode) -->
		<div class="image-preview" transition:scale={{ duration: 200, start: 0.96, opacity: 0 }}>
			<img src={mealImageUrl(editingMeal!.id)} alt="" />
			<div class="image-preview__overlay">
				<button
					type="button"
					class="image-preview__action-btn btn btn--ghost"
					onclick={onRemoveImageClickStop}
					aria-label={t('fieldImageRemove')}
				>
					<Icon name="trash-2" size={16} />
				</button>
			</div>
		</div>
	{:else if removeImage}
		<!-- Removing state -->
		<div class="image-removing">
			<span>{t('imageStagedRemove')}</span>
			<button type="button" class="btn btn--ghost" onclick={onRestoreImage}>
				{t('buttonCancel')}
			</button>
		</div>
	{/if}

	<!-- 4-tile grid: always visible except when removing -->
	{#if !removeImage}
		<div class="image-tiles" class:tiles-dimmed={isDragging && isInteractive}>
			<button
				type="button"
				class="image-tile"
				onclick={onBrowseClick}
			>
				<Icon name="upload" size={24} />
				<span>{t('imageImportUpload')}</span>
			</button>

			<button
				type="button"
				class="image-tile"
				onclick={toggleUrlRow}
			>
				<Icon name="link" size={24} />
				<span>{t('imageImportUrl')}</span>
			</button>

			<!-- svelte-ignore a11y_no_noninteractive_tabindex -->
			<div
				class="image-tile image-tile--paste"
				tabindex="0"
				onclick={onPasteClick}
				onkeydown={onPasteKeyDown}
				role="button"
			>
				<Icon name="clipboard" size={24} />
				<span>{t('imageImportPaste')}</span>
			</div>

			<div
				class="image-tile image-tile--drop"
				class:image-tile--drop-active={isDragging}
			>
				<Icon name="image-down" size={24} />
				<span>{t('imageImportDragDrop')}</span>
			</div>
		</div>
	{/if}


	<!-- URL input row -->
	{#if urlRowOpen}
		<div class="image-url-row">
			<input
				type="url"
				bind:value={imageUrl}
				placeholder={t('fieldImageUrlPlaceholder')}
				disabled={imageUrlLoading}
			/>
			<button
				type="button"
				class="btn btn--ghost"
				onclick={onLoadImageUrl}
				disabled={imageUrlLoading || !imageUrl.trim()}
			>
				{imageUrlLoading ? t('fieldImageUrlLoading') : t('fieldImageUrlLoad')}
			</button>
		</div>
	{/if}

	<!-- Error messages -->
	{#if imageError}
		<p class="form-error" role="alert">
			<Icon name="circle-alert" size={18} />
			<span>{imageError}</span>
		</p>
	{/if}

	{#if imageUrlError}
		<p class="form-error" role="alert">
			<Icon name="circle-alert" size={18} />
			<span>{imageUrlError}</span>
		</p>
	{/if}
</div>

<style>
	.image-input {
		display: flex;
		position: relative;
		flex-direction: column;
		gap: var(--space-2);
		border-radius: var(--radius-lg);
		transition: box-shadow var(--transition-fast);
	}


	/* --- 4-tile grid --- */

	.image-tiles {
		display: grid;
		grid-template-columns: 1fr 1fr;
		gap: var(--space-2);
	}

	@media (max-width: 639px) {
		.image-tiles {
			grid-template-columns: 1fr;
		}
	}

	.image-tile {
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		gap: var(--space-2);
		padding: var(--space-4);
		min-height: 96px;
		background: var(--color-surface-2);
		border: 2px dashed var(--color-border);
		border-radius: var(--radius-lg);
		color: var(--color-text-secondary);
		font-size: var(--text-sm);
		font-family: var(--font-sans);
		cursor: pointer;
		transition: border-color var(--transition-fast), background var(--transition-fast),
			transform var(--transition-fast), box-shadow var(--transition-fast),
			opacity var(--transition-fast);
	}

	.image-tile:hover {
		border-color: var(--color-primary);
		background: var(--color-primary-soft);
		color: var(--color-primary);
	}

	.image-tile:active {
		transform: scale(0.98);
	}

	.image-tile:focus-visible {
		outline: 2px solid var(--color-primary);
		outline-offset: 2px;
	}

	.image-tile--paste:focus-visible {
		outline: 2px solid var(--color-primary);
		outline-offset: 2px;
	}

	.image-tile--drop-active {
		border-color: var(--color-primary);
		border-style: solid;
		background: var(--color-primary-soft);
		color: var(--color-primary);
		box-shadow: inset 0 0 0 1px var(--color-primary);
	}

	/* --- Tile dimming during drag-over --- */

	.image-tiles.tiles-dimmed .image-tile {
		transform: scale(0.98);
		opacity: 0.55;
	}

	.image-tiles.tiles-dimmed .image-tile--drop-active {
		transform: scale(1.02);
		opacity: 1;
	}

	/* --- Full-surface drop-zone overlay --- */

	.image-input__drop-zone {
		position: absolute;
		inset: calc(-1 * var(--space-2));
		z-index: 5;
		border-radius: var(--radius-lg);
		background: var(--glass-bg-strong);
		border: 2px dashed var(--color-primary);
		backdrop-filter: blur(var(--glass-blur)) saturate(var(--glass-saturation));
		-webkit-backdrop-filter: blur(var(--glass-blur)) saturate(var(--glass-saturation));
		display: flex;
		align-items: center;
		justify-content: center;
		pointer-events: none;
		animation: drop-zone-pulse 1.6s ease-in-out infinite;
	}

	.image-input__drop-zone-inner {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: var(--space-2);
		color: var(--color-primary);
		font-family: var(--font-sans);
		font-size: var(--text-sm);
		font-weight: var(--weight-medium);
	}

	@keyframes drop-zone-pulse {
		0%, 100% { box-shadow: 0 0 0 0 rgba(124, 45, 18, 0.0); }
		50%      { box-shadow: 0 0 0 6px rgba(124, 45, 18, 0.08); }
	}

	@media (prefers-color-scheme: dark) {
		.image-input__drop-zone {
			background: var(--glass-bg-strong);
			border-color: var(--color-primary);
		}
		@keyframes drop-zone-pulse {
			0%, 100% { box-shadow: 0 0 0 0 rgba(217, 119, 87, 0.0); }
			50%      { box-shadow: 0 0 0 6px rgba(217, 119, 87, 0.12); }
		}
	}

	/* --- Preview stage --- */

	.image-preview {
		position: relative;
		aspect-ratio: 16 / 9;
		background: var(--color-primary-soft);
		border-radius: var(--radius-lg);
		overflow: hidden;
		border: 2px solid transparent;
		transition: border-color var(--transition-fast);
	}

	.image-preview img {
		width: 100%;
		height: 100%;
		object-fit: cover;
		display: block;
	}

	/* --- Staged badge --- */

	.image-badge {
		position: absolute;
		top: var(--space-2);
		left: var(--space-2);
		font-size: var(--text-xs);
		background: var(--glass-bg-strong);
		border: 1px solid var(--glass-border);
		border-radius: var(--radius-full);
		padding: var(--space-0-5) var(--space-2);
		color: var(--color-text);
		backdrop-filter: blur(var(--glass-blur-low));
		-webkit-backdrop-filter: blur(var(--glass-blur-low));
		z-index: 1;
	}

	/* --- Overlay action bar --- */

	.image-preview__overlay {
		position: absolute;
		top: var(--space-2);
		right: var(--space-2);
		display: flex;
		gap: var(--space-1);
		padding: var(--space-1);
		background: var(--glass-bg-strong);
		border: 1px solid var(--glass-border);
		border-radius: var(--radius-full);
		box-shadow: var(--shadow-md);
		opacity: 0;
		pointer-events: none;
		transition: opacity var(--transition-fast);
		z-index: 2;
	}

	.image-preview:hover .image-preview__overlay,
	.image-preview:focus-within .image-preview__overlay {
		opacity: 1;
		pointer-events: auto;
	}

	@media (hover: none) {
		.image-preview__overlay {
			opacity: 1;
			pointer-events: auto;
		}
	}

	.image-preview__action-btn {
		min-height: 32px;
		padding: var(--space-1) var(--space-2);
		border: 0;
	}

	/* --- Removing state --- */

	.image-removing {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: var(--space-2);
		padding: var(--space-8) var(--space-4);
		background: var(--color-surface-2);
		border: 2px dashed var(--color-border);
		border-radius: var(--radius-lg);
		color: var(--color-text-secondary);
		font-size: var(--text-sm);
	}

	/* --- URL input row --- */

	.image-url-row {
		display: flex;
		gap: var(--space-2);
		align-items: center;
	}

	.image-url-row input {
		flex: 1;
		min-width: 0;
	}

	/* --- Error messages --- */

	.form-error {
		animation: shake var(--motion-exit);
	}

	/* --- Accessibility --- */

	@media (prefers-reduced-motion: reduce) {
		.image-tile {
			transition: none;
		}
		.image-preview {
			transition: none;
		}
		.image-preview__overlay {
			transition: none;
		}
		.form-error {
			animation: none;
		}
		.image-input__drop-zone {
			animation: none;
		}
		.image-tiles.tiles-dimmed .image-tile,
		.image-tiles.tiles-dimmed .image-tile--drop-active {
			transition: none;
		}
	}

	@media (prefers-reduced-transparency: reduce) {
		.image-tile--drop-active {
			background: var(--color-surface);
		}
		.image-input__drop-zone {
			background: var(--color-primary-soft);
			backdrop-filter: none;
			-webkit-backdrop-filter: none;
		}
	}
</style>
