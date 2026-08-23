<script lang="ts">
	import { t } from '$lib/i18n';
	import Icon from '$lib/Icon.svelte';
	import { MAX_GENERATE_IMAGES, validateGenerateImage, validateGenerateImageTotal } from '$lib/multi-image';

	let {
		files = $bindable([]),
		disabled = false,
		onerror,
	}: {
		files?: File[];
		disabled?: boolean;
		onerror: (error: string | null) => void;
	} = $props();

	// Object URLs are created in an effect over the file list and revoked
	// whenever the list changes (or the component unmounts), so thumbnails
	// never leak blob URLs. Creating them in a $derived would be a side
	// effect inside a cached lazy value, so populate $state instead.
	let previews = $state<{ file: File; url: string }[]>([]);
	$effect(() => {
		const urls = files.map((file) => ({ file, url: URL.createObjectURL(file) }));
		previews = urls;
		return () => {
			for (const { url } of urls) URL.revokeObjectURL(url);
		};
	});

	function addFiles(list: FileList | null) {
		if (!list) return;
		const incoming = Array.from(list);
		if (files.length + incoming.length > MAX_GENERATE_IMAGES) {
			onerror(t('generateTooManyImages'));
			return;
		}
		for (const f of incoming) {
			const err = validateGenerateImage(f);
			if (err) {
				onerror(t(err));
				return;
			}
		}
		const totalErr = validateGenerateImageTotal([...files, ...incoming]);
		if (totalErr) {
			onerror(t(totalErr));
			return;
		}
		onerror(null);
		files = [...files, ...incoming];
	}

	function removeFile(index: number) {
		files = files.filter((_, i) => i !== index);
		onerror(null);
	}
</script>

{#if files.length > 0}
	<ul class="multi-image__list">
		{#each previews as { file, url }, i (file)}
			<li class="multi-image__item">
				<img src={url} alt="" />
				<button
					type="button"
					class="multi-image__remove btn btn--ghost"
					onclick={() => removeFile(i)}
					disabled={disabled}
					aria-label={t('fieldImageRemove')}
					title={t('fieldImageRemove')}
				>
					<Icon name="x" size={14} />
				</button>
			</li>
		{/each}
	</ul>
{/if}
<label class="multi-image__add">
	<input
		type="file"
		accept="image/*"
		multiple
		disabled={disabled || files.length >= MAX_GENERATE_IMAGES}
		onchange={(e) => {
			const input = e.target as HTMLInputElement;
			addFiles(input.files);
			input.value = '';
		}}
		class="multi-image__input"
	/>
	<span class="multi-image__add-icon"><Icon name="image" size={16} /></span>
	<span class="multi-image__add-label">{t('generateImagesLabel')}</span>
	{#if files.length > 0}
		<span class="multi-image__count">{files.length}/{MAX_GENERATE_IMAGES}</span>
	{/if}
</label>

<style>
	.multi-image__list {
		display: flex;
		flex-wrap: wrap;
		gap: var(--space-3);
		list-style: none;
		padding: 0;
		margin: 0 0 var(--space-3);
	}
	.multi-image__item {
		position: relative;
		width: 72px;
		height: 72px;
		border-radius: var(--radius-md);
		overflow: hidden;
	}
	.multi-image__item img {
		width: 100%;
		height: 100%;
		object-fit: cover;
		display: block;
	}
	.multi-image__remove {
		position: absolute;
		top: 2px;
		right: 2px;
		padding: 2px;
		line-height: 1;
	}
	.multi-image__add {
		display: flex;
		align-items: center;
		justify-content: center;
		gap: var(--space-2);
		padding: var(--space-3) var(--space-4);
		border: 1px dashed var(--color-border-strong);
		border-radius: var(--radius-md);
		background: var(--color-surface);
		color: var(--color-text-secondary);
		font-size: var(--text-sm);
		font-weight: var(--weight-medium);
		cursor: pointer;
		transition: background var(--transition-fast), border-color var(--transition-fast);
	}
	.multi-image__add:hover:not(:has(.multi-image__input:disabled)) {
		background: var(--color-surface-2);
		border-color: var(--color-primary);
	}
	.multi-image__add:has(.multi-image__input:disabled) {
		opacity: 0.55;
		cursor: not-allowed;
	}
	.multi-image__add:has(.multi-image__input:focus-visible) {
		outline: 2px solid var(--color-primary);
		outline-offset: 2px;
	}
	.multi-image__add-icon {
		flex-shrink: 0;
		display: flex;
		color: var(--color-primary);
	}
	.multi-image__count {
		font-size: var(--text-xs);
		color: var(--color-text-muted);
		margin-left: var(--space-1);
	}
	.multi-image__input {
		position: absolute;
		width: 1px;
		height: 1px;
		opacity: 0;
		overflow: hidden;
	}
</style>
