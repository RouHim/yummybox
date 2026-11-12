export function prefersReducedMotion(): boolean {
	if (typeof window === 'undefined') return false;
	return window.matchMedia('(prefers-reduced-motion: reduce)').matches;
}

export function isLowPowerDevice(): boolean {
	if (typeof window === 'undefined') return false;
	const coarse = window.matchMedia('(any-pointer: coarse)').matches;
	const mem = (navigator as Navigator & { deviceMemory?: number }).deviceMemory ?? Infinity;
	return coarse || mem <= 4;
}

export function tierDuration(ms: number): number {
	return prefersReducedMotion() ? 0 : (isLowPowerDevice() ? Math.round(ms * 0.5) : ms);
}

export function staggerDuration(i: number, base = 40, max = 200): number {
	return prefersReducedMotion() ? 0 : (isLowPowerDevice() ? 0 : Math.min(i * base, max));
}
