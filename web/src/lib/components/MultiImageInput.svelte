<script lang="ts">
	import { t } from '$lib/i18n';
	import Icon from '$lib/Icon.svelte';
	import { fade } from 'svelte/transition';
	import { tierDuration } from '$lib/motion';
	import {
		MAX_GENERATE_IMAGES,
		validateGenerateImage,
		validateGenerateImageTotal
	} from '$lib/multi-image';
	import { ImageCapture } from '$lib/image-stage.svelte';

	let {
		onchange,
		onerror,
	}: {
		onchange: (files: File[]) => void;
		onerror: (error: string | null) => void;
	} = $props();

	// Photos for the AI flow: the strict MIME allowlist and size ceilings the
	// backend enforces, with the same messages the user would see from it.
	const capture = new ImageCapture(
		{
			maxFiles: MAX_GENERATE_IMAGES,
			tooManyMessage: 'importLlmImageMax',
			ignoreEditablePaste: true,
			// The AI flows only accept the MIME types the model providers take
			// and reject images over 20 MiB, so both are rejected up front
			// (also for pasted images) instead of after an upload.
			validateFile: validateGenerateImage,
			validateTotal: validateGenerateImageTotal,
		},
		{
			onchange: (files) => onchange(files),
			onerror: (message) => onerror(message),
		}
	);

	const files = $derived(capture.files);

	let thumbUrls = $state<string[]>([]);
	let fileInput: HTMLInputElement | undefined = $state();

	// Object URLs for staged thumbnails; revoked on change/cleanup.
	$effect(() => {
		const urls = files.map((file) => URL.createObjectURL(file));
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
		const handler = (event: ClipboardEvent) => capture.onPaste(event);
		document.addEventListener('paste', handler);
		return () => document.removeEventListener('paste', handler);
	});

	function onBrowseClick() {
		fileInput?.click();
	}
</script>

	<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
	class="image-input"
	ondragenter={(e) => capture.onDragEnter(e)}
	ondragover={(e) => capture.onDragOver(e)}
	ondragleave={() => capture.onDragLeave()}
	ondrop={(e) => capture.onDrop(e)}
>
	{#if capture.isDragging}
		<div class="image-input__drop-zone" aria-hidden="true" transition:fade={{ duration: tierDuration(150) }}>
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
		onchange={(e) => capture.onFileInputChange(e)}
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
						onclick={() => capture.remove(i)}
					>
						<Icon name="trash-2" size={14} />
					</button>
				</div>
			{/each}
		</div>
	{/if}

	<!-- 4-tile grid -->
	<div class="image-tiles" class:tiles-dimmed={capture.isDragging}>
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
			class:image-tile--drop-active={capture.isDragging}
			onclick={onBrowseClick}
		>
			<Icon name="image-down" size={24} />
			<span>{t('imageImportDragDrop')}</span>
		</button>
	</div>

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
</style>
