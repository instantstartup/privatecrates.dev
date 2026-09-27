import { hasSubscription, planOf, type Org } from '$lib/api';
import { FREE_MEMBER_LIMIT, TRIAL_REMINDER_DAYS } from '$lib/site';

const dateFormat = new Intl.DateTimeFormat('en-GB', { day: 'numeric', month: 'long', year: 'numeric' });

export function formatDate(iso: string | null | undefined): string | null {
	if (!iso) return null;
	const d = new Date(iso);
	return Number.isNaN(d.getTime()) ? null : dateFormat.format(d);
}

const dateTimeFormat = new Intl.DateTimeFormat('en-GB', {
	day: 'numeric',
	month: 'short',
	year: 'numeric',
	hour: '2-digit',
	minute: '2-digit',
	timeZone: 'UTC'
});

/** "27 Sept 2026, 14:20", in UTC: audit logs are compared across time zones. */
export function formatDateTime(iso: string | null | undefined): string | null {
	if (!iso) return null;
	const d = new Date(iso);
	return Number.isNaN(d.getTime()) ? null : dateTimeFormat.format(d);
}

export function addDays(iso: string, days: number): string {
	return new Date(new Date(iso).getTime() + days * 86_400_000).toISOString();
}

/** Whole days until `iso`, rounded up; 0 once it has passed. */
export function daysUntil(iso: string | null | undefined): number | null {
	if (!iso) return null;
	const t = new Date(iso).getTime();
	if (Number.isNaN(t)) return null;
	return Math.max(0, Math.ceil((t - Date.now()) / 86_400_000));
}

export function plural(n: number, one: string, many = `${one}s`): string {
	return `${n} ${n === 1 ? one : many}`;
}

export type Badge = { label: string; tone: 'ok' | 'warn' | 'danger' | 'idle' };

/** The plan badge in the organisation list. */
export function planBadge(org: Org): Badge {
	switch (planOf(org)) {
		case 'free':
			return { label: 'Free', tone: 'ok' };
		case 'trial': {
			const days = daysUntil(org.trial_ends_at ?? org.tenant?.trial_ends_at);
			return {
				label: days === null ? 'Trial' : `Trial, ${plural(days, 'day')} left`,
				tone: days !== null && days <= TRIAL_REMINDER_DAYS && !org.has_payment_method ? 'warn' : 'ok'
			};
		}
		case 'paid':
			return { label: 'Paid', tone: 'ok' };
		case 'past_due':
			return { label: 'Payment overdue', tone: 'warn' };
		case 'inactive':
			if (org.trial_available) return { label: 'Trial not started', tone: 'idle' };
			return hasSubscription(org)
				? { label: 'Inactive', tone: 'danger' }
				: { label: 'No plan', tone: 'idle' };
	}
}

/** The registry's state in the organisation list; `live` is the page's verdict (see isLive on /account). */
export function setupLabel(org: Org, live: boolean): string {
	if (live) return 'Live';
	if (!org.tenant) return 'Not set up';
	return org.tenant.status !== null ? 'Paused' : 'Needs a plan';
}

/** "3 of 5 members", "12 members", or null when the count is not known. */
export function memberCount(org: Org): string | null {
	const n = org.members;
	if (n === null || n === undefined) return null;
	const limit = org.free_member_limit ?? FREE_MEMBER_LIMIT;
	return n <= limit ? `${n} of ${limit} members` : plural(n, 'member');
}

/** The apex host the account page is served from, used to preview a registry's hostname. */
export function baseDomain(): string {
	const host = typeof location === 'undefined' ? '' : location.hostname;
	if (!host || host === 'localhost' || /^[\d.]+$/.test(host) || host.includes(':'))
		return 'privatecrates.dev';
	return host.replace(/^www\./, '');
}
