import { describe, expect, it } from 'vitest';
import { evaluate } from '../src/attribution.ts';
import { parseEnvironments, readConfig, type Config } from '../src/config.ts';
import { dayOf, toMinuteRecord } from '../src/history.ts';
import { handle, route, runChecks } from '../src/index.ts';
import type { Incident } from '../src/incidents.ts';
import type { Fetch } from '../src/probes.ts';
import { recordMinute } from '../src/store.ts';
import { renderPage } from '../src/render/page.ts';
import { fixture, memoryD1, observation, snapshot, upstreams } from './helpers.ts';

const NOW = Date.parse('2026-09-27T14:30:30Z');

const config: Config = readConfig({
	ENVIRONMENTS: [
		{ id: 'production', name: 'Production', apex: 'privatecrates.dev', canary: 'canary', path: '/' },
		{ id: 'dev', name: 'Development', apex: 'dev.privatecrates.dev', canary: 'canary', path: '/dev' },
	],
	GITHUB_STATUS_URL: 'https://www.githubstatus.com/api/v2/summary.json',
	GITHUB_STATUS_PAGE: 'https://www.githubstatus.com',
	STRIPE_STATUS_URL: 'https://status.stripe.com/api/v2/summary.json',
	STRIPE_STATUS_PAGE: 'https://status.stripe.com',
	PROBE_TIMEOUT_MS: '10000',
});

const INCIDENTS: Incident[] = [
	{
		id: '2026-09-27-publish',
		title: 'Publishing is slow <for some>',
		environment: 'production',
		start: '2026-09-27T14:00:00.000Z',
		impact: 'degraded',
		components: ['publishing'],
		html: '',
		updates: [{ at: '2026-09-27T14:05:00.000Z', status: 'investigating', html: '<p>Looking.</p>' }],
	},
	{
		id: '2026-09-01-signin',
		title: 'Sign-in failed',
		environment: 'production',
		start: '2026-09-01T09:00:00.000Z',
		end: '2026-09-01T09:40:00.000Z',
		impact: 'outage',
		components: ['signin'],
		html: '<p>Write-up.</p>',
		updates: [],
	},
	{
		id: '2026-09-02-dev',
		title: 'Dev was down',
		environment: 'dev',
		start: '2026-09-02T09:00:00.000Z',
		end: '2026-09-02T10:00:00.000Z',
		impact: 'outage',
		components: ['website'],
		html: '',
		updates: [],
	},
];

async function seeded(ev = evaluate(observation(), upstreams())) {
	const db = memoryD1();
	const t = Date.parse(ev.checkedAt);
	await recordMinute(db, 'production', dayOf(t), toMinuteRecord(t, observation(), ev), ev, upstreams());
	return db;
}

async function get(path: string, db: D1Database, incidents = INCIDENTS) {
	return handle(new Request(`https://status.privatecrates.dev${path}`), config, db, incidents, NOW);
}

