/**
 * D1 storage: per-minute records, per-day summaries and the latest evaluation (migrations/0001_init.sql), and the
 * alerting state (migrations/0002_alerts.sql).
 *
 * Writes per run: for each environment one minute row, one day row and one latest row (about 8,600 rows a day for two
 * environments), and with alerting on, one alert_runs row, plus alert_state rows only when something changes (about
 * 2,900 more a day). Reads per page view: one latest row and up to 90 day rows.
 */
import { CONFIRM_RUNS, OK, type AlertRow, type Condition } from './alerting.ts';
import type { Cause, Evaluation, Reason, Upstreams } from './attribution.ts';
import { addMinute, emptyDay, pruneCutoff, type DaySummary, type MinuteRecord } from './history.ts';
import { isComponentId, type ComponentId } from './model.ts';

export interface Latest {
	checkedAt: string;
	evaluation: Evaluation;
	upstreams: Upstreams;
}

/**
 * Stores one minute and folds it into its day. Idempotent: if the cron fires twice for a minute, the second run's
 * `INSERT OR IGNORE` changes nothing and the day is not counted twice.
 */
export async function recordMinute(
	db: D1Database,
	env: string,
	day: string,
	record: MinuteRecord,
	evaluation: Evaluation,
	upstreams: Upstreams,
): Promise<void> {
	const inserted = await db
		.prepare('INSERT OR IGNORE INTO minutes (env, minute, states, probes, github) VALUES (?1, ?2, ?3, ?4, ?5)')
		.bind(env, record.minute, record.states, JSON.stringify(record.probes), record.github ? JSON.stringify(record.github) : null)
		.run();

	const statements: D1PreparedStatement[] = [
		db
			.prepare(
				`INSERT INTO latest (env, checked_at, evaluation, upstreams) VALUES (?1, ?2, ?3, ?4)
				 ON CONFLICT (env) DO UPDATE SET checked_at = ?2, evaluation = ?3, upstreams = ?4
				 WHERE excluded.checked_at >= latest.checked_at`,
			)
			.bind(env, evaluation.checkedAt, JSON.stringify(evaluation), JSON.stringify(upstreams)),
	];
	if (inserted.meta.changes > 0) {
		const row = await db
			.prepare('SELECT summary FROM days WHERE env = ?1 AND day = ?2')
			.bind(env, day)
			.first<{ summary: string }>();
		const summary = addMinute(row ? (JSON.parse(row.summary) as DaySummary) : emptyDay(day), evaluation);
		statements.push(
			db
				.prepare(
					'INSERT INTO days (env, day, summary) VALUES (?1, ?2, ?3) ON CONFLICT (env, day) DO UPDATE SET summary = ?3',
				)
				.bind(env, day, JSON.stringify(summary)),
		);
	}
	await db.batch(statements);
}

export async function loadLatest(db: D1Database, env: string): Promise<Latest | undefined> {
	const row = await db
		.prepare('SELECT checked_at, evaluation, upstreams FROM latest WHERE env = ?1')
		.bind(env)
		.first<{ checked_at: string; evaluation: string; upstreams: string }>();
	if (!row) return undefined;
	return {
		checkedAt: row.checked_at,
		evaluation: JSON.parse(row.evaluation) as Evaluation,
		upstreams: JSON.parse(row.upstreams) as Upstreams,
	};
}

export async function loadDays(db: D1Database, env: string, from: string): Promise<Map<string, DaySummary>> {
	const { results } = await db
		.prepare('SELECT day, summary FROM days WHERE env = ?1 AND day >= ?2 ORDER BY day')
		.bind(env, from)
		.all<{ day: string; summary: string }>();
	return new Map(results.map((r) => [r.day, JSON.parse(r.summary) as DaySummary]));
}

/** Deletes minute records older than 90 days and day summaries before the 90-day window. */
export async function pruneOld(db: D1Database, envs: readonly string[], nowMs: number): Promise<void> {
	const cutoff = pruneCutoff(nowMs);
	// Per environment, so each delete is a range on the primary key rather than a table scan.
	await db.batch(
		envs.flatMap((env) => [
			db.prepare('DELETE FROM minutes WHERE env = ?1 AND minute < ?2').bind(env, cutoff.minute),
			db.prepare('DELETE FROM days WHERE env = ?1 AND day < ?2').bind(env, cutoff.day),
		]),
	);
}

// ---------------------------------------------------------------------------------------------------------------
// Alerting state (migrations/0002_alerts.sql).

