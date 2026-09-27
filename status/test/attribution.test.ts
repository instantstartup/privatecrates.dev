import { describe, expect, it } from 'vitest';
import { evaluate, type ActiveIncident, type ComponentStatus, type Evaluation } from '../src/attribution.ts';
import type { ComponentId } from '../src/model.ts';
import { unavailable } from '../src/statuspage.ts';
import { counts, observation, server, snapshot, upstreams } from './helpers.ts';

function byId(ev: Evaluation): Record<ComponentId, ComponentStatus> {
	return Object.fromEntries(ev.components.map((c) => [c.id, c])) as Record<ComponentId, ComponentStatus>;
}

function states(ev: Evaluation): Record<ComponentId, string> {
	return Object.fromEntries(ev.components.map((c) => [c.id, c.state])) as Record<ComponentId, string>;
}

const ALL_OK = { reads: 'operational', downloads: 'operational', publishing: 'operational', signin: 'operational', website: 'operational', billing: 'operational' };

const githubApiIncident = (status: 'degraded_performance' | 'partial_outage' | 'major_outage' = 'degraded_performance') =>
	snapshot('github', { 'API Requests': status }, [
		{ name: 'Incident across several services', components: ['API Requests'], url: 'https://stspg.io/m1ps7yrhp4n8' },
	]);

describe('everything healthy', () => {
	it('is operational, with no reasons and nobody blamed', () => {
		const ev = evaluate(observation(), upstreams());
		expect(ev.state).toBe('operational');
		expect(states(ev)).toEqual(ALL_OK);
		for (const c of ev.components) {
			expect(c.cause).toBe('none');
			expect(c.reasons).toEqual([]);
			expect(c.notes).toEqual([]);
		}
	});
});

describe('our own probes fail, upstream healthy: it is ours', () => {
	it('healthz failing takes down everything on our server, but not billing', () => {
		const ev = evaluate(observation({ healthz: { ok: false, status: 0, ms: 10000, error: 'timeout' }, server: undefined }), upstreams());
		const c = byId(ev);
		for (const id of ['reads', 'downloads', 'publishing', 'signin', 'website'] as const) {
			expect(c[id].state).toBe('outage');
			expect(c[id].cause).toBe('ours');
			expect(c[id].reasons[0]).toMatchObject({ kind: 'ours', text: 'Our server is not responding', detail: 'Health check: timeout' });
		}
		expect(c.billing.state).toBe('operational');
		expect(ev.state).toBe('outage');
	});

	it('a health check that answers with an error is "failing", not "not responding"', () => {
		const ev = evaluate(observation({ healthz: { ok: false, status: 503, ms: 20, error: 'HTTP 503' }, server: undefined }), upstreams());
		expect(byId(ev).website.reasons[0]).toMatchObject({ text: 'Our server is failing its health check', detail: 'Health check: HTTP 503' });
	});

	it('a canary that does not answer Cargo takes down the registry components only', () => {
		const ev = evaluate(
			observation({ canary: { ok: false, status: 502, ms: 30, error: 'HTTP 502, expected 401' } }),
			upstreams(),
		);
		expect(states(ev)).toEqual({ ...ALL_OK, reads: 'outage', downloads: 'outage', publishing: 'outage' });
		expect(byId(ev).reads.reasons[0]).toMatchObject({
			kind: 'ours',
			text: 'The registry is not answering Cargo',
			detail: 'Canary registry: HTTP 502, expected 401',
		});
	});

	it('a 401 without WWW-Authenticate is a failure too', () => {
		const ev = evaluate(
			observation({ canary: { ok: false, status: 401, ms: 30, error: 'no WWW-Authenticate header' } }),
			upstreams(),
		);
		expect(byId(ev).reads.reasons[0]?.detail).toBe('Canary registry: no WWW-Authenticate header');
	});

	it('a failing /api/status degrades the website', () => {
		const ev = evaluate(observation({ apiStatus: { ok: false, status: 500, ms: 20, error: 'HTTP 500' }, server: undefined }), upstreams());
		expect(states(ev)).toEqual({ ...ALL_OK, website: 'degraded' });
		expect(byId(ev).website.cause).toBe('ours');
	});

	it('a missing /api/status (a server that predates it) is not a failure', () => {
		const ev = evaluate(observation({ apiStatus: { ok: true, status: 404, ms: 20 }, apiStatusMissing: true, server: undefined }), upstreams());
		expect(states(ev)).toEqual(ALL_OK);
	});

	it('our failure is listed before an upstream incident that happens at the same time', () => {
		const ev = evaluate(
			observation({ healthz: { ok: false, status: 503, ms: 20, error: 'HTTP 503' }, server: undefined }),
			upstreams(githubApiIncident()),
		);
		const signin = byId(ev).signin;
		expect(signin.state).toBe('outage');
		expect(signin.cause).toBe('ours');
		expect(signin.reasons.map((r) => r.kind)).toEqual(['ours', 'upstream']);
	});
});

