<script lang="ts">
	import '../app.css';
	import favicon from '$lib/assets/favicon.svg';
	import { initLocale, getLocale, t } from '$lib/i18n';
	import { initTheme, theme, cycleTheme } from '$lib/theme';
	import Icon from '$lib/Icon.svelte';
	import { isLowPowerDevice } from '$lib/motion';


	let { children } = $props();
	$effect(() => {
		initLocale();
		initTheme();
		document.documentElement.lang = getLocale();
	});

	$effect(() => {
		document.documentElement.classList.toggle('low-power', isLowPowerDevice());
		const mq = window.matchMedia('(any-pointer: coarse)');
		const handler = () => {
			document.documentElement.classList.toggle('low-power', isLowPowerDevice());
		};
		mq.addEventListener('change', handler);
		return () => mq.removeEventListener('change', handler);
	});
</script>

<svelte:head>
	<link rel="icon" href={favicon} />
</svelte:head>

<div class="app-ambient" aria-hidden="true"></div>
{@render children()}

<footer class="site-footer">
	<p class="attribution">
		{t('bgPhoto')}: <a href="https://www.pexels.com/photo/cooked-food-with-sesame-seeds-8481834/" target="_blank" rel="noopener">Sergey Meshkov</a> / Pexels
	</p>
	<p class="theme-toggle">
		<button
			onclick={() => cycleTheme()}
			aria-label={t('themeToggle')}
		>
			<Icon name={theme.current === 'dark' ? 'moon' : theme.current === 'light' ? 'sun' : 'monitor'} size={14} />
			<span>{theme.current === 'dark' ? t('themeDark') : theme.current === 'light' ? t('themeLight') : t('themeSystem')}</span>
		</button>
	</p>
</footer>