/**
 * Claims a run's minute for alerting in one environment. False when a run for this minute or a later one got there
 * first: an overlapping or retried cron run then says nothing, so nothing is paged twice.
 */
export async function claimAlertRun(db: D1Database, env: string, minute: number): Promise<boolean> {
	const r = await db
		.prepare(
			`INSERT INTO alert_runs (env, minute) VALUES (?1, ?2)
			 ON CONFLICT (env) DO UPDATE SET minute = excluded.minute WHERE excluded.minute > alert_runs.minute`,
		)
		.bind(env, minute)
		.run();
	return r.meta.changes > 0;
}

function encodeCondition(c: Condition): string {
	return c.kind === 'ok' ? 'ok' : `${c.kind}:${c.state}`;
}

function decodeCondition(s: string): Condition {
	const [kind, state] = s.split(':');
	if ((kind === 'ours' || kind === 'upstream') && (state === 'degraded' || state === 'outage')) return { kind, state };
	return OK;
}

interface AlertStateRow {
	component: string;
	observed: string;
	observed_since: number;
	streak: number;
	alerted: string;
	alerted_since: number | null;
	alerted_cause: string | null;
	alerted_reasons: string;
	paged_at: number | null;
}

export async function loadAlertRows(db: D1Database, env: string): Promise<Map<ComponentId, AlertRow>> {
	const { results } = await db
		.prepare(
			`SELECT component, observed, observed_since, streak, alerted, alerted_since, alerted_cause, alerted_reasons,
			   paged_at
			 FROM alert_state WHERE env = ?1`,
		)
		.bind(env)
		.all<AlertStateRow>();
	const out = new Map<ComponentId, AlertRow>();
	for (const r of results) {
		if (!isComponentId(r.component)) continue;
		out.set(r.component, {
			component: r.component,
			observed: decodeCondition(r.observed),
			observedSince: Number(r.observed_since),
			streak: Number(r.streak),
			alerted: decodeCondition(r.alerted),
			alertedSince: r.alerted_since === null ? null : Number(r.alerted_since),
			alertedCause: (r.alerted_cause as Cause | null) ?? null,
			alertedReasons: JSON.parse(r.alerted_reasons) as Reason[],
			pagedAt: r.paged_at === null ? null : Number(r.paged_at),
		});
	}
	return out;
}

/** Unchanged, or never stored and still what a missing row means (healthy, never alerted): nothing to write. */
function sameRow(a: AlertRow | undefined, b: AlertRow): boolean {
	if (a === undefined) {
		return b.observed.kind === 'ok' && b.streak >= CONFIRM_RUNS && b.alerted.kind === 'ok' && b.pagedAt === null;
	}
	return JSON.stringify(a) === JSON.stringify(b);
}

/**
 * Saves the rows that changed, but only while this run still holds its claim on the minute (a later run may have
 * overtaken it). False when it was overtaken: then nothing is written, and nothing should be sent.
 */
export async function saveAlertRows(
	db: D1Database,
	env: string,
	minute: number,
	previous: ReadonlyMap<ComponentId, AlertRow>,
	rows: readonly AlertRow[],
): Promise<boolean> {
	const changed = rows.filter((r) => !sameRow(previous.get(r.component), r));
	if (changed.length === 0) return true;
	const results = await db.batch(
		changed.map((r) =>
			db
				.prepare(
					`INSERT INTO alert_state (env, component, observed, observed_since, streak, alerted, alerted_since,
					   alerted_cause, alerted_reasons, paged_at)
					 SELECT ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10
					 WHERE EXISTS (SELECT 1 FROM alert_runs WHERE env = ?1 AND minute = ?11)
					 ON CONFLICT (env, component) DO UPDATE SET observed = ?3, observed_since = ?4, streak = ?5,
					   alerted = ?6, alerted_since = ?7, alerted_cause = ?8, alerted_reasons = ?9, paged_at = ?10`,
				)
				.bind(
					env,
					r.component,
					encodeCondition(r.observed),
					r.observedSince,
					r.streak,
					encodeCondition(r.alerted),
					r.alertedSince,
					r.alertedCause,
					JSON.stringify(r.alertedReasons),
					r.pagedAt,
					minute,
				),
		),
	);
	return results.every((r) => r.meta.changes > 0);
}

/** Records a test page's id; true the first time only. */
export async function claimAlertTest(db: D1Database, id: string, at: string): Promise<boolean> {
	const r = await db.prepare('INSERT OR IGNORE INTO alert_tests (id, sent_at) VALUES (?1, ?2)').bind(id, at).run();
	return r.meta.changes > 0;
}
