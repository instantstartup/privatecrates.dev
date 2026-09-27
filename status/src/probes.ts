/**
 * The probes: our three per environment, and the two upstream status pages. Each has a timeout, and each of ours is
 * retried once on failure, so a single dropped connection does not paint a day's bar.
 */
import type { Observation, ProbeResult, Upstreams } from './attribution.ts';
import type { Environment, Provider } from './model.ts';
import { parseServerStatus, type ServerStatus } from './server-status.ts';
import { parseSummary, unavailable, type UpstreamSnapshot } from './statuspage.ts';

export type Fetch = (input: string, init?: RequestInit) => Promise<Response>;

const USER_AGENT = 'privatecrates-status/1 (+https://status.privatecrates.dev)';

interface Attempt {
	result: ProbeResult;
	/** The parsed JSON body, when asked for. */
	json?: unknown;
}

async function attempt(
	fetcher: Fetch,
	url: string,
	timeoutMs: number,
	check: (res: Response) => string | undefined,
	wantJson: boolean,
): Promise<Attempt> {
	const started = Date.now();
	try {
		const res = await fetcher(url, {
			headers: { 'user-agent': USER_AGENT, accept: wantJson ? 'application/json' : '*/*' },
			redirect: 'manual',
			signal: AbortSignal.timeout(timeoutMs),
		});
		let json: unknown;
		if (wantJson && res.ok) {
			try {
				json = await res.json();
			} catch {
				json = undefined;
			}
		} else {
			await res.body?.cancel();
		}
		const error = check(res);
		return { result: { ok: !error, status: res.status, ms: Date.now() - started, error }, json };
	} catch (e) {
		const name = (e as Error).name;
		const error = name === 'TimeoutError' || name === 'AbortError' ? 'timeout' : 'network error';
		return { result: { ok: false, status: 0, ms: Date.now() - started, error } };
	}
}

async function withRetry(run: () => Promise<Attempt>): Promise<Attempt> {
	const first = await run();
	return first.result.ok ? first : run();
}

const expectOk = (res: Response) => (res.status === 200 ? undefined : `HTTP ${res.status}`);

const expectCargo401 = (res: Response) => {
	if (res.status !== 401) return `HTTP ${res.status}, expected 401`;
	return res.headers.has('www-authenticate') ? undefined : 'no WWW-Authenticate header';
};

export async function observe(
	env: Environment,
	fetcher: Fetch,
	timeoutMs: number,
	checkedAt: string,
): Promise<Observation> {
	const apex = `https://${env.apex}`;
	const [healthz, status, canary] = await Promise.all([
		withRetry(() => attempt(fetcher, `${apex}/healthz`, timeoutMs, expectOk, false)),
		withRetry(() =>
			attempt(
				fetcher,
				`${apex}/api/status`,
				timeoutMs,
				// 404 is "missing", judged below, not a failure.
				(res) => (res.status === 200 || res.status === 404 ? undefined : `HTTP ${res.status}`),
				true,
			),
		),
		withRetry(() =>
			attempt(fetcher, `https://${env.canary}.${env.apex}/index/config.json`, timeoutMs, expectCargo401, false),
		),
	]);

	let server: ServerStatus | undefined;
	let apiStatusMissing = false;
	if (status.result.ok) {
		server = status.result.status === 200 ? parseServerStatus(status.json) : undefined;
		// A 404, or a 200 that is not our JSON (the website's fallback page): the server predates /api/status.
		apiStatusMissing = !server;
	}
	return { checkedAt, healthz: healthz.result, apiStatus: status.result, apiStatusMissing, canary: canary.result, server };
}

export async function fetchUpstream(
	provider: Provider,
	url: string,
	pageUrl: string,
	fetcher: Fetch,
	timeoutMs: number,
	fetchedAt: string,
): Promise<UpstreamSnapshot> {
	const got = await attempt(fetcher, url, timeoutMs, expectOk, true);
	if (!got.result.ok) return unavailable(provider, pageUrl, fetchedAt, got.result.error ?? 'failed');
	return parseSummary(provider, pageUrl, fetchedAt, got.json);
}

export interface UpstreamUrls {
	github: { api: string; page: string };
	stripe: { api: string; page: string };
}

export async function fetchUpstreams(
	urls: UpstreamUrls,
	fetcher: Fetch,
	timeoutMs: number,
	fetchedAt: string,
): Promise<Upstreams> {
	const [github, stripe] = await Promise.all([
		fetchUpstream('github', urls.github.api, urls.github.page, fetcher, timeoutMs, fetchedAt),
		fetchUpstream('stripe', urls.stripe.api, urls.stripe.page, fetcher, timeoutMs, fetchedAt),
	]);
	return { github, stripe };
}
