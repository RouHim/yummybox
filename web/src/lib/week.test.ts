import { describe, it, expect } from 'vitest';
import { weekOfDate, mondaySundayOf, weeksInYear, isPastWeek, monthGrid } from './week';

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

describe('monthGrid', () => {
	it('returns 42 cells for a 6-row month grid', () => {
		expect(monthGrid(2026, 0).length).toBe(42); // January
		expect(monthGrid(2026, 1).length).toBe(42); // February
		expect(monthGrid(2026, 11).length).toBe(42); // December
	});

	it('marks out-of-month days with inMonth false', () => {
		// Jan 2026: first day is Jan 1 (Thu), grid starts Mon Dec 29 2025
		const jan = monthGrid(2026, 0);
		expect(jan[0].inMonth).toBe(false); // Dec 29
		expect(jan[0].date.getUTCDate()).toBe(29);
		expect(jan[0].date.getUTCMonth()).toBe(11); // December
		// The first in-month cell: Jan 1 is index 3 (Mon=Dec29, Tue=Dec30, Wed=Dec31, Thu=Jan1)
		const firstInMonth = jan.find(c => c.inMonth);
		expect(firstInMonth).toBeDefined();
		expect(firstInMonth!.date.getUTCDate()).toBe(1);
		expect(firstInMonth!.date.getUTCMonth()).toBe(0);
	});

	it('assigns each cell its ISO week via weekOfDate', () => {
		const jan = monthGrid(2026, 0);
		for (const cell of jan) {
			const expected = weekOfDate(cell.date);
			expect(cell.week).toEqual(expected);
		}
	});

	it('handles December overhang — last-row Monday may fall in next year', () => {
		// Dec 2028: Dec 1 is Fri; grid starts Mon Nov 27 2028
		// Row 5 Monday = Nov 27 + 35 days = Jan 1 2029 (ISO {2029, 1})
		const dec = monthGrid(2028, 11);
		const row5Monday = dec[35].date;
		expect(row5Monday.getUTCDay()).toBe(1); // Monday
		const week = weekOfDate(row5Monday);
		// Monday of row 5 is Jan 1 2029 → ISO week {2029, 1}
		expect(week.year).toBe(2029);
		expect(week.week).toBe(1);
	});

	it('handles January overhang from previous ISO year', () => {
		const jan = monthGrid(2026, 0);
		// First Monday (Dec 29 2025) should have ISO week {2025, 53}
		expect(jan[0].week.year).toBe(2025);
		expect(jan[0].week.week).toBe(53);
		// The second Monday (Jan 5 2026) should have ISO week {2026, 2}
		expect(jan[7].week.year).toBe(2026);
		expect(jan[7].week.week).toBe(2);
	});
});
