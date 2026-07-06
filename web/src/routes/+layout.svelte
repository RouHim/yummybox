<script lang="ts">
	import '../app.css';
	import favicon from '$lib/assets/soup.svg';
	import { initLocale, getLocale, t } from '$lib/i18n';
	import { initTheme, theme, cycleTheme } from '$lib/theme';
	import { page } from '$app/state';
	import Icon from '$lib/Icon.svelte';
	import LanguageSwitcher from '$lib/LanguageSwitcher.svelte';
	import { isLowPowerDevice } from '$lib/motion';
	import { checkBringStatus, getVersion } from '$lib/api';


	let { children } = $props();
	const pathname = $derived(page.url.pathname);
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

	let bringError = $state<string | null>(null);

	let appVersion = $state<string | null>(null);

	$effect(() => {
		checkBringStatus()
			.then((res) => {
				if (!res.configured) {
					console.log('[Bring!] not configured — set BRING_EMAIL and BRING_PASSWORD to enable shopping list sync');
					bringError = null;
				} else if (res.connected) {
					console.log('[Bring!] connected');
					bringError = null;
				} else {
					console.warn('[Bring!] error:', res.error);
					bringError = res.error;
				}
			})
			.catch((e) => {
				console.error('[Bring!] probe failed:', e);
				bringError = e instanceof Error ? e.message : String(e);
			});
	});

	$effect(() => {
		getVersion()
			.then((res) => { appVersion = res.version; })
			.catch(() => { appVersion = null; });
	});
</script>

<svelte:head>
	<title>{t('appTitle')}</title>
	<link rel="icon" href={favicon} />
</svelte:head>

<div class="app-ambient" aria-hidden="true"></div>

<header class="app-bar glass">
	<a href="/" class="app-bar__brand" aria-label={t('navHome')}
		aria-current={pathname === '/' ? 'page' : undefined}>
		<Icon name="soup" size={24} />
	</a>
	<nav class="app-bar__nav" aria-label={t('appTitle')}>
		<a href="/meals" class="app-bar__link"
			aria-current={pathname.startsWith('/meals') ? 'page' : undefined}>
			<Icon name="utensils" size={16} />
			<span class="app-bar__link-label">{t('navMeals')}</span>
		</a>
		<a href="/planner" class="app-bar__link"
			aria-current={pathname === '/planner' ? 'page' : undefined}>
			<Icon name="calendar" size={16} />
			<span class="app-bar__link-label">{t('navPlanner')}</span>
		</a>
	</nav>
	<div class="app-bar__actions">
		<button class="app-bar__theme" type="button"
			onclick={cycleTheme}
			aria-label={t('themeToggle')}
			title={`${t('themeToggle')}: ${theme.current === 'dark' ? t('themeDark') : theme.current === 'light' ? t('themeLight') : t('themeSystem')}`}>
			<Icon name={theme.current === 'dark' ? 'moon' : theme.current === 'light' ? 'sun' : 'monitor'} size={16} />
		</button>
		<LanguageSwitcher />
	</div>
</header>

{@render children()}

<footer class="site-footer glass">
	<p class="attribution">
		{t('bgPhoto')}: <a href="https://www.pexels.com/photo/cooked-food-with-sesame-seeds-8481834/" target="_blank" rel="noopener">Sergey Meshkov</a> / Pexels
	</p>
	{#if bringError}
		<span class="site-footer__bring-error" role="alert">{bringError}</span>
	{/if}
	{#if appVersion}
		<a href="https://github.com/RouHim/yummybox/releases" target="_blank" rel="noopener">{appVersion}</a>
	{/if}
</footer>
