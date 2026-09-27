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
	/** A trial started by itself (the organisation grew past the free limit) and Stripe has no address to remind. */
	billing_email_missing?: boolean;
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

/**
 * One integrity failure the server's checks found (the verifier's checks, run server-side). The plan leaves its
 * shape open, so this accepts the verifier's finding (`severity`, `subject`, `message`) as well as a coded form.
 */
export interface ComplianceProblem {
	severity?: 'error' | 'warning' | 'info' | string;
	/** What it is about, e.g. `story_engine@0.2.0`. */
	subject?: string;
	message?: string;
	code?: string;
	crate?: string;
	version?: string;
	detail?: string;
}

export interface Integrity {
	versions: number;
	/** Versions whose release exists and is immutable. */
	immutable: number;
	/** Versions whose .crate digest on GitHub matches the index `cksum`. */
	digest_matches: number;
	/** Versions with valid GitHub-signed provenance. */
	provenance: number;
	/** Versions published manually (crates that allow it): no provenance, listed for review. */
	manual: number;
	problems: ComplianceProblem[];
}

/** Who may publish a crate, from its owners file. */
export interface Publisher {
	crate: string;
	/** `owner/name` on GitHub. */
	repository: string;
	workflows: string[];
	environment: string | null;
	manual_publish: boolean;
}

export type RiskCode = 'name_clash' | 'manual_publish_allowed' | 'missing_provenance' | 'no_verify_workflow';

export interface Risk {
	code: RiskCode | string;
	crate?: string;
	detail?: string;
}

export interface AuditEntry {
	at: string;
	/** publish, yank, unyank, owners, settings, … */
	action: string;
	crate?: string;
	version?: string;
	by: string;
	provenance?: boolean;
	commit?: string;
}

/** GET /api/orgs/{org}/compliance (docs/trust-and-status.md §4). */
export interface Compliance {
	org: { id: number; login: string };
	generated_at: string;
	integrity: Integrity;
	publishers: Publisher[];
	risks: Risk[];
	/** Newest first; one page. */
	audit: AuditEntry[];
	/**
	 * The `?before=` value for the next (older) page, when the server sends one; null when there are no older
	 * entries. Without it, the page passes the oldest entry's `at`.
	 */
	audit_next_before?: string | null;
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
	/**
	 * Starts the no-card free trial; returns the onboarding document. Stripe sends the trial-ending reminder and
	 * invoices to the billing email.
	 */
	trial: (login: string, billingEmail: string) =>
		request<Onboarding>('POST', `${org(login)}/trial`, { billing_email: billingEmail }),
	/** The compliance dashboard; `before` pages back through the audit log. */
	compliance: (login: string, before?: string) =>
		request<Compliance>(
			'GET',
			`${org(login)}/compliance${before ? `?before=${encodeURIComponent(before)}` : ''}`
		),
	checkout: (login: string) => request<{ url: string }>('POST', `${org(login)}/checkout`),
	portal: (login: string) => request<{ url: string }>('POST', `${org(login)}/portal`),
	/** Sets the address Stripe sends the trial reminder and invoices to; returns the onboarding document. */
	billingEmail: (login: string, billingEmail: string) =>
		request<Onboarding>('POST', `${org(login)}/billing-email`, { billing_email: billingEmail }),
	logout: () => request<void>('POST', '/auth/logout'),
	errors: () => request<CatalogEntry[]>('GET', '/api/errors')
};

/** The audit log as CSV, for auditors: a plain download with the session cookie. */
export function auditCsvUrl(login: string): string {
	return `${org(login)}/compliance/audit.csv`;
}

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
