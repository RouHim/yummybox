<script lang="ts">
	import { t } from '$lib/i18n';
</script>

<p class="ai-config-notice" role="status">
	{t('aiConfigNotice')}
	<a href="/settings">{t('settingsLinkLabel')}</a>
</p>

<style>
	.ai-config-notice {
		display: flex;
		flex-wrap: wrap;
		gap: var(--space-2);
		align-items: baseline;
		padding: var(--space-3) var(--space-4);
		background: var(--color-surface-2);
		border: 1px solid var(--color-border);
		border-radius: var(--radius-md);
		font-size: var(--text-sm);
		color: var(--color-text-secondary);
	}
	.ai-config-notice a {
		color: var(--color-primary);
		font-weight: var(--weight-medium);
	}
</style>
