import { describe, it, expect } from 'vitest';
import { CalendarState } from './calendar.svelte';

describe('CalendarState', () => {
	it('pages back from January into December of the previous year', () => {
		const calendar = new CalendarState();
		calendar.showMonthOf(2026, 0);

		calendar.prevMonth();

		expect(calendar.year).toBe(2025);
		expect(calendar.month).toBe(11);
	});

	it('pages forward from December into January of the next year', () => {
		const calendar = new CalendarState();
		calendar.showMonthOf(2026, 11);

		calendar.nextMonth();

		expect(calendar.year).toBe(2027);
		expect(calendar.month).toBe(0);
	});

	it('derives a six-week grid for the viewed month', () => {
		const calendar = new CalendarState();
		calendar.showMonthOf(2026, 0);

		expect(calendar.grid).toHaveLength(42);
		expect(calendar.rows).toHaveLength(6);
		expect(calendar.rows[0].cells[0].date.toISOString().slice(0, 10)).toBe('2025-12-29');
	});

	it('shows the month containing today', () => {
		const calendar = new CalendarState();
		calendar.showMonthOf(1999, 5);
		const now = new Date();

		calendar.showToday();

		expect(calendar.year).toBe(now.getFullYear());
		expect(calendar.month).toBe(now.getMonth());
	});
});
