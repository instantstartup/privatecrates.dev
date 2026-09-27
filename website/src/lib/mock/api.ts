// In-browser mock of the account API, for `pnpm dev:mock` only. Production builds never include this file
// (see __MOCK_API__ in vite.config.ts; scripts/externalise-inline.js fails the build if it leaks).
//
// Pick a scenario with a query parameter on /account; it is remembered for the browser tab:
//   ?mock=admin        signed in; organisations in every state (default)
//   ?mock=member       signed in as a member of organisations only
//   ?mock=no-orgs      signed in, in no organisations
//   ?mock=signed-out   not signed in
//   ?mock=down         the API cannot be reached
//   ?mock=broken       the API answers 502 (HTML) to everything
//   ?mock=reset        start over (admin, nothing set up)
// Onboarding: the action links for GitHub steps come back to /account with ?mock_done=<step>, which completes them
// (comma-separated for several, e.g. ?org=acme&mock_done=storage_repo,storage_app).

import type { CatalogEntry, Onboarding, Org, Session, Step, Tenant } from '../api';

type Scenario = 'admin' | 'member' | 'no-orgs' | 'signed-out' | 'down' | 'broken';

interface State {
	scenario: Scenario;
	/** Onboarding progress of the "acme" organisation. */
	acme: { done: string[]; slug: string | null; subscribed: boolean; sessionPolls: number };
	appliedSearch?: string;
}

const KEY = 'pc-mock';
const RESERVED = [
	'www',
	'dev',
	'api',
	'app',
	'docs',
	'status',
	'mail',
	'admin',
	'billing',
	'login',
	'static',
	'assets'
];
const INSTALL_URL = 'https://github.com/apps/privatecrates-reader/installations/new';
const TAKEN = ['globex', 'initech'];

function fresh(scenario: Scenario = 'admin'): State {
	// acme is listed only because it has installed the reader App (GitHub user tokens see nothing else).
	return { scenario, acme: { done: ['reader_app'], slug: null, subscribed: false, sessionPolls: 0 } };
}

function load(): State {
	try {
		const raw = sessionStorage.getItem(KEY);
		if (raw) return JSON.parse(raw) as State;
	} catch {
		// Fall through to a fresh state.
	}
	return fresh();
}

function save(state: State) {
	try {
		sessionStorage.setItem(KEY, JSON.stringify(state));
	} catch {
		// Mock state then lasts for this page only.
	}
}

/** Applies ?mock= and ?mock_done= from the current URL. */
function sync(): State {
	let state = load();
	// Apply the query once per URL, so later requests from the same page do not reset what happened since.
	if (state.appliedSearch === location.search) return state;
	const params = new URLSearchParams(location.search);
	const mock = params.get('mock');
	if (mock === 'reset') state = fresh();
	else if (mock && ['admin', 'member', 'no-orgs', 'signed-out', 'down', 'broken'].includes(mock))
		state = { ...state, scenario: mock as Scenario };
	// Comma-separated, e.g. mock_done=storage_repo,storage_app
	for (const done of (params.get('mock_done') ?? '').split(',').filter(Boolean)) {
		if (!state.acme.done.includes(done)) state.acme.done.push(done);
	}
	state.appliedSearch = location.search;
	save(state);
	return state;
}

const day = 86_400_000;
const iso = (offsetDays: number) => new Date(Date.now() + offsetDays * day).toISOString();
const base = 'privatecrates.dev';

function tenant(slug: string, status: string | null, extra: Partial<Tenant> = {}): Tenant {
	return {
		slug,
		registry_url: `https://${slug}.${base}`,
		status,
		trial_ends_at: status === 'trialing' ? iso(9) : null,
		current_period_end:
			status === 'trialing' || status === null ? null : iso(status === 'canceled' ? -3 : 21),
		...extra
	};
}

function avatar(login: string) {
	return `https://avatars.example.invalid/${login}`;
}

function acmeTenant(state: State): Tenant | null {
	const a = state.acme;
	if (!a.slug) return null;
	// Pretend Stripe's webhook takes a moment: the subscription appears on the second session read after checkout.
	if (!a.subscribed || a.sessionPolls < 2) return tenant(a.slug, null);
	return tenant(a.slug, 'trialing');
}

