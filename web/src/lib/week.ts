const MS_PER_DAY = 86400000;

export function mondayOfJan1(year: number): Date {
	const jan1 = new Date(Date.UTC(year, 0, 1, 12, 0, 0));
	const day = jan1.getUTCDay();
	const daysFromMon = day === 0 ? 6 : day - 1;
	return new Date(Date.UTC(year, 0, 1 - daysFromMon, 12, 0, 0));
}

export function weekOfDate(d: Date): { year: number; week: number } {
	const year = d.getUTCFullYear();
	const mon = mondayOfJan1(year);
	if (d < mon) {
		return weekOfDate(d);
	}
	const diffMs = d.getTime() - mon.getTime();
	const diffDays = Math.floor(diffMs / MS_PER_DAY);
	const week = Math.floor(diffDays / 7) + 1;
	return { year, week };
}

export function mondaySundayOf(year: number, week: number): { monday: Date; sunday: Date } {
	const mon = mondayOfJan1(year);
	const monday = new Date(mon.getTime() + (week - 1) * 7 * MS_PER_DAY);
	const sunday = new Date(monday.getTime() + 6 * MS_PER_DAY);
	return { monday, sunday };
}

export function weeksInYear(year: number): number {
	const dec31 = new Date(Date.UTC(year, 11, 31, 12, 0, 0));
	return weekOfDate(dec31).week;
}

export function isPastWeek(
	year: number,
	week: number,
	current: { year: number; week: number }
): boolean {
	if (year < current.year) return true;
	if (year > current.year) return false;
	return week < current.week;
}

export interface MonthCell {
	date: Date;
	inMonth: boolean;
	week: { year: number; week: number };
}

/** Returns a 42-cell month grid (6 rows x 7 columns) for the given year/month.
 *  Monday-start. Includes leading/trailing days from adjacent months. */
export function monthGrid(year: number, month: number): MonthCell[] {
	const firstOfMonth = new Date(Date.UTC(year, month, 1, 12, 0, 0));
	const dayOfWeek = firstOfMonth.getUTCDay();
	const daysFromMon = dayOfWeek === 0 ? 6 : dayOfWeek - 1;
	// Monday on or before the 1st
	const start = new Date(Date.UTC(year, month, 1 - daysFromMon, 12, 0, 0));
	const cells: MonthCell[] = [];
	for (let i = 0; i < 42; i++) {
		const date = new Date(start.getTime() + i * MS_PER_DAY);
		cells.push({
			date,
			inMonth: date.getUTCMonth() === month,
			week: weekOfDate(date),
		});
	}
	return cells;
}

export interface WeekRow {
	week: { year: number; week: number };
	cells: MonthCell[];
}

/** Group a 42-cell month grid into six Monday-started week rows. */
export function weekRows(cells: MonthCell[]): WeekRow[] {
	return Array.from({ length: 6 }, (_, row) => {
		const rowCells = cells.slice(row * 7, row * 7 + 7);
		return { week: rowCells[0].week, cells: rowCells };
	});
}

/** Display range of an ISO week, e.g. "Jun 15 to Jun 21". */
export function formatWeekRange(year: number, week: number): string {
	const { monday, sunday } = mondaySundayOf(year, week);
	const opts: Intl.DateTimeFormatOptions = { month: 'short', day: 'numeric' };
	return `${monday.toLocaleDateString(undefined, opts)} to ${sunday.toLocaleDateString(undefined, opts)}`;
}

/** Calendar-grid date as a local date string, for comparing against `Date#toDateString`. */
export function localDateString(date: Date): string {
	return new Date(date.getUTCFullYear(), date.getUTCMonth(), date.getUTCDate()).toDateString();
}