/** Checks that hold for every page: structure, accessibility hooks, and nothing third-party or inline. */
function checkPage(html: string) {
	expect(html).toMatch(/^<!doctype html>\n<html lang="en-GB">/);
	for (const landmark of ['<header', '<nav aria-label="Environments">', '<main id="main"', '<footer']) expect(html).toContain(landmark);
	expect(html.match(/<h1>/g)).toHaveLength(1);
	expect(html).toContain('<a class="skip" href="#main">');
	// 6 components × 90 days, each with a text alternative.
	expect(html.match(/class="bar bar-/g)).toHaveLength(540);
	expect(html.match(/role="img" aria-label="[^"]+"/g)).toHaveLength(540);
	// No inline script or style, and every script, stylesheet and font is same-origin.
	expect(html).not.toMatch(/<script>|<script(?![^>]*\bsrc=")|style="/);
	expect(html).not.toMatch(/<(script|link|img)[^>]+(src|href)="https?:/);
}

describe('pages', () => {
	it('production, healthy: an operational banner, last checked, links to GitHub and Stripe', async () => {
		const res = await get('/', await seeded(), []);
		expect(res.status).toBe(200);
		expect(res.headers.get('content-type')).toBe('text/html; charset=utf-8');
		expect(res.headers.get('content-security-policy')).toContain("default-src 'none'");
		expect(res.headers.get('content-security-policy')).toContain("script-src 'self'");
		const html = await res.text();
		checkPage(html);
		expect(html).toContain('All systems operational');
		expect(html).toContain('Last checked <time datetime="2026-09-27T14:30:00.000Z">27 Sep 2026, 14:30 UTC</time>');
		expect(html).toContain('<a href="https://www.githubstatus.com/">GitHub\'s status page</a>');
		expect(html).toContain('<a href="https://status.stripe.com/">Stripe\'s status page</a>');
		expect(html).toContain('No incidents in the last 90 days.');
		expect(html).toContain('27 Sep 2026, Registry reads: no problems seen. 100% available.');
		expect(html).toContain('26 Sep 2026, Registry reads: no data.');
		expect(html).not.toContain('Development environment');
	});

	it('says what is degraded, whose fault, and what is unaffected', async () => {
		const gh = snapshot('github', { Actions: 'degraded_performance' }, [
			{ name: 'Actions jobs are delayed', components: ['Actions'], url: 'https://stspg.io/actions' },
		]);
		const db = await seeded(evaluate(observation(), upstreams(gh)));
		const html = await (await get('/', db, [])).text();
		checkPage(html);
		expect(html).toContain('Some systems are degraded');
		expect(html).toContain(
			'<li><strong>Publishing degraded</strong>: GitHub Actions is having an incident: <a href="https://stspg.io/actions">Actions jobs are delayed</a>.</li>',
		);
		expect(html).toContain('<p>Registry reads, Downloads, Sign-in, Website and account and Billing are unaffected.</p>');
		expect(html).toContain('<span class="pill pill-degraded">');
		expect(html).toContain('27 Sep 2026, Publishing: degraded for 1 minute. GitHub Actions is having an incident.');
	});

	it('shows ongoing and past incidents, escaped', async () => {
		const html = await (await get('/', await seeded())).text();
		expect(html).toContain('<h2 id="active-incidents">Ongoing incidents</h2>');
		expect(html).toContain('Publishing is slow &lt;for some&gt;');
		expect(html).toContain('id="incident-2026-09-01-signin"');
		expect(html).toContain('<p>Write-up.</p>');
		expect(html).not.toContain('Dev was down');
		expect(html).toContain('1 Sep 2026, Sign-in: no data. Incident: Sign-in failed.');
	});

	it('before the first check, says so', async () => {
		const html = await (await get('/', memoryD1(), [])).text();
		checkPage(html);
		expect(html).toContain('Waiting for the first check');
	});

	it('says the checks are delayed when the latest is old', () => {
		const ev = evaluate(observation(), upstreams());
		const html = renderPage({
			env: config.environments[0]!,
			environments: config.environments,
			nowMs: NOW + 10 * 60_000,
			latest: { checkedAt: ev.checkedAt, evaluation: ev, upstreams: upstreams() },
			days: new Map(),
			incidents: [],
		});
		expect(html).toContain('Our checks are delayed');
	});

	it('/dev is marked as the development environment, with its own incidents', async () => {
		const res = await get('/dev', memoryD1());
		const html = await res.text();
		checkPage(html);
		expect(html).toContain('<h1>Development status</h1>');
		expect(html).toContain('<strong>Development environment.</strong>');
		expect(html).toContain('<meta name="robots" content="noindex">');
		expect(html).toContain('<a href="/dev" aria-current="page">Development</a>');
		expect(html).toContain('Dev was down');
		expect(html).not.toContain('Publishing is slow');
		expect(html).toContain('href="/dev/feed.xml"');
	});

	it('routes, redirects and refuses', async () => {
		const db = memoryD1();
		expect((await get('/dev/', db)).headers.get('location')).toBe('/dev');
		expect((await get('/nope', db)).status).toBe(404);
		const post = await handle(new Request('https://status.privatecrates.dev/', { method: 'POST' }), config, db, [], NOW);
		expect(post.status).toBe(405);
		expect(route('/api/summary.json', config.environments)).toMatchObject({ kind: 'summary', env: { id: 'production' } });
		expect(route('/dev/api/summary.json', config.environments)).toMatchObject({ kind: 'summary', env: { id: 'dev' } });
	});
});

describe('/feed.xml', () => {
	it('is Atom with one entry per incident of that environment', async () => {
		const res = await get('/feed.xml', memoryD1());
		expect(res.headers.get('content-type')).toBe('application/atom+xml; charset=utf-8');
		const xml = await res.text();
		expect(xml).toMatch(/^<\?xml version="1.0" encoding="utf-8"\?>\n<feed xmlns="http:\/\/www.w3.org\/2005\/Atom"/);
		expect(xml.match(/<entry>/g)).toHaveLength(2);
		expect(xml).toContain('<id>https://status.privatecrates.dev/#incident-2026-09-27-publish</id>');
		expect(xml).toContain('<title>Publishing is slow &lt;for some&gt;</title>');
		expect(xml).toContain('<updated>2026-09-27T14:05:00.000Z</updated>');
		expect(xml).toContain('&lt;p&gt;Looking.&lt;/p&gt;');
		expect(xml).not.toMatch(/<(?!\/?(feed|title|id|link|updated|author|name|uri|entry|published|category|content)\b|\?xml)/);
	});

	it('dev has its own feed; an empty feed is still valid', async () => {
		const xml = await (await get('/dev/feed.xml', memoryD1())).text();
		expect(xml.match(/<entry>/g)).toHaveLength(1);
		const empty = await (await get('/feed.xml', memoryD1(), [])).text();
		expect(empty).toContain('<updated>2026-09-27T00:00:00.000Z</updated>');
		expect(empty).not.toContain('<entry>');
	});
});

describe('/api/summary.json', () => {
	it('gives the current state, machine-readable', async () => {
		const gh = snapshot('github', { 'API Requests': 'major_outage' }, [{ name: 'API down', components: ['API Requests'], url: 'https://stspg.io/api' }]);
		const db = await seeded(evaluate(observation({ server: undefined }), upstreams(gh)));
		const res = await get('/api/summary.json', db);
		expect(res.headers.get('access-control-allow-origin')).toBe('*');
		const body = (await res.json()) as Record<string, any>;
		expect(body).toMatchObject({
			environment: { id: 'production', apex: 'privatecrates.dev' },
			page: 'https://status.privatecrates.dev/',
			checked_at: '2026-09-27T14:30:00.000Z',
			stale: false,
			state: 'outage',
		});
		const signin = body.components.find((c: { id: string }) => c.id === 'signin');
		expect(signin).toMatchObject({ state: 'outage', cause: 'upstream' });
		expect(signin.reasons[0]).toMatchObject({ kind: 'upstream', url: 'https://stspg.io/api', link_text: 'API down' });
		expect(body.incidents[0]).toMatchObject({ id: '2026-09-27-publish', active: true, url: 'https://status.privatecrates.dev/#incident-2026-09-27-publish' });
	});

	it('before any check: no state, stale', async () => {
		const body = (await (await get('/dev/api/summary.json', memoryD1(), [])).json()) as Record<string, unknown>;
		expect(body).toMatchObject({ state: null, stale: true, components: [] });
	});
});

describe('a whole cron run', () => {
	it('probes both environments, shares one upstream fetch, and serves the result', async () => {
		const calls: string[] = [];
		const fetcher: Fetch = async (url) => {
			calls.push(url);
			if (url.includes('githubstatus')) return Response.json(fixture('githubstatus-summary.json'));
			if (url.includes('stripe')) return new Response('not found', { status: 404 });
			if (url.endsWith('/healthz')) return new Response('ok');
			if (url.endsWith('/api/status')) return new Response('not found', { status: 404 });
			if (url.endsWith('/index/config.json')) {
				// Dev's canary is broken.
				return url.includes('.dev.privatecrates.dev')
					? new Response('', { status: 502 })
					: new Response('', { status: 401, headers: { 'www-authenticate': 'Cargo' } });
			}
			throw new Error(url);
		};
		const db = memoryD1();
		const scheduled = Date.parse('2026-09-27T14:00:00Z');
		await runChecks(config, db, scheduled, fetcher, INCIDENTS, () => scheduled + 1500);
		expect(calls.filter((u) => u.includes('githubstatus'))).toHaveLength(1);
		expect(calls.filter((u) => u.includes('stripe'))).toHaveLength(1);

		const prod = (await (await handle(new Request('https://status.privatecrates.dev/api/summary.json'), config, db, INCIDENTS, scheduled + 60_000)).json()) as Record<string, any>;
		// The active incident marks publishing; everything else is fine.
		expect(prod.state).toBe('degraded');
		expect(prod.components.find((c: { id: string }) => c.id === 'publishing')).toMatchObject({ state: 'degraded', cause: 'incident' });
		expect(prod.upstream.stripe).toMatchObject({ available: false, status_page: 'https://status.stripe.com' });

		const dev = (await (await handle(new Request('https://status.privatecrates.dev/dev/api/summary.json'), config, db, INCIDENTS, scheduled + 60_000)).json()) as Record<string, any>;
		expect(dev.state).toBe('outage');
		expect(dev.components.find((c: { id: string }) => c.id === 'reads')).toMatchObject({ state: 'outage', cause: 'ours' });

		const html = await (await handle(new Request('https://status.privatecrates.dev/dev'), config, db, INCIDENTS, scheduled + 60_000)).text();
		expect(html).toContain('<strong>Registry reads down</strong>: The registry is not answering Cargo');
		expect(html).toContain('We could not read its status automatically');
	});
});

describe('configuration', () => {
	it('accepts the wrangler.jsonc environments and rejects mistakes', () => {
		expect(config.environments.map((e) => e.path)).toEqual(['/', '/dev']);
		expect(config.timeoutMs).toBe(10_000);
		expect(() => parseEnvironments([])).toThrow();
		expect(() => parseEnvironments([{ id: 'dev', name: 'D', apex: 'dev.privatecrates.dev', canary: 'canary', path: '/dev' }])).toThrow(/path \//);
		expect(() => parseEnvironments([{ id: 'p', name: 'P', apex: 'not a host', canary: 'canary', path: '/' }])).toThrow(/apex/);
		expect(parseEnvironments(JSON.stringify([{ id: 'p', name: 'P', apex: 'privatecrates.dev', canary: 'canary', path: '/' }]))).toHaveLength(1);
	});
});