describe('GitHub degraded but we are fine', () => {
	it('keeps components operational, with a note linking GitHub’s incident', () => {
		const ev = evaluate(observation(), upstreams(githubApiIncident()));
		expect(states(ev)).toEqual(ALL_OK);
		const reads = byId(ev).reads;
		expect(reads.cause).toBe('none');
		expect(reads.reasons).toEqual([]);
		expect(reads.notes).toEqual([
			expect.objectContaining({
				kind: 'note',
				text: 'GitHub API Requests is having an incident; we are not seeing errors calling it',
				href: 'https://stspg.io/m1ps7yrhp4n8',
			}),
		]);
	});

	it('treats too few calls to judge as no errors', () => {
		const ev = evaluate(observation({ server: server(counts(10, 9)) }), upstreams(githubApiIncident()));
		expect(states(ev)).toEqual(ALL_OK);
	});
});

describe('GitHub degraded and it reaches us', () => {
	it('says GitHub first, links its incident, and adds our error rate', () => {
		const ev = evaluate(observation({ server: server(counts(1000, 120)) }), upstreams(githubApiIncident('partial_outage')));
		expect(states(ev)).toEqual({ ...ALL_OK, reads: 'degraded', downloads: 'degraded', publishing: 'degraded', signin: 'degraded' });
		const downloads = byId(ev).downloads;
		expect(downloads.cause).toBe('upstream');
		expect(downloads.reasons[0]).toEqual({
			kind: 'upstream',
			text: 'GitHub API Requests is having an incident',
			href: 'https://stspg.io/m1ps7yrhp4n8',
			linkText: 'Incident across several services',
		});
		expect(downloads.reasons[1]).toMatchObject({
			kind: 'api-errors',
			text: "We are also seeing errors calling GitHub's API",
			detail: '12% of 1,000 calls failed in the last 5 minutes',
		});
	});

	it('an outage by our own error rate, with registry reads capped at degraded (cold caches only)', () => {
		const ev = evaluate(observation({ server: server(counts(1000, 800)) }), upstreams(githubApiIncident('major_outage')));
		expect(states(ev)).toEqual({ ...ALL_OK, reads: 'degraded', downloads: 'outage', publishing: 'outage', signin: 'outage' });
	});

	it('takes GitHub’s word for it when we cannot see our own error rate', () => {
		const ev = evaluate(
			observation({ apiStatus: { ok: true, status: 404, ms: 20 }, apiStatusMissing: true, server: undefined }),
			upstreams(githubApiIncident('major_outage')),
		);
		const c = byId(ev);
		expect(c.signin).toMatchObject({ state: 'outage', cause: 'upstream' });
		expect(c.reads).toMatchObject({ state: 'degraded', cause: 'upstream' });
		expect(c.signin.reasons).toHaveLength(1);
	});

	it('links GitHub’s status page when GitHub has not linked an incident to the component', () => {
		const ev = evaluate(observation({ server: undefined }), upstreams(snapshot('github', { 'Git Operations': 'degraded_performance' })));
		expect(byId(ev).publishing.reasons[0]).toEqual({
			kind: 'upstream',
			text: 'GitHub Git Operations reports degraded performance',
			href: 'https://www.githubstatus.com',
			linkText: 'GitHub status',
		});
	});
});

