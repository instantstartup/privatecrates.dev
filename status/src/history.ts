/**
 * Compact per-minute records, their daily summaries, the 90-day window and pruning. Pure.
 */
import type { Evaluation, Observation, ProbeResult } from './attribution.ts';
import { COMPONENTS, type ComponentId, type State } from './model.ts';

export const RETENTION_DAYS = 90;
const MINUTE_MS = 60_000;
const DAY_MS = 86_400_000;

const STATE_CODE: Record<State, string> = { operational: 'o', degraded: 'd', outage: 'x' };
const CODE_STATE: Record<string, State> = { o: 'operational', d: 'degraded', x: 'outage' };

/** One minute of one environment, as stored: about 60 bytes. */
export interface MinuteRecord {
	/** Minutes since the Unix epoch (UTC). */
	minute: number;
	/** One letter per component, in `COMPONENTS` order: o, d or x. */
	states: string;
	/** `[status, ms]` for healthz, /api/status and the canary; status 0 means no response. */
	probes: [number, number][];
	/** The server's GitHub calls in its window: `[requests, errors + rate limited]`, when known. */
	github?: [number, number];
}

export function epochMinute(ms: number): number {
	return Math.floor(ms / MINUTE_MS);
}

export function dayOf(ms: number): string {
	return new Date(ms).toISOString().slice(0, 10);
}

function probeTuple(p: ProbeResult): [number, number] {
	return [p.ok ? p.status || 200 : p.status, Math.round(p.ms)];
}

export function toMinuteRecord(ms: number, obs: Observation, evaluation: Evaluation): MinuteRecord {
	const byId = new Map(evaluation.components.map((c) => [c.id, c.state]));
	const record: MinuteRecord = {
		minute: epochMinute(ms),
		states: COMPONENTS.map((c) => STATE_CODE[byId.get(c.id) ?? 'operational']).join(''),
		probes: [probeTuple(obs.healthz), probeTuple(obs.apiStatus), probeTuple(obs.canary)],
	};
	if (obs.server) {
		record.github = [obs.server.github.requests, obs.server.github.errors + obs.server.github.rateLimited];
	}
	return record;
}

export function statesOf(record: Pick<MinuteRecord, 'states'>): Record<ComponentId, State> {
	const out = {} as Record<ComponentId, State>;
	COMPONENTS.forEach((c, i) => {
		out[c.id] = CODE_STATE[record.states[i] ?? 'o'] ?? 'operational';
	});
	return out;
}

/** A day's summary for one environment: minutes seen, and per component minutes operational, degraded, down. */
export interface DaySummary {
	day: string;
	minutes: number;
	counts: Record<ComponentId, [number, number, number]>;
	/** The latest reason each component was not operational that day, for the bar's details. */
	reasons: Partial<Record<ComponentId, string>>;
}

export function emptyDay(day: string): DaySummary {
	const counts = {} as Record<ComponentId, [number, number, number]>;
	for (const c of COMPONENTS) counts[c.id] = [0, 0, 0];
	return { day, minutes: 0, counts, reasons: {} };
}

const INDEX: Record<State, 0 | 1 | 2> = { operational: 0, degraded: 1, outage: 2 };

/** Adds one minute's evaluation to its day. Returns a new summary; the input is not changed. */
export function addMinute(summary: DaySummary, evaluation: Evaluation): DaySummary {
	const next: DaySummary = {
		day: summary.day,
		minutes: summary.minutes + 1,
		counts: { ...summary.counts },
		reasons: { ...summary.reasons },
	};
	for (const c of COMPONENTS) {
		const counts = [...(next.counts[c.id] ?? [0, 0, 0])] as [number, number, number];
		const status = evaluation.components.find((s) => s.id === c.id);
		const state = status?.state ?? 'operational';
		counts[INDEX[state]] += 1;
		next.counts[c.id] = counts;
		if (status && state !== 'operational' && status.reasons[0]) {
			next.reasons[c.id] = status.reasons[0].text.slice(0, 140);
		}
	}
	return next;
}

/** Builds day summaries from raw minute records (used to rebuild a day, and in tests). */
export function bucketByDay(records: readonly MinuteRecord[]): Map<string, DaySummary> {
	const days = new Map<string, DaySummary>();
	for (const r of records) {
		const day = dayOf(r.minute * MINUTE_MS);
		const summary = days.get(day) ?? emptyDay(day);
		summary.minutes += 1;
		const states = statesOf(r);
		for (const c of COMPONENTS) summary.counts[c.id][INDEX[states[c.id]]] += 1;
		days.set(day, summary);
	}
	return days;
}

/** How long an outage must last in a day before the day's bar shows an outage rather than degradation. */
export const OUTAGE_BAR_MINUTES = 10;

export type BarState = State | 'nodata';

export function barState(counts: [number, number, number] | undefined): BarState {
	if (!counts || counts[0] + counts[1] + counts[2] === 0) return 'nodata';
	if (counts[2] >= OUTAGE_BAR_MINUTES) return 'outage';
	if (counts[1] + counts[2] > 0) return 'degraded';
	return 'operational';
}

/** The share of checked minutes the component was not down (degraded counts as available). */
export function availability(counts: [number, number, number]): number | undefined {
	const total = counts[0] + counts[1] + counts[2];
	return total === 0 ? undefined : (counts[0] + counts[1]) / total;
}

/** The last `n` UTC days ending with today's, oldest first. */
export function lastDays(nowMs: number, n = RETENTION_DAYS): string[] {
	const today = Date.UTC(
		new Date(nowMs).getUTCFullYear(),
		new Date(nowMs).getUTCMonth(),
		new Date(nowMs).getUTCDate(),
	);
	const out: string[] = [];
	for (let i = n - 1; i >= 0; i--) out.push(dayOf(today - i * DAY_MS));
	return out;
}

/** Records older than these are deleted: minute records older than 90 days, and days before the 90-day window. */
export function pruneCutoff(nowMs: number): { minute: number; day: string } {
	return { minute: epochMinute(nowMs - RETENTION_DAYS * DAY_MS), day: lastDays(nowMs)[0]! };
}

export function prune<T extends { minute: number }>(records: readonly T[], nowMs: number): T[] {
	const { minute } = pruneCutoff(nowMs);
	return records.filter((r) => r.minute >= minute);
}

/** Totals a component's availability across a set of days. */
export function totalAvailability(days: readonly (DaySummary | undefined)[], id: ComponentId): number | undefined {
	const sum: [number, number, number] = [0, 0, 0];
	for (const d of days) {
		const c = d?.counts[id];
		if (!c) continue;
		sum[0] += c[0];
		sum[1] += c[1];
		sum[2] += c[2];
	}
	return availability(sum);
}
