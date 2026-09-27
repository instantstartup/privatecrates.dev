import { describe, expect, it } from 'vitest';
import { evaluate } from '../src/attribution.ts';
import {
	addMinute,
	availability,
	barState,
	bucketByDay,
	dayOf,
	emptyDay,
	epochMinute,
	lastDays,
	prune,
	pruneCutoff,
	statesOf,
	toMinuteRecord,
	totalAvailability,
} from '../src/history.ts';
import { loadDays, loadLatest, pruneOld, recordMinute } from '../src/store.ts';
import { counts, memoryD1, observation, server, upstreams } from './helpers.ts';

const T = Date.parse('2026-09-27T14:30:00Z');
const healthy = evaluate(observation(), upstreams());
const apiErrors = evaluate(observation({ server: server(counts(400, 40)) }), upstreams());
const down = evaluate(observation({ healthz: { ok: false, status: 0, ms: 10000, error: 'timeout' }, server: undefined }), upstreams());

describe('minute records', () => {
	it('are compact and round-trip the states', () => {
		const record = toMinuteRecord(T, observation({ server: server(counts(400, 40)) }), apiErrors);
		expect(record).toEqual({
			minute: epochMinute(T),
			states: 'ddddoo',
			probes: [
				[200, 40],
				[200, 40],
				[401, 45],
			],
			github: [400, 40],
		});
		expect(JSON.stringify(record).length).toBeLessThan(100);
		expect(statesOf(record)).toMatchObject({ reads: 'degraded', website: 'operational', billing: 'operational' });
	});

	it('record no response as status 0', () => {
		const record = toMinuteRecord(T, observation({ healthz: { ok: false, status: 0, ms: 10000, error: 'timeout' }, server: undefined }), down);
		expect(record.probes[0]).toEqual([0, 10000]);
		expect(record.states).toBe('xxxxxo');
		expect(record.github).toBeUndefined();
	});
});

describe('daily bucketing', () => {
	it('adds minutes to a day without changing the input', () => {
		const start = emptyDay('2026-09-27');
		const one = addMinute(start, healthy);
		const two = addMinute(one, apiErrors);
		expect(start.minutes).toBe(0);
		expect(one.counts.reads).toEqual([1, 0, 0]);
		expect(two.minutes).toBe(2);
		expect(two.counts.reads).toEqual([1, 1, 0]);
		expect(two.counts.website).toEqual([2, 0, 0]);
		expect(two.reasons.reads).toBe("Errors calling GitHub's API");
		expect(two.reasons.website).toBeUndefined();
	});

	it('buckets raw records by UTC day, across midnight', () => {
		const late = Date.parse('2026-09-26T23:59:00Z');
		const records = [
			toMinuteRecord(late, observation(), down),
			toMinuteRecord(late + 60_000, observation(), healthy),
			toMinuteRecord(late + 120_000, observation(), apiErrors),
		];
		const days = bucketByDay(records);
		expect([...days.keys()]).toEqual(['2026-09-26', '2026-09-27']);
		expect(days.get('2026-09-26')?.counts.website).toEqual([0, 0, 1]);
		expect(days.get('2026-09-27')?.counts.reads).toEqual([1, 1, 0]);
		expect(days.get('2026-09-27')?.minutes).toBe(2);
	});

	it('colours a bar by the worst sustained state', () => {
		expect(barState(undefined)).toBe('nodata');
		expect(barState([0, 0, 0])).toBe('nodata');
		expect(barState([1440, 0, 0])).toBe('operational');
		expect(barState([1439, 1, 0])).toBe('degraded');
		expect(barState([1430, 0, 9])).toBe('degraded');
		expect(barState([1430, 0, 10])).toBe('outage');
	});

	it('counts degraded minutes as available and outage minutes as not', () => {
		expect(availability([0, 0, 0])).toBeUndefined();
		expect(availability([1380, 30, 30])).toBeCloseTo(1410 / 1440);
		const a = { ...emptyDay('a'), counts: { ...emptyDay('a').counts, reads: [100, 0, 0] as [number, number, number] } };
		const b = { ...emptyDay('b'), counts: { ...emptyDay('b').counts, reads: [50, 0, 50] as [number, number, number] } };
		expect(totalAvailability([a, undefined, b], 'reads')).toBe(0.75);
	});
});

