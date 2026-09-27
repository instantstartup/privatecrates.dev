// In-browser mock of the account API, for `pnpm dev:mock` only. Production builds never include this file
// (see __MOCK_API__ in vite.config.ts; scripts/externalise-inline.js fails the build if it leaks).
//
// Pick a scenario with a query parameter on /account; it is remembered for the browser tab:
//   ?mock=admin          signed in; organisations in every state, acme not set up (default)
//   ?mock=member         the same, as a member of every organisation
//   ?mock=no-orgs        signed in, in no organisations
//   ?mock=signed-out     not signed in
//   ?mock=down           the API cannot be reached
//   ?mock=broken         the API answers 502 (HTML) to everything
//   ?mock=reset          start over (admin)
// Billing scenarios, each with the single organisation acme:
//   ?mock=free           3 members: free, live
//   ?mock=free-again     4 members, with a paid subscription still running (the "you can cancel" note)
//   ?mock=over-limit     12 members, set up, before its trial: the plan step offers the trial
//   ?mock=trial          mid-trial, no card
//   ?mock=trial-ending   9 days of trial left, no card: the banner
//   ?mock=auto-trial     the trial started by itself (grew past 5): asks an admin for a billing email
//   ?mock=paid           subscribed, card on file
//   ?mock=past-due       a payment failed; Stripe is retrying
//   ?mock=inactive       the trial ended 3 days ago without a card
//   ?mock=returning      subscribed once, setting up again: Subscribe (Checkout, no trial)
//   ?mock=no-billing     12 members, billing not configured on the server
//   ?mock=just-finished  3 members, only the registry name left: saving it makes the registry live
// Compliance dashboard (the Compliance tab of a live registry; add &tab=compliance to open it directly):
//   ?mock=compliance           142 versions, all verified with provenance, no risks
//   ?mock=compliance-problems  142 versions: problems, manual publishes and every kind of risk
//   ?mock=compliance-empty     a live registry with nothing published yet
// Every other live registry answers with the clean data. The audit log pages 25 entries at a time (?before=).
// The trial step asks for a billing email: an address without "@" and a dot after it gets billing::email_invalid.
// The "Set up with your AI agent" prompt: the set-up variant beside the checklist (?mock=admin, acme; or
// ?mock=just-finished), and the crate-repository variant in the ready guide (?mock=free, trial, paid).
// Onboarding: the action links for GitHub steps come back to /account with ?mock_done=<step>, which completes them
// (comma-separated for several, e.g. ?org=acme&mock_done=storage_repo,storage_app). The billing portal comes back
// with ?mock_done=card, which adds a card.

import type {
	AuditEntry,
	CatalogEntry,
	Compliance,
	Onboarding,
	Org,
	Plan,
	Publisher,
	Session,
	Step,
	Tenant
} from '../api';

const SIGNED_IN = ['admin', 'member', 'no-orgs', 'signed-out', 'down', 'broken'] as const;
const BILLING = [
	'free',
	'free-again',
	'over-limit',
	'trial',
	'trial-ending',
	'paid',
	'past-due',
	'inactive',
	'returning',
	'no-billing',
	'just-finished',
	'compliance',
	'compliance-problems',
	'compliance-empty',
	'auto-trial'
] as const;
type Scenario = (typeof SIGNED_IN)[number] | (typeof BILLING)[number];

const GITHUB_STEPS = ['reader_app', 'storage_repo', 'storage_app'];
const LIMIT = 5;
const TRIAL_DAYS = 90;
const PRICE_USD = 100;

/** A Stripe subscription; dates are offsets in days from now. */
interface Sub {
	status: 'trialing' | 'active' | 'past_due' | 'canceled';
	trialEnd: number | null;
	periodEnd: number | null;
	card: boolean;
}

/** One organisation as the server would see it. */
interface OrgModel {
	id: number;
	login: string;
	role: 'admin' | 'member';
	members: number;
	done: string[];
	slug: string | null;
	sub: Sub | null;
	/** Had a trial (so POST /trial is refused). */
	trialUsed: boolean;
	/** Stripe configured on the server. */
	billing: boolean;
	/** Checkout finished but Stripe's webhook has not arrived: the subscription appears after a few session reads. */
	pendingCheckout: number | null;
	/** Set on the Stripe customer when the trial starts. */
	billingEmail?: string;
}