describe('GitHub Actions (customers publish from it; we never call it)', () => {
	const actions = (status: 'degraded_performance' | 'major_outage') =>
		snapshot('github', { Actions: status }, [{ name: 'Actions jobs are delayed', components: ['Actions'], url: 'https://stspg.io/actions' }]);

	it('degrades publishing only, attributed to GitHub, even with our error rate normal', () => {
		const ev = evaluate(observation(), upstreams(actions('degraded_performance')));
		expect(states(ev)).toEqual({ ...ALL_OK, publishing: 'degraded' });
		const publishing = byId(ev).publishing;
		expect(publishing.cause).toBe('upstream');
		expect(publishing.reasons).toEqual([
			{ kind: 'upstream', text: 'GitHub Actions is having an incident', href: 'https://stspg.io/actions', linkText: 'Actions jobs are delayed' },
		]);
	});

	it('a major Actions outage is a publishing outage', () => {
		expect(byId(evaluate(observation(), upstreams(actions('major_outage')))).publishing.state).toBe('outage');
	});
});

describe('errors calling GitHub with nothing reported upstream', () => {
	it('says "Errors calling GitHub\'s API", blaming neither', () => {
		const ev = evaluate(observation({ server: server(counts(400, 28)) }), upstreams());
		const c = byId(ev);
		for (const id of ['reads', 'downloads', 'publishing', 'signin'] as const) {
			expect(c[id].state).toBe('degraded');
			expect(c[id].cause).toBe('api-errors');
			expect(c[id].reasons).toEqual([
				{ kind: 'api-errors', text: "Errors calling GitHub's API", detail: '7% of 400 calls failed in the last 5 minutes' },
			]);
		}
		expect(c.website.state).toBe('operational');
		expect(c.billing.state).toBe('operational');
	});

	it('counts rate-limited calls, and goes to outage at half (reads capped)', () => {
		const ev = evaluate(observation({ server: server(counts(100, 10, 50)) }), upstreams());
		expect(states(ev)).toEqual({ ...ALL_OK, reads: 'degraded', downloads: 'outage', publishing: 'outage', signin: 'outage' });
		expect(byId(ev).signin.reasons[0]?.detail).toBe('60% of 100 calls failed in the last 5 minutes, 50 rate limited');
	});

	it('still says so when GitHub’s status page could not be read', () => {
		const gh = unavailable('github', 'https://www.githubstatus.com', '2026-09-27T14:30:00Z', 'timeout');
		const ev = evaluate(observation({ server: server(counts(400, 40)) }), upstreams(gh));
		expect(byId(ev).signin).toMatchObject({ state: 'degraded', cause: 'api-errors' });
	});

	it('and is operational when GitHub’s page cannot be read but our calls are fine', () => {
		const gh = unavailable('github', 'https://www.githubstatus.com', '2026-09-27T14:30:00Z', 'timeout');
		expect(states(evaluate(observation(), upstreams(gh)))).toEqual(ALL_OK);
	});

	it('Actions incident plus our own API errors: GitHub first, our errors after', () => {
		const gh = snapshot('github', { Actions: 'partial_outage' }, [{ name: 'Actions', components: ['Actions'] }]);
		const publishing = byId(evaluate(observation({ server: server(counts(400, 40)) }), upstreams(gh))).publishing;
		expect(publishing.cause).toBe('upstream');
		expect(publishing.reasons.map((r) => r.kind)).toEqual(['upstream']);
		expect(publishing.state).toBe('degraded');
	});
});

describe('billing and Stripe', () => {
	const stripeDown = snapshot('stripe', { API: 'partial_outage' }, [{ name: 'Elevated API errors', components: ['API'], url: 'https://status.stripe.com/x' }]);

	it('Stripe degraded but our calls are fine: operational with a note', () => {
		const billing = byId(evaluate(observation(), upstreams(undefined, stripeDown))).billing;
		expect(billing.state).toBe('operational');
		expect(billing.notes[0]?.text).toBe('Stripe is having an incident; we are not seeing errors calling it');
	});

	it('Stripe degraded and our calls failing: billing degraded, Stripe first', () => {
		const billing = byId(evaluate(observation({ server: server(undefined, counts(10, 3)) }), upstreams(undefined, stripeDown))).billing;
		expect(billing).toMatchObject({ state: 'degraded', cause: 'upstream' });
		expect(billing.reasons[0]).toMatchObject({ text: 'Stripe is having an incident', href: 'https://status.stripe.com/x' });
		expect(billing.reasons[1]?.text).toBe("We are also seeing errors calling Stripe's API");
	});

	it('Stripe degraded and Stripe not configured on the server: take Stripe’s word', () => {
		const billing = byId(evaluate(observation({ server: { ...server(), stripe: undefined } }), upstreams(undefined, stripeDown))).billing;
		expect(billing).toMatchObject({ state: 'degraded', cause: 'upstream' });
	});

	it('Stripe’s status unreadable and our calls failing: "Errors calling Stripe\'s API"', () => {
		const stripe = unavailable('stripe', 'https://status.stripe.com', '2026-09-27T14:30:00Z', 'not a Statuspage summary');
		const billing = byId(evaluate(observation({ server: server(undefined, counts(10, 8)) }), upstreams(undefined, stripe))).billing;
		expect(billing).toMatchObject({ state: 'outage', cause: 'api-errors' });
		expect(billing.reasons[0]?.text).toBe("Errors calling Stripe's API");
	});

	it('a GitHub outage does not touch billing', () => {
		expect(byId(evaluate(observation({ server: undefined }), upstreams(githubApiIncident('major_outage')))).billing.state).toBe('operational');
	});
});

