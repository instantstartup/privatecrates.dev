/** Dates, times and lists, in British English, always UTC. */

// Formatted by hand rather than with Intl, so the text is the same in workerd, Node and every ICU version.
const MONTHS = ['Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun', 'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec'];

/** "27 Sep 2026". */
export function formatDay(isoOrDay: string): string {
	const d = new Date(isoOrDay.length === 10 ? `${isoOrDay}T00:00:00Z` : isoOrDay);
	return `${d.getUTCDate()} ${MONTHS[d.getUTCMonth()]} ${d.getUTCFullYear()}`;
}

/** "14:05 UTC". */
export function formatTime(iso: string): string {
	const d = new Date(iso);
	const pad = (n: number) => String(n).padStart(2, '0');
	return `${pad(d.getUTCHours())}:${pad(d.getUTCMinutes())} UTC`;
}

export function formatDateTime(iso: string): string {
	return `${formatDay(iso)}, ${formatTime(iso)}`;
}

/** "a", "a and b", "a, b and c". */
export function joinNames(names: readonly string[]): string {
	if (names.length <= 1) return names.join('');
	return `${names.slice(0, -1).join(', ')} and ${names[names.length - 1]}`;
}

export function formatMinutes(n: number): string {
	if (n < 60) return `${n} minute${n === 1 ? '' : 's'}`;
	const h = Math.floor(n / 60);
	const m = n % 60;
	return `${h} hour${h === 1 ? '' : 's'}${m ? ` ${m} minute${m === 1 ? '' : 's'}` : ''}`;
}

export function formatPercent(share: number): string {
	const pct = share * 100;
	if (pct === 100) return '100%';
	// Never round a bad day up to 100%.
	return `${(Math.floor(pct * 100) / 100).toFixed(2)}%`;
}

export function formatAgo(fromIso: string, nowMs: number): string {
	const minutes = Math.max(0, Math.round((nowMs - Date.parse(fromIso)) / 60_000));
	if (minutes === 0) return 'less than a minute ago';
	return `${formatMinutes(minutes)} ago`;
}
