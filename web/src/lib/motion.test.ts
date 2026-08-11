import { describe, it, expect, vi, afterEach } from 'vitest';
import { prefersReducedMotion, transitionDuration, isLowPowerDevice, tierDuration, staggerDuration } from './motion';

afterEach(() => {
	vi.unstubAllGlobals();
});

function stubWindow(matchMediaResult: boolean): void {
	vi.stubGlobal('window', {
		matchMedia: (query: string) => ({ matches: matchMediaResult, media: query }),
	});
}

describe('prefersReducedMotion', () => {
	it('returns false when window is undefined', () => {
		expect(prefersReducedMotion()).toBe(false);
	});

	it('returns true when the user prefers reduced motion', () => {
		stubWindow(true);
		expect(prefersReducedMotion()).toBe(true);
	});

	it('returns false when the user does not prefer reduced motion', () => {
		stubWindow(false);
		expect(prefersReducedMotion()).toBe(false);
	});
});

describe('transitionDuration', () => {
	it('returns the given duration when motion is not reduced', () => {
		stubWindow(false);
		expect(transitionDuration(250)).toBe(250);
	});

	it('returns 0 when motion is reduced', () => {
		stubWindow(true);
		expect(transitionDuration(250)).toBe(0);
	});
});

describe('isLowPowerDevice', () => {
	it('returns false when window is undefined', () => {
		expect(isLowPowerDevice()).toBe(false);
	});

	it('returns true on a coarse-pointer device', () => {
		stubWindow(true);
		expect(isLowPowerDevice()).toBe(true);
	});

	it('returns false on a fine-pointer device without deviceMemory', () => {
		stubWindow(false);
		vi.stubGlobal('navigator', {});
		expect(isLowPowerDevice()).toBe(false);
	});

	it('returns true when deviceMemory is 4 or less', () => {
		stubWindow(false);
		vi.stubGlobal('navigator', { deviceMemory: 4 });
		expect(isLowPowerDevice()).toBe(true);
	});

	it('returns false when deviceMemory is above 4', () => {
		stubWindow(false);
		vi.stubGlobal('navigator', { deviceMemory: 8 });
		expect(isLowPowerDevice()).toBe(false);
	});
});

describe('tierDuration', () => {
	it('returns 0 when motion is reduced', () => {
		stubWindow(true);
		expect(tierDuration(300)).toBe(0);
	});

	it('halves and rounds the duration on a low-power device', () => {
		stubWindow(false);
		vi.stubGlobal('navigator', { deviceMemory: 2 });
		expect(tierDuration(301)).toBe(151);
	});

	it('returns the full duration on a normal device', () => {
		stubWindow(false);
		vi.stubGlobal('navigator', { deviceMemory: 8 });
		expect(tierDuration(300)).toBe(300);
	});
});

describe('staggerDuration', () => {
	it('returns 0 when motion is reduced', () => {
		stubWindow(true);
		expect(staggerDuration(3)).toBe(0);
	});

	it('returns 0 on a low-power device', () => {
		stubWindow(false);
		vi.stubGlobal('navigator', { deviceMemory: 2 });
		expect(staggerDuration(3)).toBe(0);
	});

	it('scales by index up to the maximum', () => {
		stubWindow(false);
		vi.stubGlobal('navigator', { deviceMemory: 8 });
		expect(staggerDuration(2)).toBe(80);
		expect(staggerDuration(10)).toBe(200);
	});

	it('uses the provided base and max', () => {
		stubWindow(false);
		vi.stubGlobal('navigator', { deviceMemory: 8 });
		expect(staggerDuration(3, 50, 500)).toBe(150);
		expect(staggerDuration(20, 50, 500)).toBe(500);
	});
});
