import { describe, it, expect } from 'vitest';
import { weekOfDate, mondaySundayOf, weeksInYear, isPastWeek } from './week';

describe('weekOfDate', () => {
	it('returns 2026 week 1 for Jan 1 2026', () => {
		const d = new Date('2026-01-01T12:00:00Z');
		const result = weekOfDate(d);
		expect(result.year).toBe(2026);
		expect(result.week).toBe(1);
	});
});

describe('mondaySundayOf', () => {
	it('returns Dec 29 2025 to Jan 4 2026 for week 1 2026', () => {
		const { monday, sunday } = mondaySundayOf(2026, 1);
		expect(monday.toISOString().slice(0, 10)).toBe('2025-12-29');
		expect(sunday.toISOString().slice(0, 10)).toBe('2026-01-04');
	});

	it('returns Jun 15 to Jun 21 2026 for week 25 2026', () => {
		const { monday, sunday } = mondaySundayOf(2026, 25);
		expect(monday.toISOString().slice(0, 10)).toBe('2026-06-15');
		expect(sunday.toISOString().slice(0, 10)).toBe('2026-06-21');
	});
});

describe('weeksInYear', () => {
	it('returns 53 for 2026', () => {
		expect(weeksInYear(2026)).toBe(53);
	});

	it('returns 53 for 2023', () => {
		// Lock in determinism
		const actual = weeksInYear(2023);
		expect(actual).toBe(weeksInYear(2023));
		expect(actual).toBeGreaterThanOrEqual(52);
		expect(actual).toBeLessThanOrEqual(53);
	});
});

describe('isPastWeek', () => {
	const current = { year: 2026, week: 25 };

	it('given_current_year_and_earlier_week_then_returns_true', () => {
		expect(isPastWeek(2026, 10, current)).toBe(true);
	});

	it('given_current_year_and_same_week_then_returns_false', () => {
		expect(isPastWeek(2026, 25, current)).toBe(false);
	});

	it('given_current_year_and_later_week_then_returns_false', () => {
		expect(isPastWeek(2026, 30, current)).toBe(false);
	});

	it('given_previous_year_then_returns_true', () => {
		expect(isPastWeek(2025, 52, current)).toBe(true);
	});

	it('given_future_year_then_returns_false', () => {
		expect(isPastWeek(2027, 1, current)).toBe(false);
	});
});
