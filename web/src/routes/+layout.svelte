<script lang="ts">
	import '../app.css';
	import favicon from '$lib/assets/favicon.svg';
	import { initLocale, getLocale, t } from '$lib/i18n';
	import { initTheme, theme, cycleTheme } from '$lib/theme';
	import { page } from '$app/state';
	import Icon from '$lib/Icon.svelte';
	import { isLowPowerDevice } from '$lib/motion';
	import { checkBringStatus } from '$lib/api';


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

	type BringBadgeState = 'hidden' | 'checking' | 'connected' | 'error';
	let bringState = $state<BringBadgeState>('hidden');
	let bringError = $state<string | null>(null);

	$effect(() => {
		checkBringStatus()
			.then((res) => {
				if (!res.configured) {
					console.log('[Bring!] not configured — set BRING_EMAIL and BRING_PASSWORD to enable shopping list sync');
					bringState = 'hidden';
				} else if (res.connected) {
					console.log('[Bring!] connected');
					bringState = 'connected';
				} else {
					console.warn('[Bring!] error:', res.error);
					bringState = 'error';
					bringError = res.error;
				}
			})
			.catch((e) => {
				console.error('[Bring!] probe failed:', e);
				bringState = 'error';
				bringError = e instanceof Error ? e.message : String(e);
			});
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
			<Icon name="utensils" size={16} /> {t('navMeals')}
		</a>
		<a href="/planner" class="app-bar__link"
			aria-current={pathname === '/planner' ? 'page' : undefined}>
			<Icon name="calendar" size={16} /> {t('navPlanner')}
		</a>
	</nav>
	<div class="app-bar__actions">
		{#if bringState !== 'hidden'}
			<button
				class="app-bar__bring"
				class:app-bar__bring--connected={bringState === 'connected'}
				class:app-bar__bring--error={bringState === 'error'}
				type="button"
				aria-label={bringState === 'checking' ? t('bringStatusChecking') : bringState === 'connected' ? t('bringStatusConnected') : t('bringStatusError')}
				title={bringState === 'error' && bringError ? bringError : undefined}
				disabled={bringState === 'checking'}
			>
				<Icon name={bringState === 'checking' ? 'loader-circle' : bringState === 'connected' ? 'check' : 'circle-alert'} size={16} />
			</button>
			{#if bringState === 'error' && bringError}
				<span class="app-bar__bring-error" role="alert">{bringError}</span>
			{/if}
		{/if}
		<button class="app-bar__theme" type="button"
			onclick={cycleTheme}
			aria-label={t('themeToggle')}
			title={`${t('themeToggle')}: ${theme.current === 'dark' ? t('themeDark') : theme.current === 'light' ? t('themeLight') : t('themeSystem')}`}>
			<Icon name={theme.current === 'dark' ? 'moon' : theme.current === 'light' ? 'sun' : 'monitor'} size={16} />
		</button>
	</div>
</header>

{@render children()}

<footer class="site-footer glass">
	<p class="attribution">
		{t('bgPhoto')}: <a href="https://www.pexels.com/photo/cooked-food-with-sesame-seeds-8481834/" target="_blank" rel="noopener">Sergey Meshkov</a> / Pexels
	</p>
</footer>
