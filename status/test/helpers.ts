import { readdirSync, readFileSync } from 'node:fs';
import { DatabaseSync, type SQLInputValue } from 'node:sqlite';
import { join } from 'node:path';
import type { Observation, ProbeResult, Upstreams } from '../src/attribution.ts';
import type { Provider } from '../src/model.ts';
import type { CallCounts, ServerStatus } from '../src/server-status.ts';
import type { UpstreamIncident, UpstreamSnapshot, UpstreamStatus } from '../src/statuspage.ts';

export const root = join(import.meta.dirname, '..');

export function fixture(name: string): unknown {
	return JSON.parse(readFileSync(join(root, 'test/fixtures', name), 'utf8'));
}

/**
 * A D1Database over node:sqlite with the real migrations applied, in order: enough of the API for src/store.ts
 * (prepare, bind, run, first, all, batch), so the SQL itself is tested.
 */
export function memoryD1(): D1Database & { sqlite: DatabaseSync } {
	const sqlite = new DatabaseSync(':memory:');
	for (const file of readdirSync(join(root, 'migrations')).filter((f) => f.endsWith('.sql')).sort()) {
		sqlite.exec(readFileSync(join(root, 'migrations', file), 'utf8'));
	}
	class Statement {
		readonly sql: string;
		readonly params: SQLInputValue[];
		constructor(sql: string, params: SQLInputValue[] = []) {
			this.sql = sql;
			this.params = params;
		}
		bind(...params: unknown[]) {
			return new Statement(this.sql, params as SQLInputValue[]);
		}
		runSync() {
			const r = sqlite.prepare(this.sql).run(...this.params);
			return { success: true, results: [], meta: { changes: Number(r.changes) } };
		}
		async run() {
			return this.runSync();
		}
		async first<T>() {
			return (sqlite.prepare(this.sql).get(...this.params) as T | undefined) ?? null;
		}
		async all<T>() {
			return { success: true, results: sqlite.prepare(this.sql).all(...this.params) as T[], meta: {} };
		}
	}
	const db = {
		sqlite,
		prepare: (sql: string) => new Statement(sql),
		// Synchronous inside, like D1's batch: one transaction, never interleaved with another caller's.
		async batch(statements: Statement[]) {
			sqlite.exec('BEGIN');
			try {
				const out = [];
				for (const s of statements) out.push(s.runSync());
				sqlite.exec('COMMIT');
				return out;
			} catch (e) {
				sqlite.exec('ROLLBACK');
				throw e;
			}
		},
	};
	return db as unknown as D1Database & { sqlite: DatabaseSync };
}

export const OK: ProbeResult = { ok: true, status: 200, ms: 40 };
export const CANARY_OK: ProbeResult = { ok: true, status: 401, ms: 45 };

export function counts(requests: number, errors: number, rateLimited = 0): CallCounts {
	return { windowSeconds: 300, requests, errors, rateLimited };
}

export function server(github: CallCounts = counts(1840, 3), stripe: CallCounts | undefined = counts(12, 0)): ServerStatus {
	return { version: '0.2.0', startedAt: '2026-09-27T14:13:23Z', github, stripe };
}

export function observation(over: Partial<Observation> = {}): Observation {
	return {
		checkedAt: '2026-09-27T14:30:00.000Z',
		healthz: OK,
		apiStatus: OK,
		apiStatusMissing: false,
		canary: CANARY_OK,
		server: server(),
		...over,
	};
}

const GITHUB_COMPONENTS = ['Git Operations', 'Webhooks', 'API Requests', 'Issues', 'Pull Requests', 'Actions', 'Packages', 'Pages'];

export function snapshot(
	provider: Provider,
	statuses: Record<string, UpstreamStatus> = {},
	incidents: Partial<UpstreamIncident>[] = [],
): UpstreamSnapshot {
	const names = provider === 'github' ? GITHUB_COMPONENTS : ['API', 'Dashboard', 'Checkout', 'Webhooks'];
	return {
		provider,
		pageUrl: provider === 'github' ? 'https://www.githubstatus.com' : 'https://status.stripe.com',
		fetchedAt: '2026-09-27T14:30:00.000Z',
		available: true,
		description: 'All Systems Operational',
		components: names.map((name, i) => ({ id: `c${i}`, name, status: statuses[name] ?? 'operational' })),
		incidents: incidents.map((i, n) => ({
			id: `i${n}`,
			name: 'Incident',
			status: 'investigating',
			impact: 'minor',
			url: `https://stspg.io/${provider}${n}`,
			components: [],
			startedAt: '2026-09-27T14:00:00.000Z',
			updatedAt: '2026-09-27T14:10:00.000Z',
			...i,
		})),
	};
}

export function upstreams(github = snapshot('github'), stripe = snapshot('stripe')): Upstreams {
	return { github, stripe };
}
