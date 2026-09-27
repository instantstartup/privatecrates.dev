import type { Tenant } from '$lib/api';

const dateFormat = new Intl.DateTimeFormat('en-GB', { day: 'numeric', month: 'long', year: 'numeric' });

export function formatDate(iso: string | null | undefined): string | null {
	if (!iso) return null;
	const d = new Date(iso);
	return Number.isNaN(d.getTime()) ? null : dateFormat.format(d);
}

/** Reads keep working for this long after a cancelled subscription's period ends (billing model). */
export const READ_GRACE_DAYS = 14;

export function addDays(iso: string, days: number): string {
	return new Date(new Date(iso).getTime() + days * 86_400_000).toISOString();
}

export type Badge = { label: string; tone: 'ok' | 'warn' | 'danger' | 'idle' };

export function tenantBadge(tenant: Tenant | null): Badge {
	if (!tenant) return { label: 'Not set up', tone: 'idle' };
	if (tenant.status === null) return { label: 'Set-up incomplete', tone: 'idle' };
	switch (tenant.status) {
		case 'trialing':
			return { label: 'Free trial', tone: 'ok' };
		case 'active':
			return { label: 'Live', tone: 'ok' };
		case 'past_due':
			return { label: 'Payment overdue', tone: 'warn' };
		case 'canceled':
			return { label: 'Cancelled', tone: 'danger' };
		case 'unpaid':
			return { label: 'Unpaid', tone: 'danger' };
		default:
			return { label: 'Inactive', tone: 'danger' };
	}
}

/** The apex host the account page is served from, used to preview a registry's hostname. */
export function baseDomain(): string {
	const host = typeof location === 'undefined' ? '' : location.hostname;
	if (!host || host === 'localhost' || /^[\d.]+$/.test(host) || host.includes(':'))
		return 'privatecrates.dev';
	return host.replace(/^www\./, '');
}