describe('the dependency table, every combination', () => {
	// docs/trust-and-status.md §1, restated independently of src/model.ts.
	const DEPENDS: Record<ComponentId, string[]> = {
		reads: ['API Requests'],
		downloads: ['API Requests'],
		publishing: ['API Requests', 'Git Operations', 'Actions'],
		signin: ['API Requests'],
		website: [],
		billing: [],
	};
	const ourCalls = {
		normal: server(counts(1000, 2)),
		high: server(counts(1000, 100)),
		unknown: undefined,
	};
	for (const dep of ['API Requests', 'Git Operations', 'Actions', 'Webhooks']) {
		for (const [calls, srv] of Object.entries(ourCalls)) {
			it(`GitHub ${dep} degraded, our GitHub calls ${calls}`, () => {
				const gh = snapshot('github', { [dep]: 'degraded_performance' }, [{ name: `${dep} incident`, components: [dep] }]);
				const ev = evaluate(observation({ server: srv }), upstreams(gh));
				for (const c of ev.components) {
					const depends = DEPENDS[c.id].includes(dep);
					const usesGitHub = DEPENDS[c.id].length > 0;
					let expected: [string, string];
					if (depends && (dep === 'Actions' || calls === 'unknown')) expected = ['degraded', 'upstream'];
					else if (depends && calls === 'normal') expected = ['operational', 'none'];
					else if (depends) expected = ['degraded', 'upstream'];
					else if (usesGitHub && calls === 'high') expected = ['degraded', 'api-errors'];
					else expected = ['operational', 'none'];
					expect([c.state, c.cause], `${c.id}`).toEqual(expected);
					if (expected[1] === 'upstream') {
						expect(c.reasons[0]).toMatchObject({ kind: 'upstream', text: `GitHub ${dep} is having an incident` });
					}
					if (depends && expected[0] === 'operational') expect(c.notes).toHaveLength(1);
				}
			});
		}
	}
});

describe('our own incidents', () => {
	const incident = (over: Partial<ActiveIncident> = {}): ActiveIncident => ({
		id: 'x',
		title: 'Publishing failed for some organisations',
		components: ['publishing'],
		impact: 'degraded',
		href: '/#incident-x',
		...over,
	});

	it('raise their components to at least their impact, linked to the write-up', () => {
		const ev = evaluate(observation(), upstreams(), [incident()]);
		expect(states(ev)).toEqual({ ...ALL_OK, publishing: 'degraded' });
		expect(byId(ev).publishing).toMatchObject({
			cause: 'incident',
			reasons: [{ kind: 'incident', text: 'See our incident report', href: '/#incident-x', linkText: 'Publishing failed for some organisations' }],
		});
	});

	it('an outage impact is an outage', () => {
		expect(byId(evaluate(observation(), upstreams(), [incident({ impact: 'outage' })])).publishing.state).toBe('outage');
	});

	it('keep the upstream attribution first when there is one', () => {
		const gh = snapshot('github', { Actions: 'degraded_performance' }, [{ name: 'Actions', components: ['Actions'] }]);
		const publishing = byId(evaluate(observation(), upstreams(gh), [incident()])).publishing;
		expect(publishing.cause).toBe('upstream');
		expect(publishing.reasons.map((r) => r.kind)).toEqual(['upstream', 'incident']);
	});
});
