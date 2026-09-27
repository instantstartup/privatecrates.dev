import { describe, expect, it } from 'vitest';
import { evaluate } from '../src/attribution.ts';
import {
	componentStatus,
	describeUpstreamStatus,
	incidentFor,
	parseSummary,
	upstreamState,
} from '../src/statuspage.ts';
import { fixture, observation, snapshot } from './helpers.ts';

// Captured from https://www.githubstatus.com/api/v2/summary.json on 27 September 2026 (all operational), and one
// real incident from https://www.githubstatus.com/api/v2/incidents.json (23 September 2026, API Requests).
const summary = fixture('githubstatus-summary.json') as Record<string, unknown>;
const incident = fixture('githubstatus-incident.json') as Record<string, unknown>;
const PAGE = 'https://www.githubstatus.com';
const AT = '2026-09-27T14:30:00.000Z';

/** The captured summary with the captured incident reopened and API Requests degraded, as during that incident. */
function duringIncident(over: Record<string, unknown> = {}) {
	const body = structuredClone(summary) as { components: { name: string; status: string }[]; incidents: unknown[] };
	for (const c of body.components) if (c.name === 'API Requests') c.status = 'degraded_performance';
	body.incidents = [{ ...structuredClone(incident), status: 'investigating', resolved_at: null, ...over }];
	return body;
}

describe('parsing GitHub’s Statuspage summary', () => {
	it('reads the captured sample', () => {
		const snap = parseSummary('github', PAGE, AT, summary);
		expect(snap.available).toBe(true);
		expect(snap.description).toBe('All Systems Operational');
		expect(snap.incidents).toEqual([]);
		for (const name of ['API Requests', 'Git Operations', 'Webhooks', 'Actions']) {
			expect(componentStatus(snap, name)).toBe('operational');
		}
		expect(componentStatus(snap, 'Nonexistent')).toBeUndefined();
		expect(componentStatus(snap, '*')).toBe('operational');
	});

	it('finds the incident for a degraded component and links its short link', () => {
		const snap = parseSummary('github', PAGE, AT, duringIncident());
		expect(componentStatus(snap, 'API Requests')).toBe('degraded_performance');
		const found = incidentFor(snap, 'API Requests');
		expect(found).toMatchObject({
			name: 'Incident across several services',
			url: 'https://stspg.io/m1ps7yrhp4n8',
			components: ['API Requests'],
			status: 'investigating',
		});
		expect(incidentFor(snap, 'Actions')).toBeUndefined();
	});

	it('falls back to the components named in the incident’s updates', () => {
		const snap = parseSummary('github', PAGE, AT, duringIncident({ components: [] }));
		expect(incidentFor(snap, 'API Requests')?.url).toBe('https://stspg.io/m1ps7yrhp4n8');
	});

	it('leaves out resolved incidents', () => {
		const body = duringIncident({ status: 'resolved' });
		expect(parseSummary('github', PAGE, AT, body).incidents).toEqual([]);
	});

	it('only keeps https links', () => {
		const snap = parseSummary('github', PAGE, AT, duringIncident({ shortlink: 'javascript:alert(1)' }));
		expect(snap.incidents[0]?.url).toBe(PAGE);
	});

	it('drives attribution end to end', () => {
		const github = parseSummary('github', PAGE, AT, duringIncident());
		const ev = evaluate(observation({ server: undefined }), { github, stripe: snapshot('stripe') });
		const signin = ev.components.find((c) => c.id === 'signin');
		expect(signin).toMatchObject({ state: 'degraded', cause: 'upstream' });
		expect(signin?.reasons[0]?.href).toBe('https://stspg.io/m1ps7yrhp4n8');
	});
});

describe('bodies that are not a Statuspage summary', () => {
	it('Stripe’s legacy /current JSON (captured 27 September 2026, frozen since February 2024) is unavailable', () => {
		const snap = parseSummary('stripe', 'https://status.stripe.com', AT, fixture('stripe-current.json'));
		expect(snap).toMatchObject({ available: false, error: 'not a Statuspage summary', components: [], incidents: [] });
		expect(componentStatus(snap, '*')).toBeUndefined();
	});

	it('HTML, null and junk are unavailable', () => {
		for (const body of [null, 'html', 42, { components: 'x' }, []]) {
			expect(parseSummary('github', PAGE, AT, body).available).toBe(false);
		}
	});
});

describe('status mapping', () => {
	it('maps Statuspage statuses onto ours', () => {
		expect(upstreamState('operational')).toBe('operational');
		expect(upstreamState('degraded_performance')).toBe('degraded');
		expect(upstreamState('partial_outage')).toBe('degraded');
		expect(upstreamState('under_maintenance')).toBe('degraded');
		expect(upstreamState('major_outage')).toBe('outage');
		expect(describeUpstreamStatus('partial_outage')).toBe('a partial outage');
	});

	it('the provider as a whole takes its worst component', () => {
		const snap = snapshot('stripe', { API: 'degraded_performance', Checkout: 'major_outage' });
		expect(componentStatus(snap, '*')).toBe('major_outage');
	});
});
