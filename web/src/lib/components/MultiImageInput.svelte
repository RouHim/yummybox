<script lang="ts">
	import { t } from '$lib/i18n';
	import Icon from '$lib/Icon.svelte';
	import { fade } from 'svelte/transition';
	import { loadImageFromUrl, ApiError } from '$lib/api';

	let {
		onchange,
		onerror,
	}: {
		onchange: (files: File[]) => void;
		onerror: (error: string | null) => void;
	} = $props();

	const MAX_PHOTOS = 5;

	let files = $state<File[]>([]);
	let thumbUrls = $state<string[]>([]);
	let error = $state<string | null>(null);
	let imageUrl = $state('');
	let imageUrlLoading = $state(false);
	let imageUrlError = $state<string | null>(null);
	let dragDepth = $state(0);
	const isDragging = $derived(dragDepth > 0);
	let urlRowOpen = $state(false);

	// Object URLs for staged thumbnails; revoked on change/cleanup.
	$effect(() => {
		const urls = files.map((f) => URL.createObjectURL(f));
		thumbUrls = urls;
		return () => {
			for (const url of urls) URL.revokeObjectURL(url);
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

	// All-or-nothing: an add that would exceed the cap, or that contains a
	// non-image file, is rejected entirely — no partial staging.
	function addFiles(incoming: File[]) {
		if (incoming.length === 0) return;
		for (const file of incoming) {
			if (!looksLikeImage(file)) {
				error = t('imageErrorNotImage');
				onerror(error);
				return;
			}
		}
		if (files.length + incoming.length > MAX_PHOTOS) {
			error = t('importLlmImageMax');
			onerror(error);
			return;
		}
		files = [...files, ...incoming];
		error = null;
		onerror(null);
		onchange(files);
	}

	function removePhoto(i: number) {
		files = files.filter((_, idx) => idx !== i);
		error = null;
		onerror(null);
		onchange(files);
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
		const dropped = e.dataTransfer?.files;
		if (dropped && dropped.length > 0) {
			addFiles(Array.from(dropped));
			return;
		}
		// Cross-window browser drag: payload is a URL string, not a File.
		const url = readDraggedUrl(e.dataTransfer);
		if (url) {
			loadAndStageFromUrl(url).catch((err) => {
				if (err instanceof ApiError) {
					const msg = err.message || '';
					if (msg.includes('unreachable') || msg.includes('HTTP')) {
						error = t('imageErrorUrlUnreachable');
					} else if (msg.includes('not a recognizable') || msg.includes('corrupt')) {
						error = t('imageErrorUrlNotImage');
					} else {
						error = t('imageErrorUrlGeneric');
					}
				} else {
					error = t('imageErrorUrlGeneric');
				}
				onerror(error);
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
		addFiles(Array.from(target.files ?? []));
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
				addFiles([imageFile]);
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
					addFiles([file]);
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
						addFiles([new File([bytes], 'pasted.png', { type: mime })]);
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
							addFiles([new File([blob], 'pasted.png', { type: blob.type || 'image/png' })]);
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
						addFiles([new File([blob], 'pasted.png', { type })]);
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
		addFiles([file]);
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
</script>

	<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
	class="image-input"
	ondragenter={onDragEnter}
	ondragover={onDragOver}
	ondragleave={onDragLeave}
	ondrop={onDrop}
>
	{#if isDragging}
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
		multiple
		style="display:none"
		bind:this={fileInput}
		onchange={onFileInputChange}
		aria-label={t('fieldImageChoose')}
	/>

	<!-- Staged photo strip: visible order, per-photo remove -->
	{#if files.length > 0}
		<div class="multi-image-thumbs">
			{#each files as file, i (file)}
				<div class="multi-image-thumb" title={file.name}>
					<img src={thumbUrls[i]} alt="" />
					<span class="multi-image-index">{i + 1}</span>
					<button
						type="button"
						class="multi-image-remove"
						aria-label={t('fieldImageRemove')}
						onclick={() => removePhoto(i)}
					>
						<Icon name="trash-2" size={14} />
					</button>
				</div>
			{/each}
		</div>
	{/if}

	<!-- 4-tile grid -->
	<div class="image-tiles" class:tiles-dimmed={isDragging}>
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
	{#if error}
		<p class="form-error" role="alert">
			<Icon name="circle-alert" size={18} />
			<span>{error}</span>
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

	/* --- Staged photo strip --- */

	.multi-image-thumbs {
		display: flex;
		flex-wrap: wrap;
		gap: var(--space-2);
	}

	.multi-image-thumb {
		position: relative;
		width: 88px;
		height: 66px;
		flex: 0 0 88px;
		border-radius: var(--radius-md);
		overflow: hidden;
		border: 1px solid var(--color-border);
		background: var(--color-surface-2);
	}

	.multi-image-thumb img {
		width: 100%;
		height: 100%;
		object-fit: cover;
		display: block;
	}

	.multi-image-index {
		position: absolute;
		top: var(--space-0-5);
		left: var(--space-0-5);
		font-size: var(--text-xs);
		line-height: 1;
		background: var(--glass-bg-strong);
		border: 1px solid var(--glass-border);
		border-radius: var(--radius-full);
		padding: var(--space-0-5) var(--space-1-5);
		color: var(--color-text);
		backdrop-filter: blur(var(--glass-blur-low));
		-webkit-backdrop-filter: blur(var(--glass-blur-low));
		z-index: 1;
	}

	.multi-image-remove {
		position: absolute;
		top: var(--space-0-5);
		right: var(--space-0-5);
		display: flex;
		align-items: center;
		justify-content: center;
		width: 24px;
		height: 24px;
		padding: 0;
		border: 1px solid var(--glass-border);
		border-radius: var(--radius-full);
		background: var(--glass-bg-strong);
		color: var(--color-text);
		cursor: pointer;
		backdrop-filter: blur(var(--glass-blur-low));
		-webkit-backdrop-filter: blur(var(--glass-blur-low));
		z-index: 2;
		transition: background var(--transition-fast), color var(--transition-fast);
	}

	.multi-image-remove:hover {
		background: var(--color-danger-soft, var(--color-primary-soft));
		color: var(--color-danger, var(--color-primary));
	}

	.multi-image-remove:focus-visible {
		outline: 2px solid var(--color-primary);
		outline-offset: 2px;
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
