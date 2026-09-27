/**
 * status.privatecrates.dev. The Cron Trigger probes every environment each minute and records the result in D1;
 * the fetch handler renders pages from D1 and the incidents bundled at build time.
 */
import { EmailMessage } from 'cloudflare:email';
import { alertEnvironment, alertTest, readAlerter, type AlertEnv, type Alerter } from './alerts.ts';
import { evaluate } from './attribution.ts';
import { readConfig, type Config } from './config.ts';
import generatedIncidents from './generated/incidents.json';
import { dayOf, epochMinute, lastDays, toMinuteRecord } from './history.ts';
import { activeIncidents, forEnvironment, type Incident } from './incidents.ts';
import type { Environment } from './model.ts';
import { fetchUpstreams, observe, type Fetch } from './probes.ts';
import { renderFeed } from './render/feed.ts';
import { renderPage } from './render/page.ts';
import { renderSummary } from './render/summary.ts';
import { loadDays, loadLatest, pruneOld, recordMinute } from './store.ts';

const INCIDENTS = generatedIncidents as Incident[];

/**
 * One probe run: every environment, sharing one fetch of each upstream status page. With an `alerter`, it then pages or
 * emails what changed (src/alerting.ts); a failure there is logged and never fails the run.
 */
export async function runChecks(
	config: Config,
	db: D1Database,
	scheduledMs: number,
	fetcher: Fetch,
	incidents: readonly Incident[] = INCIDENTS,
	now: () => number = Date.now,
	alerter?: Alerter,
): Promise<void> {
	const fetchedAt = new Date(now()).toISOString();
	const upstreams = await fetchUpstreams(config.upstream, fetcher, config.timeoutMs, fetchedAt);
	await Promise.all(
		config.environments.map(async (env) => {
			const obs = await observe(env, fetcher, config.timeoutMs, new Date(now()).toISOString());
			const active = activeIncidents(forEnvironment(incidents, env.id), now(), env.path);
			const evaluation = evaluate(obs, upstreams, active);
			const record = toMinuteRecord(scheduledMs, obs, evaluation);
			await recordMinute(db, env.id, dayOf(scheduledMs), record, evaluation, upstreams);
			if (alerter) {
				try {
					await alertEnvironment(alerter, db, env, evaluation, scheduledMs);
				} catch (e) {
					console.error(`alert: ${env.id}: ${(e as Error)?.message ?? String(e)}`);
				}
			}
		}),
	);
	// Hourly is plenty for a 90-day window.
	if (epochMinute(scheduledMs) % 60 === 0) {
		await pruneOld(
			db,
			config.environments.map((e) => e.id),
			scheduledMs,
		);
	}
}

const CSP = [
	"default-src 'none'",
	"script-src 'self'",
	"style-src 'self'",
	"font-src 'self'",
	"img-src 'self'",
	"base-uri 'none'",
	"form-action 'none'",
	"frame-ancestors 'none'",
].join('; ');

export const SECURITY_HEADERS: Record<string, string> = {
	'content-security-policy': CSP,
	'x-content-type-options': 'nosniff',
	'referrer-policy': 'no-referrer',
	'strict-transport-security': 'max-age=31536000',
	'cross-origin-opener-policy': 'same-origin',
	'cross-origin-resource-policy': 'same-origin',
	'permissions-policy': 'camera=(), microphone=(), geolocation=(), interest-cohort=()',
};

function respond(body: string, type: string, status = 200, extra: Record<string, string> = {}): Response {
	return new Response(body, {
		status,
		headers: { ...SECURITY_HEADERS, 'content-type': type, 'cache-control': 'public, max-age=30', ...extra },
	});
}

function notFound(): Response {
	return respond(
		'<!doctype html><html lang="en-GB"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1"><title>Not found: PrivateCrates status</title><link rel="stylesheet" href="/status.css"></head><body><main id="main" class="page"><h1>Not found</h1><p><a href="/">PrivateCrates status</a></p></main></body></html>',
		'text/html; charset=utf-8',
		404,
	);
}

type Route = { env: Environment; kind: 'page' | 'feed' | 'summary' } | { redirect: string } | undefined;

export function route(pathname: string, environments: readonly Environment[]): Route {
	for (const env of environments) {
		const base = env.path === '/' ? '' : env.path;
		if (pathname === (base || '/')) return { env, kind: 'page' };
		if (base && pathname === `${base}/`) return { redirect: base };
		if (pathname === `${base}/feed.xml`) return { env, kind: 'feed' };
		if (pathname === `${base}/api/summary.json`) return { env, kind: 'summary' };
	}
	return undefined;
}

export async function handle(
	request: Request,
	config: Config,
	db: D1Database,
	incidents: readonly Incident[] = INCIDENTS,
	nowMs = Date.now(),
): Promise<Response> {
	if (request.method !== 'GET' && request.method !== 'HEAD') {
		return new Response('Method not allowed', { status: 405, headers: { ...SECURITY_HEADERS, allow: 'GET, HEAD' } });
	}
	const url = new URL(request.url);
	const found = route(url.pathname, config.environments);
	if (!found) return notFound();
	if ('redirect' in found) return new Response(null, { status: 301, headers: { location: found.redirect } });

	const { env, kind } = found;
	const envIncidents = forEnvironment(incidents, env.id);
	if (kind === 'feed') {
		return respond(renderFeed(env, envIncidents, url.origin, nowMs), 'application/atom+xml; charset=utf-8');
	}
	const latest = await loadLatest(db, env.id);
	if (kind === 'summary') {
		return respond(
			JSON.stringify(renderSummary(env, latest, envIncidents, url.origin, nowMs), null, 2),
			'application/json; charset=utf-8',
			200,
			{ 'access-control-allow-origin': '*' },
		);
	}
	const days = await loadDays(db, env.id, lastDays(nowMs)[0]!);
	const html = renderPage({
		env,
		environments: config.environments,
		nowMs,
		latest,
		days,
		incidents: envIncidents,
	});
	return respond(html, 'text/html; charset=utf-8');
}

export default {
	async fetch(request, env): Promise<Response> {
		return handle(request, readConfig(env), env.DB);
	},
	async scheduled(controller, env): Promise<void> {
		const fetcher: Fetch = (input, init) => fetch(input, init);
		const vars = env as Env & AlertEnv;
		let alerter: Alerter | undefined;
		try {
			alerter = readAlerter(vars, (from, to, raw) => new EmailMessage(from, to, raw), fetcher);
		} catch (e) {
			console.error(`alert: configuration: ${(e as Error)?.message ?? String(e)}`);
		}
		const test = vars.ALERT_TEST?.trim();
		await Promise.all([
			runChecks(readConfig(env), env.DB, controller.scheduledTime, fetcher, INCIDENTS, Date.now, alerter),
			test
				? alertTest(alerter, env.DB, test, controller.scheduledTime).catch((e: unknown) =>
						console.error(`alert test: ${(e as Error)?.message ?? String(e)}`),
					)
				: undefined,
		]);
	},
} satisfies ExportedHandler<Env>;
