/** Reads the Worker's vars (wrangler.jsonc) into a checked configuration. */
import type { Environment } from './model.ts';
import type { UpstreamUrls } from './probes.ts';

export interface Vars {
	ENVIRONMENTS: unknown;
	GITHUB_STATUS_URL: string;
	GITHUB_STATUS_PAGE: string;
	STRIPE_STATUS_URL: string;
	STRIPE_STATUS_PAGE: string;
	PROBE_TIMEOUT_MS: string;
}

export interface Config {
	environments: Environment[];
	upstream: UpstreamUrls;
	timeoutMs: number;
}

const HOST = /^(?=.{1,253}$)([a-z0-9]([a-z0-9-]{0,61}[a-z0-9])?)(\.[a-z0-9]([a-z0-9-]{0,61}[a-z0-9])?)+$/;
const LABEL = /^[a-z0-9]([a-z0-9-]{0,61}[a-z0-9])?$/;

export function parseEnvironments(raw: unknown): Environment[] {
	const list = typeof raw === 'string' ? (JSON.parse(raw) as unknown) : raw;
	if (!Array.isArray(list) || list.length === 0) throw new Error('ENVIRONMENTS must be a non-empty list');
	const seen = new Set<string>();
	const out = list.map((item: unknown, n) => {
		const x = item as Partial<Environment>;
		const where = `ENVIRONMENTS[${n}]`;
		if (typeof x.id !== 'string' || !LABEL.test(x.id)) throw new Error(`${where}.id must be a short lower-case name`);
		if (typeof x.name !== 'string' || !x.name) throw new Error(`${where}.name is required`);
		if (typeof x.apex !== 'string' || !HOST.test(x.apex)) throw new Error(`${where}.apex must be a host name`);
		if (typeof x.canary !== 'string' || !LABEL.test(x.canary)) throw new Error(`${where}.canary must be a registry slug`);
		if (typeof x.path !== 'string' || !/^\/([a-z0-9-]+)?$/.test(x.path)) {
			throw new Error(`${where}.path must be / or /name`);
		}
		if (seen.has(x.id) || seen.has(x.path)) throw new Error(`${where} repeats an id or path`);
		seen.add(x.id).add(x.path);
		return { id: x.id, name: x.name, apex: x.apex, canary: x.canary, path: x.path };
	});
	if (!out.some((e) => e.path === '/')) throw new Error('one environment must have path /');
	return out;
}

export function readConfig(vars: Vars): Config {
	const timeoutMs = Number(vars.PROBE_TIMEOUT_MS || 10_000);
	return {
		environments: parseEnvironments(vars.ENVIRONMENTS),
		upstream: {
			github: { api: vars.GITHUB_STATUS_URL, page: vars.GITHUB_STATUS_PAGE },
			stripe: { api: vars.STRIPE_STATUS_URL, page: vars.STRIPE_STATUS_PAGE },
		},
		timeoutMs: Number.isFinite(timeoutMs) && timeoutMs > 0 ? Math.min(timeoutMs, 25_000) : 10_000,
	};
}
