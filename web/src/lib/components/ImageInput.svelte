<script lang="ts">
	import { t } from '$lib/i18n';
	import Icon from '$lib/Icon.svelte';
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
	let isDragging = $state(false);
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

	function stageImage(file: File | null) {
		if (file && !file.type.startsWith('image/')) {
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

	function onDragEnter(e: DragEvent) {
		if (e.dataTransfer?.files?.length) {
			isDragging = true;
		}
		e.preventDefault();
	}

	function onDragOver(e: DragEvent) {
		e.preventDefault();
	}

	function onDragLeave(_e: DragEvent) {
		isDragging = false;
	}

	function onDrop(e: DragEvent) {
		e.preventDefault();
		isDragging = false;
		const files = e.dataTransfer?.files;
		if (!files || files.length === 0) return;
		const file = files[0];
		if (file) stageImage(file);
	}

	// --- Upload tile (click-to-browse) ---

	let fileInput: HTMLInputElement | undefined = $state();

	function onBrowseClick() {
		fileInput?.click();
	}

	function onBrowseClickStop(e: MouseEvent) {
		e.stopPropagation();
		fileInput?.click();
	}

	function onKeyDown(e: KeyboardEvent) {
		if (e.key === 'Enter' || e.key === ' ') {
			e.preventDefault();
			onBrowseClick();
		}
	}

	function onFileInputChange(e: Event) {
		const target = e.target as HTMLInputElement;
		const file = target.files?.[0] ?? null;
		stageImage(file);
		target.value = '';
	}

	// --- Paste tile ---

	function onPaste(e: ClipboardEvent) {
		const items = e.clipboardData?.files;
		if (!items || items.length === 0) return;
		const imageFile = Array.from(items).find((f) => f.type.startsWith('image/'));
		if (imageFile) {
			e.preventDefault();
			stageImage(imageFile);
		}
	}

	// --- URL load ---

	async function onLoadImageUrl() {
		imageUrlError = null;
		const url = imageUrl.trim();
		if (!url) return;
		imageUrlLoading = true;
		try {
			const resp = await loadImageFromUrl(url);
			const bytes = Uint8Array.from(atob(resp.imageBase64), (c) => c.charCodeAt(0));
			const file = new File([bytes], 'imported.jpg', { type: 'image/jpeg' });
			stageImage(file);
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
	class:drop-active={isDragging && isInteractive}
	ondragenter={isInteractive ? onDragEnter : undefined}
	ondragover={isInteractive ? onDragOver : undefined}
	ondragleave={isInteractive ? onDragLeave : undefined}
	ondrop={isInteractive ? onDrop : undefined}
>
	<input
		type="file"
		accept="image/*"
		style="display:none"
		bind:this={fileInput}
		onchange={onFileInputChange}
		aria-label={formImage ? t('fieldImageReplace') : t('fieldImageChoose')}
	/>

	<!-- 4-tile grid: empty state -->
	{#if !hasStaged && !hasExisting && !removeImage}
		<div class="image-tiles">
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
				onpaste={onPaste}
				onkeydown={onKeyDown}
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

	<!-- Preview stage: staged image -->
	{#if hasStaged}
		<div class="image-preview">
			<img src={stagedImageUrl!} alt="" class="staged-image-preview" />
			<div class="image-badge">{t('imageStaged')}</div>
			<div class="image-preview__overlay">
				<button
					type="button"
					class="image-preview__action-btn btn btn--ghost"
					onclick={onBrowseClickStop}
					aria-label={t('fieldImageReplace')}
				>
					<Icon name="image" size={16} />
				</button>
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
		<div class="image-preview">
			<img src={mealImageUrl(editingMeal!.id)} alt="" />
			<div class="image-preview__overlay">
				<button
					type="button"
					class="image-preview__action-btn btn btn--ghost"
					onclick={onBrowseClickStop}
					aria-label={t('fieldImageReplace')}
				>
					<Icon name="image" size={16} />
				</button>
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

	<!-- URL row toggle (always visible when image is shown) -->
	{#if hasStaged || hasExisting}
		<div class="image-meta">
			<button
				type="button"
				class="image-url-toggle"
				onclick={toggleUrlRow}
			>
				{urlRowOpen ? t('buttonCancel') : t('fieldImageUrlLoad')}
			</button>
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
		flex-direction: column;
		gap: var(--space-2);
		border-radius: var(--radius-lg);
		transition: box-shadow var(--transition-fast);
	}

	.image-input.drop-active {
		box-shadow: inset 0 0 0 2px var(--color-primary);
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
			transform var(--transition-fast), box-shadow var(--transition-fast);
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

	/* --- URL toggle + row --- */

	.image-meta {
		display: flex;
		align-items: center;
		gap: var(--space-2);
		font-size: var(--text-sm);
	}

	.image-url-toggle {
		background: none;
		border: 0;
		padding: 0;
		font-size: var(--text-sm);
		color: var(--color-primary);
		cursor: pointer;
		text-decoration: underline;
	}

	.image-url-toggle:hover {
		color: var(--color-primary-hover);
	}

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
	}

	@media (prefers-reduced-transparency: reduce) {
		.image-tile--drop-active {
			background: var(--color-surface);
		}
	}
</style>