describe('the 90-day window and pruning', () => {
	it('lists 90 UTC days ending today', () => {
		const days = lastDays(T);
		expect(days).toHaveLength(90);
		expect(days[89]).toBe('2026-09-27');
		expect(days[0]).toBe('2026-06-30');
		expect(lastDays(Date.parse('2026-03-01T00:00:00Z'), 2)).toEqual(['2026-02-28', '2026-03-01']);
	});

	it('keeps exactly 90 days of minutes', () => {
		const cutoff = pruneCutoff(T);
		expect(cutoff.day).toBe('2026-06-30');
		const minute = (ms: number) => ({ minute: epochMinute(ms) });
		const ninetyDays = 90 * 86_400_000;
		const kept = prune([minute(T - ninetyDays - 60_000), minute(T - ninetyDays), minute(T)], T);
		expect(kept).toEqual([minute(T - ninetyDays), minute(T)]);
	});
});

describe('D1 storage (the real migration, on SQLite)', () => {
	it('records a minute, folds it into its day, and is idempotent', async () => {
		const db = memoryD1();
		const up = upstreams();
		const record = toMinuteRecord(T, observation(), apiErrors);
		await recordMinute(db, 'production', dayOf(T), record, apiErrors, up);
		await recordMinute(db, 'production', dayOf(T), record, apiErrors, up); // a repeated cron run
		await recordMinute(db, 'production', dayOf(T), toMinuteRecord(T + 60_000, observation(), healthy), healthy, up);
		const days = await loadDays(db, 'production', '2026-09-01');
		expect(days.get('2026-09-27')?.minutes).toBe(2);
		expect(days.get('2026-09-27')?.counts.reads).toEqual([1, 1, 0]);
		const latest = await loadLatest(db, 'production');
		expect(latest?.evaluation.state).toBe('operational');
		expect(latest?.upstreams.github?.available).toBe(true);
		expect(await loadLatest(db, 'dev')).toBeUndefined();
	});

	it('does not let an older run overwrite a newer latest', async () => {
		const db = memoryD1();
		const newer = { ...healthy, checkedAt: '2026-09-27T14:31:00.000Z' };
		const older = { ...down, checkedAt: '2026-09-27T14:30:00.000Z' };
		await recordMinute(db, 'production', '2026-09-27', toMinuteRecord(T + 60_000, observation(), newer), newer, upstreams());
		await recordMinute(db, 'production', '2026-09-27', toMinuteRecord(T, observation(), older), older, upstreams());
		expect((await loadLatest(db, 'production'))?.evaluation.state).toBe('operational');
	});

	it('prunes minutes and days older than 90 days, per environment', async () => {
		const db = memoryD1();
		const old = T - 91 * 86_400_000;
		for (const env of ['production', 'dev']) {
			await recordMinute(db, env, dayOf(old), toMinuteRecord(old, observation(), healthy), healthy, upstreams());
			await recordMinute(db, env, dayOf(T), toMinuteRecord(T, observation(), healthy), healthy, upstreams());
		}
		await pruneOld(db, ['production', 'dev'], T);
		const minutes = db.sqlite.prepare('SELECT env, minute FROM minutes ORDER BY env').all();
		expect(minutes).toEqual([
			{ env: 'dev', minute: epochMinute(T) },
			{ env: 'production', minute: epochMinute(T) },
		]);
		const days = db.sqlite.prepare('SELECT day FROM days').all();
		expect(days).toEqual([{ day: '2026-09-27' }, { day: '2026-09-27' }]);
	});
});