interface State {
	scenario: Scenario;
	orgs: OrgModel[];
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

function model(id: number, login: string, over: Partial<OrgModel> = {}): OrgModel {
	return {
		id,
		login,
		role: 'admin',
		members: 3,
		done: [...GITHUB_STEPS],
		slug: login,
		sub: null,
		trialUsed: false,
		billing: true,
		pendingCheckout: null,
		...over
	};
}

const trialing = (days: number, card = false): Sub => ({
	status: 'trialing',
	trialEnd: days,
	periodEnd: null,
	card
});
const paid = (status: Sub['status'] = 'active'): Sub => ({
	status,
	trialEnd: -60,
	periodEnd: 21,
	card: true
});

function orgsFor(scenario: Scenario): OrgModel[] {
	const acme = (over: Partial<OrgModel>) => [model(100, 'acme', over)];
	switch (scenario) {
		case 'free':
			return acme({ members: 3 });
		case 'free-again':
			return acme({ members: 4, sub: paid(), trialUsed: true });
		case 'over-limit':
			return acme({ members: 12 });
		case 'trial':
			return acme({ members: 12, sub: trialing(52), trialUsed: true });
		case 'trial-ending':
			return acme({ members: 12, sub: trialing(9), trialUsed: true });
		case 'auto-trial':
			// Grew past the limit: the trial started by itself, so Stripe has no address yet.
			return acme({ members: 7, sub: trialing(88), trialUsed: true, billingEmail: '' });
		case 'paid':
			return acme({ members: 12, sub: paid(), trialUsed: true });
		case 'past-due':
			return acme({ members: 12, sub: paid('past_due'), trialUsed: true });
		case 'inactive':
			return acme({
				members: 12,
				sub: { status: 'canceled', trialEnd: -3, periodEnd: -3, card: false },
				trialUsed: true
			});
		case 'returning':
			// Subscribed once, cancelled long ago, and now setting the registry up again.
			return acme({
				members: 12,
				slug: null,
				sub: { status: 'canceled', trialEnd: -300, periodEnd: -200, card: false },
				trialUsed: true
			});
		case 'no-billing':
			return acme({ members: 12, billing: false });
		case 'just-finished':
			return acme({ members: 3, slug: null });
		case 'compliance':
		case 'compliance-problems':
		case 'compliance-empty':
			return acme({ members: 12, sub: paid(), trialUsed: true });
		default: {
			// acme is listed only because it has installed the reader App (GitHub user tokens see nothing else).
			const role = scenario === 'member' ? 'member' : 'admin';
			return [
				model(100, 'acme', { role, members: 12, done: ['reader_app'], slug: null }),
				model(101, 'globex', { role: 'member', members: 3 }),
				model(102, 'initech', { role, members: 40, sub: trialing(52), trialUsed: true }),
				model(103, 'hooli', { role, members: 9, sub: trialing(6), trialUsed: true }),
				model(104, 'stark', { role, members: 25, sub: paid(), trialUsed: true }),
				model(105, 'wayne', { role, members: 8, sub: paid('past_due'), trialUsed: true }),
				model(106, 'umbrella', {
					role,
					members: 30,
					sub: { status: 'canceled', trialEnd: -3, periodEnd: -3, card: false },
					trialUsed: true
				})
			];
		}
	}
}

function fresh(scenario: Scenario = 'admin'): State {
	return { scenario, orgs: orgsFor(scenario) };
}

function load(): State {
	try {
		const raw = sessionStorage.getItem(KEY);
		if (raw) {
			const state = JSON.parse(raw) as State;
			if (Array.isArray(state.orgs)) return state;
		}
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
	else if (mock && (BILLING as readonly string[]).includes(mock)) state = fresh(mock as Scenario);
	else if (mock && (SIGNED_IN as readonly string[]).includes(mock)) {
		// Switching between the multi-organisation scenarios keeps onboarding progress.
		const multi = state.orgs.length > 1;
		state = multi ? { ...state, scenario: mock as Scenario } : fresh(mock as Scenario);
		const role = mock === 'member' ? 'member' : 'admin';
		for (const o of state.orgs) if (o.login !== 'globex') o.role = role;
	}
	const target = state.orgs.find((o) => o.login === (params.get('org') ?? 'acme')) ?? state.orgs[0];
	// Comma-separated, e.g. mock_done=storage_repo,storage_app
	for (const done of (params.get('mock_done') ?? '').split(',').filter(Boolean)) {
		if (!target) break;
		if (done === 'card') {
			if (target.sub) target.sub.card = true;
		} else if (!target.done.includes(done)) target.done.push(done);
	}
	state.appliedSearch = location.search;
	save(state);
	return state;
}

const day = 86_400_000;
const iso = (offsetDays: number | null) =>
	offsetDays === null ? null : new Date(Date.now() + offsetDays * day).toISOString();
const base = 'privatecrates.dev';

const isActive = (sub: Sub | null) =>
	!!sub && (sub.status === 'trialing' || sub.status === 'active' || sub.status === 'past_due');

function planOf(o: OrgModel): Plan {
	if (o.members <= LIMIT) return 'free';
	switch (o.sub?.status) {
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

function trialAvailable(o: OrgModel): boolean {
	return o.billing && o.members > LIMIT && !o.trialUsed && !isActive(o.sub);
}

function tenantOf(o: OrgModel): Tenant | null {
	if (!o.slug) return null;
	return {
		slug: o.slug,
		registry_url: `https://${o.slug}.${base}`,
		status: o.sub?.status ?? null,
		trial_ends_at: iso(o.sub?.trialEnd ?? null),
		current_period_end: iso(o.sub?.periodEnd ?? null)
	};
}

function toOrg(o: OrgModel): Org {
	return {
		id: o.id,
		login: o.login,
		avatar_url: `https://avatars.example.invalid/${o.login}`,
		role: o.role,
		members: o.members,
		free_member_limit: LIMIT,
		plan: planOf(o),
		trial_ends_at: iso(o.sub?.trialEnd ?? null),
		has_payment_method: o.sub?.card ?? false,
		billing_email_missing: !!o.sub && o.billingEmail === '',
		current_period_end: iso(o.sub?.periodEnd ?? null),
		trial_available: trialAvailable(o),
		tenant: tenantOf(o)
	};
}

function session(state: State): Session {
	if (state.scenario === 'signed-out') return { user: null, orgs: [] };
	const user = { login: 'alice', avatar_url: 'https://avatars.example.invalid/alice', name: 'Alice Moreau' };
	if (state.scenario === 'no-orgs') return { user, orgs: [], install_url: INSTALL_URL };
	// Pretend Stripe's webhook takes a moment: the subscription appears on the second session read after checkout.
	for (const o of state.orgs) {
		if (o.pendingCheckout === null) continue;
		o.pendingCheckout += 1;
		if (o.pendingCheckout >= 2) {
			o.sub = { status: 'active', trialEnd: o.sub?.trialEnd ?? null, periodEnd: 30, card: true };
			o.pendingCheckout = null;
		}
	}
	save(state);
	return { user, orgs: state.orgs.map(toOrg), install_url: INSTALL_URL };
}

function planStep(o: OrgModel, status: (done: boolean) => Step['status']): Step {
	const plan = planOf(o);
	if (plan === 'free') return { id: 'plan', status: 'done' };
	if (!o.billing)
		return {
			id: 'plan',
			status: 'done',
			detail: 'Billing is not configured on this server, so the registry works without a plan.'
		};
	if (isActive(o.sub)) return { id: 'plan', status: 'done' };
	if (trialAvailable(o))
		return {
			id: 'plan',
			status: status(false),
			detail: `${o.members} members: start your 3-month free trial, no card needed.`
		};
	return {
		id: 'plan',
		status: status(false),
		detail: `${o.login} has had its free trial. Subscribe to use the registry: $${PRICE_USD} per month, card required.`
	};
}

function onboarding(o: OrgModel): Onboarding {
	const member = o.role !== 'admin';
	const appsInstalled = o.done.includes('reader_app') && o.done.includes('storage_app');
	const ask = member ? { detail: `Ask an admin of ${o.login} to complete this step.` } : {};
	const status = (done: boolean, needsApps = false): Step['status'] =>
		done ? 'done' : member || (needsApps && !appsInstalled) ? 'blocked' : 'todo';
	const back = (id: string) => `/account?org=${o.login}&mock_done=${id}`;
	const gh = (id: string, extra: Partial<Step> = {}): Step => ({
		id,
		status: status(o.done.includes(id)),
		action_url: back(id),
		...extra,
		...ask
	});
	const plan = planStep(o, (d) => status(d));
	return {
		org: { id: o.id, login: o.login },
		steps: [
			gh('reader_app'),
			gh('storage_repo', {
				detail: `Create a private repository, e.g. ${o.login}/crates-store, and enable immutable releases in Settings → General → Releases.`
			}),
			gh('storage_app'),
			{
				id: 'settings',
				status: status(o.slug !== null, true),
				detail: member
					? ask.detail
					: appsInstalled
						? 'Choose your registry name.'
						: 'Install both Apps first: the storage App writes privatecrates.toml.'
			},
			member && plan.status !== 'done' ? { ...plan, ...ask } : plan
		],
		suggested_slug: o.login
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
	[
		'billing::email_invalid',
		'{email} is not a valid email address. Stripe sends the trial reminder and invoices there',
		422
	],
	['registry::not_found', 'not found', 404],
	['github::rate_limited', 'GitHub’s rate limit was reached; please try again in a few minutes', 503]
].map(([code, message, http_status]) => ({ code, message, http_status }) as CatalogEntry);

// ---------------------------------------------------------------------------------------------------------------
// Compliance

const AUDIT_PAGE = 25;
const HOUR = 3_600_000;

function publishersFor(org: string, problems: boolean): Publisher[] {
	const p = (
		crate: string,
		repo: string,
		workflows: string[],
		environment: string | null,
		manual = false
	): Publisher => ({ crate, repository: `${org}/${repo}`, workflows, environment, manual_publish: manual });
	return [
		p('story_engine', 'story-engine', ['release.yml'], 'crates'),
		p('story_macros', 'story-engine', ['release.yml'], 'crates'),
		p('ledger_core', 'ledger', ['publish.yml'], 'crates'),
		p('http_retry', 'platform', ['publish.yml', 'hotfix.yml'], null),
		p('serde_utils', 'platform', ['publish.yml'], null),
		p('tools', 'dev-tools', ['publish.yml'], null, problems)
	];
}

/** The whole audit log, newest first: one publish per version, plus the odd yank and owners change. */
function auditFor(org: string, publishers: Publisher[], versions: number, problems: boolean): AuditEntry[] {
	// A fixed clock, so pages stay stable while the mock is open.
	const start = Date.UTC(2026, 8, 27, 14, 20, 1);
	const entries: AuditEntry[] = [];
	const counters: Record<string, number> = {};
	let t = start;
	for (let i = 0; i < versions; i++) {
		const pub = publishers[i % publishers.length];
		const n = (counters[pub.crate] = (counters[pub.crate] ?? 0) + 1);
		const version = `0.${Math.floor((200 - n) / 10)}.${(200 - n) % 10}`;
		const manual = problems && pub.manual_publish && n <= 3;
		const workflow = pub.workflows[0];
		entries.push({
			at: new Date(t).toISOString(),
			action: 'publish',
			crate: pub.crate,
			version,
			by: manual
				? 'alice (manual publish)'
				: `workflow ${pub.repository}/.github/workflows/${workflow}@refs/tags/v${version} (run ${900 - i})`,
			provenance: !manual && !(problems && pub.crate === 'http_retry' && n === 1),
			commit: ((i * 2654435761) >>> 0).toString(16).padStart(8, '0').slice(0, 7)
		});
		t -= 7 * HOUR + (i % 5) * 1_380_000;
		if (i % 23 === 11) {
			entries.push({
				at: new Date(t).toISOString(),
				action: 'yank',
				crate: pub.crate,
				version,
				by: 'alice',
				commit: ((i * 40503) >>> 0).toString(16).padStart(7, '0').slice(0, 7)
			});
			t -= 3 * HOUR;
		}
		if (i % 31 === 30) {
			entries.push({
				at: new Date(t).toISOString(),
				action: 'owners',
				crate: pub.crate,
				by: 'bob (commit to owners/)',
				commit: ((i * 69069) >>> 0).toString(16).padStart(7, '0').slice(0, 7)
			});
			t -= 5 * HOUR;
		}
	}
	if (versions > 0)
		entries.push({
			at: new Date(t - 24 * HOUR).toISOString(),
			action: 'settings',
			by: `privatecrates-storage[bot], set up by alice (registry ${org})`,
			commit: 'a1b2c3d'
		});
	return entries;
}

function compliance(o: OrgModel, scenario: Scenario, before: string | null): Compliance {
	const kind =
		scenario === 'compliance-problems' ? 'problems' : scenario === 'compliance-empty' ? 'empty' : 'clean';
	const problems = kind === 'problems';
	const versions = kind === 'empty' ? 0 : 142;
	const publishers = kind === 'empty' ? [] : publishersFor(o.login, problems);
	const all = auditFor(o.login, publishers, versions, problems);
	const older = before ? all.filter((e) => e.at < before) : all;
	return {
		org: { id: o.id, login: o.login },
		generated_at: new Date(Date.now() - 4 * 60_000).toISOString(),
		integrity: problems
			? {
					versions,
					immutable: 141,
					digest_matches: 142,
					provenance: 138,
					manual: 3,
					problems: [
						{
							severity: 'error',
							subject: 'ledger_core@0.18.4',
							message:
								'the release is not immutable: immutable releases were off in the storage repository when it was published'
						},
						{
							severity: 'warning',
							subject: 'http_retry@0.19.9',
							message:
								'no provenance, and http_retry does not allow manual publishing: the provenance file is missing from the release'
						}
					]
				}
			: {
					versions,
					immutable: versions,
					digest_matches: versions,
					provenance: versions,
					manual: 0,
					problems: []
				},
		publishers,
		risks: problems
			? [
					{ code: 'name_clash', crate: 'serde_utils', detail: 'a crate with this name exists on crates.io' },
					{ code: 'manual_publish_allowed', crate: 'tools' },
					{ code: 'missing_provenance', crate: 'http_retry', detail: '1 version without provenance' },
					{ code: 'no_verify_workflow' }
				]
			: kind === 'empty'
				? [{ code: 'no_verify_workflow' }]
				: [],
		audit: older.slice(0, AUDIT_PAGE)
	};
}

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

	const m = path.match(
		/^\/api\/orgs\/([^/]+)\/(onboarding|settings|trial|checkout|portal|compliance|billing-email)$/
	);
	if (!m) return delay(error(404, 'not found', 'registry::not_found'));
	if (state.scenario === 'signed-out' || state.scenario === 'no-orgs')
		return delay(error(401, 'Sign in to continue.', 'session::required'));
	const [, login, action] = m;
	const o = state.orgs.find((x) => x.login === login);
	if (!o) return delay(error(404, `You are not a member of ${login}.`, 'account::org_not_found'));

	if (action === 'onboarding' && method === 'GET') return delay(json(onboarding(o)));
	if (action === 'compliance' && method === 'GET') {
		if (!o.slug) return delay(error(404, `${login} has no registry yet.`, 'registry::not_found'));
		return delay(json(compliance(o, state.scenario, url.searchParams.get('before'))), 500);
	}
	if (method !== 'POST') return delay(error(405, 'method not allowed', 'registry::method'));
	if (o.role !== 'admin')
		return delay(error(403, `Only admins of ${login} can do this.`, 'account::admin_required'));

	if (action === 'settings') {
		const body = JSON.parse(String(init?.body ?? '{}')) as { slug?: string };
		const slug = (body.slug ?? '').trim();
		if (!o.done.includes('storage_app'))
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
		if (TAKEN.includes(slug) && slug !== login)
			return delay(error(409, `${slug} is taken. Choose another name.`, 'account::slug_taken'));
		o.slug = slug;
		save(state);
		return delay(json(onboarding(o)), 700);
	}
	if (action === 'billing-email') {
		const body = JSON.parse(String(init?.body ?? '{}')) as { billing_email?: string };
		const email = (body.billing_email ?? '').trim();
		if (!/^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(email))
			return delay(
				error(
					400,
					`${email || 'That'} is not a valid email address. Stripe sends the trial reminder and invoices there.`,
					'billing::email_invalid'
				)
			);
		if (!o.sub) return delay(error(409, `${login} has no subscription yet.`, 'billing::no_subscription'));
		o.billingEmail = email;
		save(state);
		return delay(json(onboarding(o)), 500);
	}
	if (action === 'trial') {
		const body = JSON.parse(String(init?.body ?? '{}')) as { billing_email?: string };
		const email = (body.billing_email ?? '').trim();
		if (!/^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(email))
			return delay(
				error(
					422,
					email
						? `${email} is not a valid email address. Stripe sends the trial reminder and invoices there.`
						: 'Enter a billing email address: Stripe sends the trial reminder and invoices there.',
					'billing::email_invalid'
				)
			);
		if (o.members <= LIMIT)
			return delay(
				error(409, `${login} has ${o.members} members, so it is on the free plan.`, 'billing::free_plan')
			);
		if (isActive(o.sub))
			return delay(error(409, `${login} already has a subscription.`, 'billing::already_subscribed'));
		if (o.trialUsed)
			return delay(error(409, `${login} has already had its free trial.`, 'billing::trial_used'));
		if (!o.billing)
			return delay(error(503, 'Billing is not configured on this server.', 'billing::not_configured'));
		o.sub = trialing(TRIAL_DAYS);
		o.trialUsed = true;
		o.billingEmail = email;
		save(state);
		return delay(json(onboarding(o)), 700);
	}
	if (action === 'checkout') {
		if (isActive(o.sub))
			return delay(error(409, `${login} already has a subscription.`, 'billing::already_subscribed'));
		o.pendingCheckout = 0;
		save(state);
		return delay(json({ url: `/account?org=${login}&checkout=success` }), 700);
	}
	if (action === 'portal') {
		if (!o.sub && !o.trialUsed)
			return delay(error(409, `${login} has no billing account.`, 'billing::no_customer'));
		return delay(json({ url: `/account?org=${login}&mock_done=card` }), 500);
	}
	return delay(error(404, 'not found', 'registry::not_found'));
};
