<script lang="ts">
	import { t } from '$lib/i18n';
	import Icon from '$lib/Icon.svelte';
	import { fade, scale } from 'svelte/transition';
	import { mealImageUrl } from '$lib/api';
	import type { Meal } from '$lib/types';
	import { ImageCapture, looksLikeImage } from '$lib/image-stage.svelte';

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

	// A meal image: any format the backend can decode, one file at a time.
	let removeImage = $state(false);
	const initialFiles = [initialImage].filter((file): file is File => file !== null);
	const capture = new ImageCapture(
		{
			maxFiles: 1,
			allowPlainTextDrag: true,
			// The meal form stages a new file when one is picked; a file that
			// is not an image is the only rejection.
			validateFile: (file) => (looksLikeImage(file) ? null : 'imageErrorNotImage'),
		},
		{
			onchange: (files) => {
				const file = files[0] ?? null;
				if (file) removeImage = false;
				onchange(file, removeImage);
			},
			onerror: (message) => onerror(message),
		},
		initialFiles
	);

	const formImage = $derived(capture.files[0] ?? null);
	let stagedImageUrl = $state<string | null>(null);

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
		const handler = (event: ClipboardEvent) => capture.onPaste(event);
		document.addEventListener('paste', handler);
		return () => document.removeEventListener('paste', handler);
	});

	// --- Upload tile (click-to-browse) ---

	let fileInput: HTMLInputElement | undefined = $state();

	function onBrowseClick() {
		fileInput?.click();
	}

	// --- Remove / restore existing image ---

	function onRemoveImageClick() {
		removeImage = true;
		capture.reset();
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
	const isDragging = $derived(capture.isDragging);
</script>

	<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
	class="image-input"
	ondragenter={isInteractive ? (e) => capture.onDragEnter(e) : undefined}
	ondragover={isInteractive ? (e) => capture.onDragOver(e) : undefined}
	ondragleave={isInteractive ? () => capture.onDragLeave() : undefined}
	ondrop={isInteractive ? (e) => capture.onDrop(e) : undefined}
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
		onchange={(e) => capture.onFileInputChange(e)}
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
					class="overlay-action-btn btn btn--ghost"
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
					class="overlay-action-btn btn btn--ghost"
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
				onclick={() => capture.toggleUrlRow()}
			>
				<Icon name="link" size={24} />
				<span>{t('imageImportUrl')}</span>
			</button>

			<!-- svelte-ignore a11y_no_noninteractive_tabindex -->
			<div
				class="image-tile image-tile--paste"
				tabindex="0"
				onclick={() => capture.onPasteClick()}
				onkeydown={(e) => capture.onPasteKeyDown(e)}
				role="button"
			>
				<Icon name="clipboard" size={24} />
				<span>{t('imageImportPaste')}</span>
			</div>

			<button
				type="button"
				class="image-tile image-tile--drop"
				class:image-tile--drop-active={isDragging}
				onclick={onBrowseClick}
			>
				<Icon name="image-down" size={24} />
				<span>{t('imageImportDragDrop')}</span>
			</button>
		</div>
	{/if}


	<!-- URL input row -->
	{#if capture.urlRowOpen}
		<div class="image-url-row">
			<input
				type="url"
				bind:value={capture.urlInput}
				placeholder={t('fieldImageUrlPlaceholder')}
				disabled={capture.urlLoading}
			/>
			<button
				type="button"
				class="btn btn--ghost"
				onclick={() => capture.loadUrl()}
				disabled={capture.urlLoading || !capture.urlInput.trim()}
			>
				{capture.urlLoading ? t('fieldImageUrlLoading') : t('fieldImageUrlLoad')}
			</button>
		</div>
	{/if}

	<!-- Error messages -->
	{#if capture.error}
		<p class="form-error" role="alert">
			<Icon name="circle-alert" size={18} />
			<span>{capture.error}</span>
		</p>
	{/if}

	{#if capture.urlError}
		<p class="form-error" role="alert">
			<Icon name="circle-alert" size={18} />
			<span>{capture.urlError}</span>
		</p>
	{/if}
</div>

<style>
	/* Shared picker chrome (.image-input, .image-tiles, .image-tile,
	   .image-input__drop-zone, .image-url-row) lives in app.css. */

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
</style>
