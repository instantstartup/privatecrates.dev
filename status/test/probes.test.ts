import { describe, expect, it } from 'vitest';
import { fetchUpstreams, observe, type Fetch } from '../src/probes.ts';
import type { Environment } from '../src/model.ts';
import { fixture } from './helpers.ts';

const ENV: Environment = { id: 'production', name: 'Production', apex: 'privatecrates.dev', canary: 'canary', path: '/' };
const AT = '2026-09-27T14:30:00.000Z';

const SERVER_STATUS = {
	version: '0.2.0',
	started_at: '2026-09-27T14:13:23Z',
	github: { window_seconds: 300, requests: 1840, errors: 3, rate_limited: 0, latency_ms_p50: 180, latency_ms_p95: 640 },
	stripe: { configured: true, requests: 12, errors: 0 },
	tenants: 14,
};

type Handler = (url: string) => Response | Promise<Response>;

function fakeFetch(routes: Record<string, Handler>): Fetch & { calls: string[] } {
	const calls: string[] = [];
	const f = (async (url: string) => {
		calls.push(url);
		const handler = routes[url];
		if (!handler) throw new TypeError('network error');
		return handler(url);
	}) as Fetch & { calls: string[] };
	f.calls = calls;
	return f;
}

const healthy: Record<string, Handler> = {
	'https://privatecrates.dev/healthz': () => new Response('ok'),
	'https://privatecrates.dev/api/status': () => Response.json(SERVER_STATUS),
	'https://canary.privatecrates.dev/index/config.json': () =>
		new Response('unauthorised', { status: 401, headers: { 'www-authenticate': 'Cargo login_url="https://privatecrates.dev/account"' } }),
};

describe('our probes', () => {
	it('all pass against a healthy server, and read its GitHub counts', async () => {
		const obs = await observe(ENV, fakeFetch(healthy), 1000, AT);
		expect(obs.healthz).toMatchObject({ ok: true, status: 200 });
		expect(obs.canary).toMatchObject({ ok: true, status: 401 });
		expect(obs.apiStatusMissing).toBe(false);
		expect(obs.server?.github).toEqual({ windowSeconds: 300, requests: 1840, errors: 3, rateLimited: 0 });
		expect(obs.server?.stripe).toEqual({ windowSeconds: 300, requests: 12, errors: 0, rateLimited: 0 });
	});

	it('a canary 401 without WWW-Authenticate fails', async () => {
		const obs = await observe(
			ENV,
			fakeFetch({ ...healthy, 'https://canary.privatecrates.dev/index/config.json': () => new Response('', { status: 401 }) }),
			1000,
			AT,
		);
		expect(obs.canary).toMatchObject({ ok: false, status: 401, error: 'no WWW-Authenticate header' });
	});

	it('a canary 200 (no auth at all) fails', async () => {
		const obs = await observe(ENV, fakeFetch({ ...healthy, 'https://canary.privatecrates.dev/index/config.json': () => new Response('{}') }), 1000, AT);
		expect(obs.canary).toMatchObject({ ok: false, error: 'HTTP 200, expected 401' });
	});

	it('retries once, so a single dropped connection is not a failure', async () => {
		let n = 0;
		const fetcher = fakeFetch({
			...healthy,
			'https://privatecrates.dev/healthz': () => (n++ === 0 ? new Response('', { status: 502 }) : new Response('ok')),
		});
		const obs = await observe(ENV, fetcher, 1000, AT);
		expect(obs.healthz.ok).toBe(true);
		expect(fetcher.calls.filter((u) => u.endsWith('/healthz'))).toHaveLength(2);
	});

	it('times out', async () => {
		const hang: Handler = () => new Promise(() => {});
		const fetcher: Fetch = (url, init) =>
			url.endsWith('/healthz')
				? new Promise((_, reject) => init?.signal?.addEventListener('abort', () => reject(init.signal?.reason)))
				: fakeFetch({ ...healthy, 'x': hang })(url, init);
		const obs = await observe(ENV, fetcher, 20, AT);
		expect(obs.healthz).toMatchObject({ ok: false, status: 0, error: 'timeout' });
	});

	it('network errors are failures with no status', async () => {
		const obs = await observe(ENV, fakeFetch({}), 1000, AT);
		expect(obs.healthz).toMatchObject({ ok: false, status: 0, error: 'network error' });
	});

	it('a 404 or the website’s HTML fallback on /api/status is "missing", not a failure', async () => {
		for (const res of [() => new Response('nope', { status: 404 }), () => new Response('<!doctype html>', { headers: { 'content-type': 'text/html' } })]) {
			const obs = await observe(ENV, fakeFetch({ ...healthy, 'https://privatecrates.dev/api/status': res }), 1000, AT);
			expect(obs.apiStatus.ok).toBe(true);
			expect(obs.apiStatusMissing).toBe(true);
			expect(obs.server).toBeUndefined();
		}
	});

	it('a 500 on /api/status is a failure', async () => {
		const obs = await observe(ENV, fakeFetch({ ...healthy, 'https://privatecrates.dev/api/status': () => new Response('', { status: 500 }) }), 1000, AT);
		expect(obs.apiStatus).toMatchObject({ ok: false, error: 'HTTP 500' });
		expect(obs.apiStatusMissing).toBe(false);
	});
});

describe('upstream status pages', () => {
	const urls = {
		github: { api: 'https://www.githubstatus.com/api/v2/summary.json', page: 'https://www.githubstatus.com' },
		stripe: { api: 'https://status.stripe.com/api/v2/summary.json', page: 'https://status.stripe.com' },
	};

	it('fetches both once, and copes with Stripe’s missing Statuspage API (404 today)', async () => {
		const fetcher = fakeFetch({
			[urls.github.api]: () => Response.json(fixture('githubstatus-summary.json')),
			[urls.stripe.api]: () => new Response('<!DOCTYPE html>', { status: 404 }),
		});
		const up = await fetchUpstreams(urls, fetcher, 1000, AT);
		expect(up.github?.available).toBe(true);
		expect(up.stripe).toMatchObject({ available: false, error: 'HTTP 404', pageUrl: 'https://status.stripe.com' });
		expect(fetcher.calls).toHaveLength(2);
	});
});
