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
		// Date falls in previous year's last week
		return weekOfDate(d);
	}
	const diffMs = d.getTime() - mon.getTime();
	const diffDays = Math.floor(diffMs / 86400000);
	const week = Math.floor(diffDays / 7) + 1;
	return { year, week };
}

export function mondaySundayOf(year: number, week: number): { monday: Date; sunday: Date } {
	const mon = mondayOfJan1(year);
	const monday = new Date(mon.getTime() + (week - 1) * 7 * 86400000);
	const sunday = new Date(monday.getTime() + 6 * 86400000);
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
		const date = new Date(start.getTime() + i * 86400000);
		cells.push({
			date,
			inMonth: date.getUTCMonth() === month,
			week: weekOfDate(date),
		});
	}
	return cells;
}
