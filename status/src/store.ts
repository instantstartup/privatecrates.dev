/**
 * D1 storage: per-minute records, per-day summaries and the latest evaluation. See migrations/0001_init.sql.
 *
 * Writes per run: for each environment one minute row, one day row and one latest row (about 8,600 rows a day for two
 * environments). Reads per page view: one latest row and up to 90 day rows.
 */
import type { Evaluation, Upstreams } from './attribution.ts';
import { addMinute, emptyDay, pruneCutoff, type DaySummary, type MinuteRecord } from './history.ts';

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
