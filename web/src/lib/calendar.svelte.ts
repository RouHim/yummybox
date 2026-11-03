import { monthGrid, weekRows, type MonthCell, type WeekRow } from './week';

/**
 * Calendar view state: the viewed month, its 42-cell Monday-started grid and
 * the six week rows.
 *
 * Month paging wraps across year boundaries here, so no page re-implements the
 * December→January rollover or re-derives the grid rows.
 */
export class CalendarState {
	/** Viewed year. */
	year = $state(new Date().getFullYear());
	/** Viewed month, 0-indexed. */
	month = $state(new Date().getMonth());

	grid = $derived<MonthCell[]>(monthGrid(this.year, this.month));
	rows = $derived<WeekRow[]>(weekRows(this.grid));

	/** Show the month containing the given date. */
	showMonthOf(year: number, month: number): void {
		this.year = year;
		this.month = month;
	}

	prevMonth(): void {
		if (this.month === 0) {
			this.month = 11;
			this.year--;
		} else {
			this.month--;
		}
	}

	nextMonth(): void {
		if (this.month === 11) {
			this.month = 0;
			this.year++;
		} else {
			this.month++;
		}
	}

	showToday(): void {
		const now = new Date();
		this.year = now.getFullYear();
		this.month = now.getMonth();
	}
}
