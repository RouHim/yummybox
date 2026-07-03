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

	// Object URL for staged-image thumbnail preview.
	// Uses a local var so the effect never reads stagedImageUrl (avoids effect_update_depth_exceeded).
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

	// --- DnD handlers ---

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

	// --- Click-to-browse ---

	let fileInput: HTMLInputElement | undefined = $state();

	function onBrowseClick() {
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

	// --- Clipboard paste ---

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

	// --- Remove existing image ---

	function onRemoveImageClick() {
		removeImage = true;
		formImage = null;
		onchange(null, true);
	}
</script>

<div class="image-input">
	<span class="field__label">{t('fieldImageLabel')}</span>

	{#if editMode && editingMeal?.has_image && !removeImage && !formImage}
		<div class="meal-image-controls">
			<img src={mealImageUrl(editingMeal.id)} alt="" class="meal-image-preview" />
			<span class="image-status">{t('fieldImageCurrent')}</span>
			<button type="button" class="btn btn--ghost" onclick={onRemoveImageClick}>
				<Icon name="trash-2" size={14} /> {t('fieldImageRemove')}
			</button>
		</div>
	{/if}

	{#if stagedImageUrl}
		<div class="meal-image-controls">
			<img src={stagedImageUrl} alt="" class="staged-image-preview" />
			<span class="image-status">{t('imageStaged')}</span>
		</div>
	{:else if removeImage}
		<span class="image-status">{t('imageStagedRemove')}</span>
	{/if}

	<!-- svelte-ignore a11y_no_noninteractive_tabindex -->
	<div
		class="image-input__dropzone"
		class:drop-active={isDragging}
		tabindex="0"
		onclick={onBrowseClick}
		onkeydown={onKeyDown}
		onpaste={onPaste}
		ondragenter={onDragEnter}
		ondragover={onDragOver}
		ondragleave={onDragLeave}
		ondrop={onDrop}
		role="button"
		aria-label={t('imageDropPrompt')}
	>
		<Icon name="image" size={24} />
		<span>{t('imageDropPrompt')}</span>
		<input
			type="file"
			accept="image/*"
			id="image-input"
			style="display:none"
			bind:this={fileInput}
			onchange={onFileInputChange}
			aria-label={formImage ? t('fieldImageReplace') : t('fieldImageChoose')}
		/>
	</div>

	<small class="image-paste-hint">{t('imagePasteHint')}</small>

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
	}

	.meal-image-controls {
		display: flex;
		align-items: center;
		gap: var(--space-2);
		margin-bottom: var(--space-2);
	}

	.meal-image-preview {
		max-width: 100px;
		max-height: 100px;
		object-fit: cover;
		border-radius: var(--radius-sm);
	}

	.staged-image-preview {
		max-width: 120px;
		max-height: 120px;
		object-fit: cover;
		border-radius: var(--radius-sm);
		border: 2px solid var(--color-primary);
	}

	.image-status {
		display: block;
		margin-top: var(--space-1);
		font-size: var(--text-xs);
		color: var(--color-text-secondary);
	}

	.image-input__dropzone {
		border: 2px dashed var(--color-border);
		border-radius: var(--radius-lg);
		padding: var(--space-6);
		text-align: center;
		cursor: pointer;
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: var(--space-2);
		color: var(--color-text-secondary);
		font-size: var(--text-sm);
		transition: border-color var(--transition-fast), background var(--transition-fast);
	}

	.image-input__dropzone.drop-active {
		border-color: var(--color-primary);
		background: var(--color-primary-soft);
	}

	@media (pointer: fine) {
		.image-input__dropzone:hover {
			border-color: var(--color-primary-soft);
		}

		.image-input__dropzone.drop-active:hover {
			border-color: var(--color-primary);
		}
	}

	@media (pointer: coarse) {
		.image-input__dropzone.drop-active {
			border-color: var(--color-border);
			background: transparent;
		}
	}

	@media (prefers-reduced-motion: reduce) {
		.image-input__dropzone {
			transition: none;
		}
	}

	@media (prefers-reduced-transparency: reduce) {
		.image-input__dropzone.drop-active {
			background: var(--color-surface);
		}
	}

	.image-url-row {
		display: flex;
		gap: var(--space-2);
		align-items: center;
		margin-bottom: var(--space-1);
	}

	.image-url-row input {
		flex: 1;
		min-width: 0;
	}

	.image-paste-hint {
		display: block;
		font-size: var(--text-xs);
		color: var(--color-text-muted);
	}

	.form-error {
		animation: shake var(--motion-exit);
	}
</style>
