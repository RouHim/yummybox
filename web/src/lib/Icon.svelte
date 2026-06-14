<script lang="ts">
	type IconName = 'search' | 'empty-meals' | 'alert' | 'plate';
	let { name, class: className = '', size = 24 }: { name: IconName; class?: string; size?: number } = $props();
</script>

{#if name === 'search'}
	<svg xmlns="http://www.w3.org/2000/svg" width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class={className} aria-hidden="true">
		<circle cx="11" cy="11" r="7"></circle>
		<line x1="21" y1="21" x2="16.65" y2="16.65"></line>
	</svg>
{:else if name === 'empty-meals'}
	<svg xmlns="http://www.w3.org/2000/svg" width={size} height={size} viewBox="0 0 96 96" fill="none" stroke="currentColor" stroke-width="3" stroke-linecap="round" stroke-linejoin="round" class={className} aria-hidden="true">
		<!-- Plate with fork & knife -->
		<circle cx="48" cy="48" r="28"></circle>
		<circle cx="48" cy="48" r="20" stroke-dasharray="2 4" opacity="0.4"></circle>
		<line x1="14" y1="20" x2="14" y2="76"></line>
		<line x1="82" y1="20" x2="82" y2="76"></line>
		<line x1="11" y1="20" x2="17" y2="20"></line>
		<line x1="11" y1="76" x2="17" y2="76"></line>
		<line x1="79" y1="20" x2="85" y2="20"></line>
		<line x1="79" y1="76" x2="85" y2="76"></line>
	</svg>
{:else if name === 'alert'}
	<svg xmlns="http://www.w3.org/2000/svg" width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class={className} aria-hidden="true">
		<circle cx="12" cy="12" r="10"></circle>
		<line x1="12" y1="8" x2="12" y2="12"></line>
		<line x1="12" y1="16" x2="12.01" y2="16"></line>
	</svg>
{/if}