function session(state: State): Session {
	if (state.scenario === 'signed-out') return { user: null, orgs: [] };
	const user = { login: 'alice', avatar_url: avatar('alice'), name: 'Alice Moreau' };
	if (state.scenario === 'no-orgs') return { user, orgs: [], install_url: INSTALL_URL };
	if (state.acme.subscribed) state.acme.sessionPolls += 1;
	save(state);
	const role = state.scenario === 'member' ? 'member' : 'admin';
	const orgs: Org[] = [
		{ id: 100, login: 'acme', avatar_url: avatar('acme'), role, tenant: acmeTenant(state) },
		{
			id: 101,
			login: 'globex',
			avatar_url: avatar('globex'),
			role: 'member',
			tenant: tenant('globex', 'trialing')
		},
		{ id: 102, login: 'initech', avatar_url: avatar('initech'), role, tenant: tenant('initech', 'active') },
		{ id: 103, login: 'hooli', avatar_url: avatar('hooli'), role, tenant: tenant('hooli', 'past_due') },
		{
			id: 104,
			login: 'umbrella',
			avatar_url: avatar('umbrella'),
			role,
			tenant: tenant('umbrella', 'canceled')
		}
	];
	return { user, orgs, install_url: INSTALL_URL };
}

function onboarding(state: State, login: string): Onboarding | null {
	const s = session(state);
	const org = s.orgs.find((o) => o.login === login);
	if (!org) return null;
	const member = org.role !== 'admin';
	const done = (id: string): boolean => {
		if (login !== 'acme') {
			// Other organisations are fully set up, except a cancelled subscription.
			return !(id === 'subscription' && org.tenant && org.tenant.status === 'canceled');
		}
		if (id === 'settings') return state.acme.slug !== null;
		if (id === 'subscription') return state.acme.subscribed;
		return state.acme.done.includes(id);
	};
	const appsInstalled = done('reader_app') && done('storage_app');
	const status = (id: string): Step['status'] =>
		done(id) ? 'done' : member || (id === 'settings' && !appsInstalled) ? 'blocked' : 'todo';
	const ask = member ? { detail: `Ask an admin of ${login} to complete this step.` } : {};
	const back = (id: string) => `/account?org=${login}&mock_done=${id}`;
	return {
		org: { id: org.id, login },
		steps: [
			{ id: 'reader_app', status: status('reader_app'), action_url: back('reader_app'), ...ask },
			{
				id: 'storage_repo',
				status: status('storage_repo'),
				detail: `Create a private repository, e.g. ${login}/crates-store, and enable immutable releases in Settings → General → Releases.`,
				action_url: back('storage_repo'),
				...ask
			},
			{ id: 'storage_app', status: status('storage_app'), action_url: back('storage_app'), ...ask },
			{
				id: 'settings',
				status: status('settings'),
				detail: member
					? ask.detail
					: appsInstalled
						? 'Choose your registry name.'
						: 'Install both Apps first: the storage App writes privatecrates.toml.'
			},
			{ id: 'subscription', status: status('subscription'), ...ask }
		],
		suggested_slug: login
	};
}

function json(body: unknown, status = 200): Response {
	return new Response(JSON.stringify(body), { status, headers: { 'Content-Type': 'application/json' } });
}

function error(status: number, detail: string, code: string): Response {
	return json({ errors: [{ detail, code }] }, status);
}

const catalog: CatalogEntry[] = [
	['auth::token_required', 'this registry needs a token; see {login_url}', 401],
	[
		'auth::token_rejected',
		'GitHub rejected the token; it may have expired. Run `cargo login` for this registry',
		401
	],
	['auth::registry_token_invalid', 'the registry token is invalid or has expired', 401],
	['auth::oidc_token_invalid', 'the OIDC token is invalid: {reason}', 401],
	['auth::sso_required', 'the token is not authorised for SAML single sign-on; authorise it at {url}', 403],
	['auth::no_access', 'you do not have access to any repository in this registry', 403],
	['auth::push_required', '{action} needs push access to {repository}', 403],
	[
		'billing::subscription_inactive',
		'this registry’s subscription is not active; see https://privatecrates.dev/account',
		402
	],
	['publish::ci_only', '{name} is published from CI only, so every version has verifiable provenance', 403],
	['publish::workflow_not_allowed', '{name} can only be published by {allowed}, not {workflow}', 403],
	[
		'publish::environment_required',
		'{name} can only be published from a job in the `{environment}` GitHub environment',
		403
	],
	['publish::version_exists', '{name} {version} already exists; versions are immutable', 409],
	['publish::name_on_crates_io', '{name} also exists on crates.io', 409],
	['publish::crate_too_large', 'the .crate file is {size} bytes; the limit is {limit}', 413],
	[
		'publish::immutable_releases_disabled',
		'the storage repository does not have immutable releases enabled',
		409
	],
	['registry::not_found', 'not found', 404],
	['github::rate_limited', 'GitHub’s rate limit was reached; please try again in a few minutes', 503]
].map(([code, message, http_status]) => ({ code, message, http_status }) as CatalogEntry);

