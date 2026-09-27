/**
 * The server's own view, from `GET https://{apex}/api/status` (docs/trust-and-status.md §2), and what its error
 * counts say about our calls to GitHub and Stripe.
 */

export interface CallCounts {
	windowSeconds: number;
	requests: number;
	errors: number;
	rateLimited: number;
}

export interface ServerStatus {
	version: string;
	startedAt: string;
	github: CallCounts;
	/** Absent when Stripe is not configured on that environment. */
	stripe?: CallCounts;
}

function isRecord(value: unknown): value is Record<string, unknown> {
	return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function count(value: unknown): number | undefined {
	return typeof value === 'number' && Number.isFinite(value) && value >= 0 ? Math.floor(value) : undefined;
}

function counts(value: unknown, defaultWindow: number): CallCounts | undefined {
	if (!isRecord(value)) return undefined;
	const requests = count(value.requests);
	const errors = count(value.errors);
	if (requests === undefined || errors === undefined) return undefined;
	return {
		windowSeconds: count(value.window_seconds) ?? defaultWindow,
		requests,
		errors,
		rateLimited: count(value.rate_limited) ?? 0,
	};
}

/** Parses the JSON body; undefined when it is not the shape we expect (for example the website's HTML fallback). */
export function parseServerStatus(body: unknown): ServerStatus | undefined {
	if (!isRecord(body)) return undefined;
	const github = counts(body.github, 300);
	if (!github) return undefined;
	let stripe: CallCounts | undefined;
	if (isRecord(body.stripe) && body.stripe.configured !== false) {
		stripe = counts(body.stripe, github.windowSeconds);
	}
	return {
		version: typeof body.version === 'string' ? body.version : '',
		startedAt: typeof body.started_at === 'string' ? body.started_at : '',
		github,
		stripe,
	};
}

export type ErrorLevel = 'none' | 'degraded' | 'outage';

export interface ErrorThresholds {
	/** Fewer calls than this in the window are too few to judge. */
	minRequests: number;
	degraded: number;
	outage: number;
}

export const GITHUB_THRESHOLDS: ErrorThresholds = { minRequests: 20, degraded: 0.05, outage: 0.5 };
export const STRIPE_THRESHOLDS: ErrorThresholds = { minRequests: 5, degraded: 0.2, outage: 0.6 };

/** Errors and rate-limited answers, as a share of calls. */
export function errorRate(c: CallCounts): number {
	return c.requests === 0 ? 0 : Math.min(1, (c.errors + c.rateLimited) / c.requests);
}

export function errorLevel(c: CallCounts | undefined, t: ErrorThresholds): ErrorLevel {
	if (!c || c.requests < t.minRequests) return 'none';
	const rate = errorRate(c);
	if (rate >= t.outage) return 'outage';
	if (rate >= t.degraded) return 'degraded';
	return 'none';
}

export function describeErrors(c: CallCounts): string {
	const pct = Math.round(errorRate(c) * 100);
	const minutes = Math.round(c.windowSeconds / 60);
	const limited = c.rateLimited > 0 ? `, ${c.rateLimited.toLocaleString('en-GB')} rate limited` : '';
	return `${pct}% of ${c.requests.toLocaleString('en-GB')} calls failed in the last ${minutes} minutes${limited}`;
}
