<script lang="ts">
	type IconName = 'search' | 'empty-meals' | 'circle-alert' | 'calendar' | 'chevron-left' | 'chevron-right' | 'chevron-down' | 'plus' | 'trash-2' | 'x' | 'check' | 'pen-line' | 'utensils' | 'sun' | 'moon' | 'monitor' | 'soup' | 'loader-circle' | 'shopping-bag' | 'image' | 'link' | 'archive' | 'sparkles' | 'layers' | 'download' | 'ellipsis-vertical' | 'upload' | 'clipboard' | 'image-down';
	let { name, class: className = '', size = 24, spin = false }: { name: IconName; class?: string; size?: number; spin?: boolean } = $props();

	let iconClass = $derived(spin ? `${className} icon-spin`.trim() : className);
	import soupSvgRaw from '$lib/assets/soup.svg?raw';
</script>

{#if name === 'search'}
	<svg xmlns="http://www.w3.org/2000/svg" width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class={iconClass} aria-hidden="true">
		<path d="m21 21-4.34-4.34" />
		<circle cx="11" cy="11" r="8" />
	</svg>
{:else if name === 'empty-meals'}
	<svg xmlns="http://www.w3.org/2000/svg" width={size} height={size} viewBox="0 0 96 96" fill="none" stroke="currentColor" stroke-width="3" stroke-linecap="round" stroke-linejoin="round" class={iconClass} aria-hidden="true">
		<circle cx="48" cy="48" r="28" />
		<circle cx="48" cy="48" r="20" stroke-dasharray="2 4" opacity="0.4" />
		<line x1="14" y1="20" x2="14" y2="76" />
		<line x1="82" y1="20" x2="82" y2="76" />
		<line x1="11" y1="20" x2="17" y2="20" />
		<line x1="11" y1="76" x2="17" y2="76" />
		<line x1="79" y1="20" x2="85" y2="20" />
		<line x1="79" y1="76" x2="85" y2="76" />
	</svg>
{:else if name === 'circle-alert'}
	<svg xmlns="http://www.w3.org/2000/svg" width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class={iconClass} aria-hidden="true">
		<circle cx="12" cy="12" r="10" />
		<line x1="12" x2="12" y1="8" y2="12" />
		<line x1="12" x2="12.01" y1="16" y2="16" />
	</svg>
{:else if name === 'calendar'}
	<svg xmlns="http://www.w3.org/2000/svg" width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class={iconClass} aria-hidden="true">
		<path d="M8 2v4" />
		<path d="M16 2v4" />
		<rect width="18" height="18" x="3" y="4" rx="2" />
		<path d="M3 10h18" />
	</svg>
{:else if name === 'chevron-left'}
	<svg xmlns="http://www.w3.org/2000/svg" width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class={iconClass} aria-hidden="true">
		<path d="m15 18-6-6 6-6" />
	</svg>
{:else if name === 'chevron-right'}
	<svg xmlns="http://www.w3.org/2000/svg" width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class={iconClass} aria-hidden="true">
		<path d="m9 18 6-6-6-6" />
	</svg>
{:else if name === 'chevron-down'}
	<svg xmlns="http://www.w3.org/2000/svg" width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class={iconClass} aria-hidden="true">
		<path d="m6 9 6 6 6-6" />
	</svg>
{:else if name === 'plus'}
	<svg xmlns="http://www.w3.org/2000/svg" width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class={iconClass} aria-hidden="true">
		<path d="M5 12h14" />
		<path d="M12 5v14" />
	</svg>
{:else if name === 'trash-2'}
	<svg xmlns="http://www.w3.org/2000/svg" width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class={iconClass} aria-hidden="true">
		<path d="M10 11v6" />
		<path d="M14 11v6" />
		<path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6" />
		<path d="M3 6h18" />
		<path d="M8 6V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2" />
	</svg>
{:else if name === 'x'}
	<svg xmlns="http://www.w3.org/2000/svg" width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class={iconClass} aria-hidden="true">
		<path d="M18 6 6 18" />
		<path d="m6 6 12 12" />
	</svg>
{:else if name === 'check'}
	<svg xmlns="http://www.w3.org/2000/svg" width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class={iconClass} aria-hidden="true">
		<path d="M20 6 9 17l-5-5" />
	</svg>
{:else if name === 'pen-line'}
	<svg xmlns="http://www.w3.org/2000/svg" width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class={iconClass} aria-hidden="true">
		<path d="M13 21h8" />
		<path d="M21.174 6.812a1 1 0 0 0-3.986-3.987L3.842 16.174a2 2 0 0 0-.5.83l-1.321 4.352a.5.5 0 0 0 .623.622l4.353-1.32a2 2 0 0 0 .83-.497z" />
	</svg>
{:else if name === 'utensils'}
	<svg xmlns="http://www.w3.org/2000/svg" width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class={iconClass} aria-hidden="true">
		<path d="M3 2v7c0 1.1.9 2 2 2h4a2 2 0 0 0 2-2V2" />
		<path d="M7 2v20" />
		<path d="M21 15V2a5 5 0 0 0-5 5v6c0 1.1.9 2 2 2h3Zm0 0v7" />
	</svg>
{:else if name === 'sun'}
	<svg xmlns="http://www.w3.org/2000/svg" width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class={iconClass} aria-hidden="true">
		<circle cx="12" cy="12" r="4" />
		<path d="M12 2v2" />
		<path d="M12 20v2" />
		<path d="m4.93 4.93 1.41 1.41" />
		<path d="m17.66 17.66 1.41 1.41" />
		<path d="M2 12h2" />
		<path d="M20 12h2" />
		<path d="m6.34 17.66-1.41 1.41" />
		<path d="m19.07 4.93-1.41 1.41" />
	</svg>
{:else if name === 'moon'}
	<svg xmlns="http://www.w3.org/2000/svg" width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class={iconClass} aria-hidden="true">
		<path d="M12 3a6 6 0 0 0 9 9 9 9 0 1 1-9-9Z" />
	</svg>
{:else if name === 'monitor'}
	<svg xmlns="http://www.w3.org/2000/svg" width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class={iconClass} aria-hidden="true">
		<rect width="20" height="14" x="2" y="3" rx="2" />
		<line x1="8" x2="16" y1="21" y2="21" />
		<line x1="12" x2="12" y1="17" y2="21" />
	</svg>
{:else if name === 'soup'}
	<!-- eslint-disable-next-line svelte/no-at-html-tags -- trusted local asset, build-time inlined -->
	{@html soupSvgRaw}
{:else if name === 'loader-circle'}
	<svg xmlns="http://www.w3.org/2000/svg" width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class={iconClass} aria-hidden="true">
		<path d="M21 12a9 9 0 1 1-6.219-8.56" />
	</svg>
{:else if name === 'shopping-bag'}
	<svg xmlns="http://www.w3.org/2000/svg" width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class={iconClass} aria-hidden="true">
		<path d="M6 2 3 6v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2V6l-3-4Z" />
		<path d="M3 6h18" />
		<path d="M16 10a4 4 0 0 1-8 0" />
	</svg>
{:else if name === 'image'}
	<svg xmlns="http://www.w3.org/2000/svg" width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class={iconClass} aria-hidden="true">
		<rect width="18" height="18" x="3" y="3" rx="2" ry="2" />
		<circle cx="9" cy="9" r="2" />
		<path d="m21 15-3.086-3.086a2 2 0 0 0-2.828 0L6 21" />
	</svg>
{:else if name === 'link'}
	<svg xmlns="http://www.w3.org/2000/svg" width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class={iconClass} aria-hidden="true">
		<path d="M10 13a5 5 0 0 0 7.54.54l3-3a5 5 0 0 0-7.07-7.07l-1.72 1.71" />
		<path d="M14 11a5 5 0 0 0-7.54-.54l-3 3a5 5 0 0 0 7.07 7.07l1.71-1.71" />
	</svg>
{:else if name === 'archive'}
	<svg xmlns="http://www.w3.org/2000/svg" width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class={iconClass} aria-hidden="true">
		<rect width="20" height="5" x="2" y="3" rx="1" />
		<path d="M4 8v11a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8" />
		<path d="M10 12h4" />
	</svg>
{:else if name === 'sparkles'}
	<svg xmlns="http://www.w3.org/2000/svg" width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class={iconClass} aria-hidden="true">
		<path d="M9.937 15.5A2 2 0 0 0 8.5 14.063l-6.135-1.582a.5.5 0 0 1 0-.962L8.5 9.936A2 2 0 0 0 9.937 8.5l1.582-6.135a.5.5 0 0 1 .963 0L14.063 8.5A2 2 0 0 0 15.5 9.937l6.135 1.581a.5.5 0 0 1 0 .964L15.5 14.063a2 2 0 0 0-1.437 1.437l-1.582 6.135a.5.5 0 0 1-.963 0z" />
		<path d="M20 3v4" />
		<path d="M22 5h-4" />
		<path d="M4 17v2" />
		<path d="M5 18H3" />
	</svg>
{:else if name === 'layers'}
	<svg xmlns="http://www.w3.org/2000/svg" width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class={iconClass} aria-hidden="true">
		<path d="M12.83 2.18a2 2 0 0 0-1.66 0L2.6 6.08a1 1 0 0 0 0 1.83l8.58 3.91a2 2 0 0 0 1.66 0l8.58-3.9a1 1 0 0 0 0-1.83Z" />
		<path d="m22 17.65-9.17 4.16a2 2 0 0 1-1.66 0L2 17.65" />
		<path d="m22 12.65-9.17 4.16a2 2 0 0 1-1.66 0L2 12.65" />
	</svg>
{:else if name === 'download'}
	<svg xmlns="http://www.w3.org/2000/svg" width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class={iconClass} aria-hidden="true">
		<path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4" />
		<polyline points="7 10 12 15 17 10" />
		<line x1="12" x2="12" y1="15" y2="3" />
	</svg>
{:else if name === 'ellipsis-vertical'}
	<svg xmlns="http://www.w3.org/2000/svg" width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class={iconClass} aria-hidden="true">
		<circle cx="12" cy="5" r="1" />
		<circle cx="12" cy="12" r="1" />
		<circle cx="12" cy="19" r="1" />
	</svg>
{:else if name === 'upload'}
	<svg xmlns="http://www.w3.org/2000/svg" width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class={iconClass} aria-hidden="true">
		<path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4" />
		<polyline points="17 8 12 3 7 8" />
		<line x1="12" x2="12" y1="3" y2="15" />
	</svg>
{:else if name === 'clipboard'}
	<svg xmlns="http://www.w3.org/2000/svg" width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class={iconClass} aria-hidden="true">
		<path d="M16 4h2a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2h2" />
		<rect x="8" y="2" width="8" height="4" rx="1" ry="1" />
	</svg>
{:else if name === 'image-down'}
	<svg xmlns="http://www.w3.org/2000/svg" width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class={iconClass} aria-hidden="true">
		<rect x="3" y="3" width="18" height="18" rx="2" ry="2" />
		<circle cx="8.5" cy="8.5" r="1.5" />
		<polyline points="21 15 16 10 5 21" />
		<line x1="12" y1="12" x2="12" y2="18" />
		<polyline points="9 15 12 18 15 15" />
	</svg>
{/if}

<style>
	svg {
		vertical-align: middle;
	}
	.icon-spin {
		animation: icon-spin 1s linear infinite;
	}
	@keyframes icon-spin {
		from { transform: rotate(0deg); }
		to { transform: rotate(360deg); }
	}

	@media (prefers-reduced-motion: reduce) {
		.icon-spin {
			animation: none;
		}
	}
</style>