function delay<T>(value: T, ms = 350): Promise<T> {
	return new Promise((resolve) => setTimeout(() => resolve(value), ms));
}

export const mockFetch: typeof fetch = async (input, init) => {
	const state = sync();
	const url = new URL(
		typeof input === 'string' ? input : input instanceof URL ? input.href : input.url,
		location.href
	);
	const method = (init?.method ?? 'GET').toUpperCase();
	const path = url.pathname;

	if (state.scenario === 'down') {
		await delay(null, 600);
		throw new TypeError('Failed to fetch');
	}
	if (state.scenario === 'broken' && path !== '/api/errors') {
		return delay(
			new Response('<h1>Bad gateway</h1>', { status: 502, headers: { 'Content-Type': 'text/html' } })
		);
	}
	if (method === 'POST' && init?.headers && !JSON.stringify(init.headers).includes('application/json')) {
		return delay(error(403, 'POST requests must send Content-Type: application/json.', 'session::csrf'));
	}

	if (path === '/api/errors') return delay(json(catalog));
	if (path === '/auth/logout' && method === 'POST') {
		state.scenario = 'signed-out';
		save(state);
		return delay(new Response(null, { status: 204 }));
	}
	if (path === '/api/session') return delay(json(session(state)));

	const m = path.match(/^\/api\/orgs\/([^/]+)\/(onboarding|settings|checkout|portal)$/);
	if (!m) return delay(error(404, 'not found', 'registry::not_found'));
	if (state.scenario === 'signed-out') return delay(error(401, 'Sign in to continue.', 'session::required'));
	const [, login, action] = m;
	const doc = onboarding(state, login);
	if (!doc) return delay(error(404, `You are not a member of ${login}.`, 'account::org_not_found'));
	const admin = session(state).orgs.find((o) => o.login === login)?.role === 'admin';

	if (action === 'onboarding' && method === 'GET') return delay(json(doc));
	if (method !== 'POST') return delay(error(405, 'method not allowed', 'registry::method'));
	if (!admin) return delay(error(403, `Only admins of ${login} can do this.`, 'account::admin_required'));

	if (action === 'settings') {
		const body = JSON.parse(String(init?.body ?? '{}')) as { slug?: string };
		const slug = (body.slug ?? '').trim();
		if (!state.acme.done.includes('storage_app'))
			return delay(
				error(
					409,
					'Install the storage App on your storage repository first.',
					'account::storage_app_missing'
				)
			);
		if (!/^[a-z0-9](?:[a-z0-9-]{1,30}[a-z0-9])$/.test(slug))
			return delay(
				error(
					422,
					'Use 3 to 32 lowercase letters, digits or hyphens, starting and ending with a letter or digit.',
					'account::slug_invalid'
				)
			);
		if (RESERVED.includes(slug))
			return delay(error(422, `${slug} is reserved. Choose another name.`, 'account::slug_reserved'));
		if (TAKEN.includes(slug))
			return delay(error(409, `${slug} is taken. Choose another name.`, 'account::slug_taken'));
		state.acme.slug = slug;
		save(state);
		return delay(json(onboarding(state, login)), 700);
	}
	if (action === 'checkout') {
		const current = session(state).orgs.find((o) => o.login === login)?.tenant?.status;
		if (current === 'trialing' || current === 'active' || current === 'past_due')
			return delay(error(409, `${login} already has a subscription.`, 'billing::already_subscribed'));
		if (login === 'acme') {
			state.acme.subscribed = true;
			state.acme.sessionPolls = 0;
			save(state);
		}
		return delay(json({ url: `/account?org=${login}&checkout=success` }), 700);
	}
	if (action === 'portal') return delay(json({ url: `/account?org=${login}` }), 500);
	return delay(error(404, 'not found', 'registry::not_found'));
};
