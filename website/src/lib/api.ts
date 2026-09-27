// Client for the account API described in docs/website-api.md. Same origin, cookie session.

export type TenantStatus = 'trialing' | 'active' | 'past_due' | 'canceled' | 'unpaid' | 'incomplete' | string;

export interface Tenant {
	slug: string;
	registry_url: string;
	/** null when the registry is configured but has no subscription (yet, or any more). */
	status: TenantStatus | null;
	trial_ends_at: string | null;
	current_period_end: string | null;
}

/**
 * The organisation's plan (docs/website-api.md, billing model): `free` at or under the member limit; `inactive` when
 * over it with a subscription that ended or never started.
 */
export type Plan = 'free' | 'trial' | 'paid' | 'past_due' | 'inactive';

export interface Org {
	id: number;
	login: string;
	avatar_url: string;
	role: 'admin' | 'member';
	/** Active members, not counting outside collaborators; null when not known yet. */
	members?: number | null;
	free_member_limit?: number;
	/** Absent only from servers older than the free plan; see planOf(). */
	plan?: Plan;
	trial_ends_at?: string | null;
	has_payment_method?: boolean;
	current_period_end?: string | null;
	/** Whether POST /trial will work (over the limit, never had a trial, billing configured). */
	trial_available?: boolean;
	tenant: Tenant | null;
}

export interface User {
	login: string;
	avatar_url: string;
	name: string | null;
}

export interface Session {
	user: User | null;
	/**
	 * Organisations the user belongs to that have installed the reader App (a GitHub App user token sees
	 * only those).
	 */
	orgs: Org[];
	/** Installs the reader App on another organisation; GitHub then returns the user to /account. */
	install_url?: string;
}

export type StepStatus = 'done' | 'todo' | 'blocked';

export interface Step {
	/** `plan` was called `subscription` by older servers. */
	id: 'reader_app' | 'storage_repo' | 'storage_app' | 'settings' | 'plan' | string;
	status: StepStatus;
	action_url?: string;
	detail?: string;
}

export interface Onboarding {
	org: { id: number; login: string };
	steps: Step[];
	suggested_slug?: string;
}

export interface CatalogEntry {
	code: string;
	message: string;
	http_status: number;
}

export interface ApiErrorItem {
	detail: string;
	code?: string;
}

/** An error from the API (or from reaching it), with the server's messages when it sent any. */
export class ApiError extends Error {
	constructor(
		/** HTTP status, or 0 when the server could not be reached. */
		readonly status: number,
		readonly errors: ApiErrorItem[]
	) {
		super(errors.map((e) => e.detail).join(' '));
	}
}

type Fetch = typeof fetch;

let transport: Promise<Fetch> | undefined;

function getTransport(): Promise<Fetch> {
	if (!transport) {
		transport = __MOCK_API__
			? import('./mock/api').then((m) => m.mockFetch)
			: Promise.resolve((input, init) => fetch(input, init));
	}
	return transport;
}

async function request<T>(method: 'GET' | 'POST', path: string, body?: unknown): Promise<T> {
	const doFetch = await getTransport();
	let res: Response;
	try {
		res = await doFetch(path, {
			method,
			credentials: 'include',
			headers:
				method === 'POST'
					? { 'Content-Type': 'application/json', Accept: 'application/json' }
					: { Accept: 'application/json' },
			body: method === 'POST' ? JSON.stringify(body ?? {}) : undefined
		});
	} catch {
		throw new ApiError(0, [
			{ detail: 'Could not reach PrivateCrates. Check your connection and try again.' }
		]);
	}
	if (res.status === 204) return undefined as T;
	let json: unknown = null;
	try {
		json = await res.json();
	} catch {
		// Not JSON: handled below.
	}
	if (!res.ok) {
		const errors = (json as { errors?: ApiErrorItem[] } | null)?.errors;
		throw new ApiError(
			res.status,
			Array.isArray(errors) && errors.length
				? errors
				: [
						{
							detail: `PrivateCrates answered with an unexpected error (HTTP ${res.status}). Try again in a moment.`
						}
					]
		);
	}
	if (json === null) {
		throw new ApiError(res.status, [
			{ detail: 'PrivateCrates sent a response this page could not read. Try again in a moment.' }
		]);
	}
	return json as T;
}

const org = (login: string) => `/api/orgs/${encodeURIComponent(login)}`;

export const api = {
	session: () => request<Session>('GET', '/api/session'),
	onboarding: (login: string) => request<Onboarding>('GET', `${org(login)}/onboarding`),
	saveSettings: (login: string, slug: string) =>
		request<Onboarding>('POST', `${org(login)}/settings`, { slug }),
	/** Starts the no-card free trial; returns the onboarding document. */
	trial: (login: string) => request<Onboarding>('POST', `${org(login)}/trial`),
	checkout: (login: string) => request<{ url: string }>('POST', `${org(login)}/checkout`),
	portal: (login: string) => request<{ url: string }>('POST', `${org(login)}/portal`),
	logout: () => request<void>('POST', '/auth/logout'),
	errors: () => request<CatalogEntry[]>('GET', '/api/errors')
};

/** Where "Sign in with GitHub" goes. */
export function loginUrl(returnTo = '/account'): string {
	if (__MOCK_API__) return `/account?mock=admin`;
	return `/auth/github/login?return_to=${encodeURIComponent(returnTo)}`;
}

/** Subscription states in which the registry works (docs/website-api.md, billing model). */
export function isActive(status: TenantStatus | null): boolean {
	return status === 'trialing' || status === 'active' || status === 'past_due';
}

/** The organisation's plan, derived from the subscription for servers that do not send it. */
export function planOf(org: Org): Plan {
	if (org.plan) return org.plan;
	switch (org.tenant?.status ?? null) {
		case null:
			return 'free';
		case 'trialing':
			return 'trial';
		case 'active':
			return 'paid';
		case 'past_due':
			return 'past_due';
		default:
			return 'inactive';
	}
}

/** Whether the organisation has, or had, a Stripe subscription: then it has a billing portal. */
export function hasSubscription(org: Org): boolean {
	return org.tenant?.status != null || !!org.trial_ends_at || !!org.current_period_end;
}

/** Whether every onboarding step is done. */
export function allDone(doc: Onboarding): boolean {
	return doc.steps.every((s) => s.status === 'done');
}
