/**
 * Who to wake, and when. Pure: no I/O, no clock. The cron works out each component's state (attribution.ts); this
 * compares it with what we last told the owner (kept in D1 by store.ts) and decides what to say. See docs/deploy.md
 * §14.5.
 *
 * - **Page** when one of *our* components (a problem not attributed to GitHub or Stripe) is degraded or down for
 *   {@link CONFIRM_RUNS} consecutive runs, and again when it recovers, with how long it lasted. Production pages are
 *   urgent (ntfy priority 5); dev pages are priority 3.
 * - **Notify** at low priority (2) when a problem is attributed upstream, and when that ends. Nothing to fix.
 * - **Remind** every {@link REMINDER_MS} while a production problem of ours goes on.
 * - **Flap limit**: at most one message per component per {@link FLAP_MS}, except recoveries. A change held back by
 *   the limit is not lost: it goes out on the first run after the window, if it still holds.
 * - Every change in one environment in one run becomes one message.
 */
import type { Cause, ComponentStatus, Evaluation, Reason } from './attribution.ts';
import { COMPONENTS, PROVIDER_NAMES, type ComponentId, type Environment, type State } from './model.ts';

/** Consecutive runs (minutes) a problem must be seen before we say anything. */
export const CONFIRM_RUNS = 2;
/** How often an ongoing production problem of ours is paged again. */
export const REMINDER_MS = 30 * 60_000;
/** The least time between two messages about one component, recoveries aside. */
export const FLAP_MS = 10 * 60_000;

export type Kind = 'ok' | 'ours' | 'upstream';

export interface Condition {
	kind: Kind;
	/** `operational` exactly when `kind` is `ok`. */
	state: State;
}

export const OK: Condition = { kind: 'ok', state: 'operational' };

/** What we remember about one component of one environment between runs (a row of `alert_state`). */
export interface AlertRow {
	component: ComponentId;
	/** What the last run saw. */
	observed: Condition;
	/** When the run of `observed.kind` began (ms). */
	observedSince: number;
	/** Consecutive runs with `observed.kind`, capped at {@link CONFIRM_RUNS} (so a steady state writes nothing). */
	streak: number;
	/** What we last told the owner. */
	alerted: Condition;
	/** When the problem we told them about began (ms). */
	alertedSince: number | null;
	alertedCause: Cause | null;
	/** The reasons we gave, for the message when it ends. */
	alertedReasons: Reason[];
	/** When we last sent a message about it, other than a recovery (ms): for the flap limit and reminders. */
	pagedAt: number | null;
}

export type ItemKind = 'page' | 'reminder' | 'improved' | 'upstream' | 'upstream-ended' | 'resolved';

/** One component's news, in one message. */
export interface AlertItem {
	kind: ItemKind;
	component: ComponentId;
	name: string;
	/** The state now (`operational` for resolved and upstream-ended). */
	state: State;
	/** The state before, for resolved, improved and upstream-ended. */
	previous?: State;
	cause: Cause;
	/** When the problem began (ms). */
	since: number;
	/** How long it has lasted, or lasted (ms). */
	durationMs: number;
	reasons: Reason[];
}

const RANK: Record<State, number> = { operational: 0, degraded: 1, outage: 2 };

/** Which of our alert rules a component's status falls under. */
export function classify(status: Pick<ComponentStatus, 'state' | 'cause'>): Condition {
	if (status.state === 'operational') return OK;
	switch (status.cause) {
		case 'upstream':
			return { kind: 'upstream', state: status.state };
		case 'ours':
		case 'api-errors':
			return { kind: 'ours', state: status.state };
		// Raised only by an incident we wrote ourselves, with the probes fine: we already know.
		case 'incident':
		case 'none':
			return OK;
	}
}

export function sameCondition(a: Condition, b: Condition): boolean {
	return a.kind === b.kind && a.state === b.state;
}

export function freshRow(component: ComponentId, nowMs: number): AlertRow {
	return {
		component,
		observed: OK,
		observedSince: nowMs,
		streak: CONFIRM_RUNS,
		alerted: OK,
		alertedSince: null,
		alertedCause: null,
		alertedReasons: [],
		pagedAt: null,
	};
}

/** Production pages are urgent; other environments never are. */
export function isProduction(env: Pick<Environment, 'id'>): boolean {
	return env.id === 'production';
}

export interface Decision {
	/** The new state of every component. */
	rows: AlertRow[];
	/** What to tell the owner, in component order; empty for nothing. */
	items: AlertItem[];
}

/**
 * One run's decision for one environment. `previous` is what `decide` returned last time (missing rows are treated
 * as healthy and quiet: a new deployment does not page for what it cannot know).
 */
export function decide(
	previous: ReadonlyMap<ComponentId, AlertRow>,
	evaluation: Pick<Evaluation, 'components'>,
	runMs: number,
	production: boolean,
): Decision {
	const rows: AlertRow[] = [];
	const items: AlertItem[] = [];
	for (const status of evaluation.components) {
		const prev = previous.get(status.id) ?? freshRow(status.id, runMs);
		const cond = classify(status);
		const continues = prev.observed.kind === cond.kind;
		const streak = Math.min(continues ? prev.streak + 1 : 1, CONFIRM_RUNS);
		const observedSince = continues ? prev.observedSince : runMs;
		const row: AlertRow = { ...prev, observed: cond, observedSince, streak };

		// Recovery counts at once; a problem only once it has held for CONFIRM_RUNS.
		const confirmed = cond.kind === 'ok' || streak >= CONFIRM_RUNS ? cond : prev.alerted;
		const base = { component: status.id, name: status.name, cause: status.cause };

		if (sameCondition(confirmed, prev.alerted)) {
			const since = prev.alertedSince ?? runMs;
			if (
				production &&
				confirmed.kind === 'ours' &&
				prev.pagedAt !== null &&
				runMs - prev.pagedAt >= REMINDER_MS
			) {
				items.push({
					...base,
					kind: 'reminder',
					state: confirmed.state,
					since,
					durationMs: runMs - since,
					reasons: cond.kind === 'ours' ? status.reasons : prev.alertedReasons,
				});
				row.pagedAt = runMs;
			}
		} else {
			// Our problem is over, even if an upstream one follows it: a recovery, never held back.
			if (confirmed.kind === 'ok' || (prev.alerted.kind === 'ours' && confirmed.kind === 'upstream')) {
				const since = prev.alertedSince ?? runMs;
				items.push({
					...base,
					cause: prev.alertedCause ?? 'none',
					kind: prev.alerted.kind === 'ours' ? 'resolved' : 'upstream-ended',
					state: 'operational',
					previous: prev.alerted.state,
					since,
					durationMs: runMs - since,
					reasons: prev.alertedReasons,
				});
				row.alerted = OK;
				row.alertedSince = null;
				row.alertedCause = null;
				row.alertedReasons = [];
			}
			if (confirmed.kind === 'ok') {
				// Done.
			} else if (prev.pagedAt !== null && runMs - prev.pagedAt < FLAP_MS) {
				// Held back by the flap limit; decided again next run.
			} else {
				// A new problem, or a change of state or of blame. The same kind of problem keeps its start time.
				const was = row.alerted;
				const since = was.kind === confirmed.kind && row.alertedSince !== null ? row.alertedSince : observedSince;
				let kind: ItemKind = confirmed.kind === 'upstream' ? 'upstream' : 'page';
				if (kind === 'page' && was.kind === 'ours' && RANK[confirmed.state] < RANK[was.state]) kind = 'improved';
				items.push({
					...base,
					kind,
					state: confirmed.state,
					previous: was.kind === 'ok' ? undefined : was.state,
					since,
					durationMs: runMs - since,
					reasons: status.reasons,
				});
				row.alerted = confirmed;
				row.alertedSince = since;
				row.alertedCause = status.cause;
				row.alertedReasons = status.reasons;
				row.pagedAt = runMs;
			}
		}
		rows.push(row);
	}
	return { rows, items };
}

// ---------------------------------------------------------------------------------------------------------------
// Composing the message.

/** One message for both channels. */
export interface AlertMessage {
	/** The ntfy title: `Production: Publishing down`. */
	title: string;
	/** The email subject: `[PrivateCrates production] Publishing down`, or `[resolved] …`. */
	subject: string;
	/** Plain text, the same in both channels. */
	body: string;
	/** ntfy priority, 1 (min) to 5 (urgent). */
	priority: 1 | 2 | 3 | 4 | 5;
	/** ntfy tags: emoji short codes first, then the environment. */
	tags: string[];
	/** The status page for the environment. */
	click: string;
}

export function itemPriority(kind: ItemKind, production: boolean): AlertMessage['priority'] {
	switch (kind) {
		case 'page':
			return production ? 5 : 3;
		case 'reminder':
			return production ? 5 : 3;
		case 'improved':
		case 'resolved':
			return 3;
		case 'upstream':
		case 'upstream-ended':
			return 2;
	}
}

const ORDER: ItemKind[] = ['page', 'reminder', 'improved', 'upstream', 'upstream-ended', 'resolved'];

function word(state: State): string {
	return state === 'outage' ? 'down' : state === 'degraded' ? 'degraded' : 'operational';
}

export function formatDuration(ms: number): string {
	const minutes = Math.max(0, Math.round(ms / 60_000));
	if (minutes < 60) return `${minutes} min`;
	const hours = Math.floor(minutes / 60);
	if (hours < 24) return minutes % 60 ? `${hours} h ${minutes % 60} min` : `${hours} h`;
	const days = Math.floor(hours / 24);
	return hours % 24 ? `${days} d ${hours % 24} h` : `${days} d`;
}

/** `2026-09-27 14:31 UTC`. */
export function formatTime(ms: number): string {
	return `${new Date(ms).toISOString().slice(0, 16).replace('T', ' ')} UTC`;
}

function list(names: string[]): string {
	if (names.length <= 1) return names.join('');
	return `${names.slice(0, -1).join(', ')} and ${names.at(-1)}`;
}

function providerOf(id: ComponentId): string {
	const p = COMPONENTS.find((c) => c.id === id)?.provider;
	return p ? PROVIDER_NAMES[p] : 'upstream';
}

function describeReason(r: Reason): string {
	let s = r.text;
	if (r.detail) s += ` (${r.detail})`;
	if (r.href) s += r.linkText ? `: ${r.linkText} ${r.href}` : `: ${r.href}`;
	return s;
}

/** Title phrases, grouping components with the same news: `Publishing and Sign-in down`. */
function summarise(items: readonly AlertItem[]): string {
	const groups = new Map<string, AlertItem[]>();
	for (const item of items) {
		const key = `${item.kind}:${item.state}:${item.kind.startsWith('upstream') ? providerOf(item.component) : ''}`;
		groups.set(key, [...(groups.get(key) ?? []), item]);
	}
	const phrases = [...groups.values()].map((group) => {
		const first = group[0]!;
		const names = list(group.map((i) => i.name));
		switch (first.kind) {
			case 'page':
				return `${names} ${word(first.state)}`;
			case 'reminder':
				return `${names} still ${word(first.state)} (${formatDuration(first.durationMs)})`;
			case 'improved':
				return `${names} now ${word(first.state)}`;
			case 'upstream':
				return `${names} ${word(first.state)}: ${providerOf(first.component)} incident`;
			case 'upstream-ended':
				return `${names}: ${providerOf(first.component)} problem over`;
			case 'resolved':
				return group.length === 1 ? `${names} recovered after ${formatDuration(first.durationMs)}` : `${names} recovered`;
		}
	});
	return phrases.join('; ');
}

function describeItem(item: AlertItem, names: readonly string[]): string[] {
	const lines: string[] = [];
	const who = list([...names]);
	const is = names.length > 1 ? 'are' : 'is';
	const has = names.length > 1 ? 'have' : 'has';
	const since = `Since ${formatTime(item.since)} (${formatDuration(item.durationMs)}).`;
	const why = item.reasons.map((r) => `- ${describeReason(r)}`);
	switch (item.kind) {
		case 'page':
		case 'reminder':
		case 'improved': {
			const head =
				item.kind === 'reminder'
					? `${who} ${is} still ${word(item.state)}.`
					: item.kind === 'improved'
						? `${who} ${is} now ${word(item.state)}, no longer ${word(item.previous ?? 'outage')}.`
						: `${who} ${is} ${word(item.state)}.`;
			lines.push(head);
			lines.push(
				item.cause === 'api-errors'
					? `Whose: unclear. We are seeing errors calling ${providerOf(item.component)}, which reports nothing yet.`
					: 'Whose: ours. Something to fix on our side.',
			);
			lines.push(since);
			if (why.length) lines.push('Why:', ...why);
			break;
		}
		case 'upstream': {
			const r = item.reasons.find((x) => x.kind === 'upstream') ?? item.reasons[0];
			const link = r?.href ? `: ${r.linkText ? `${r.linkText} ` : ''}(${r.href})` : '';
			lines.push(`${who} ${word(item.state)}: ${r ? r.text : `${providerOf(item.component)} problem`}${link}. Nothing to fix on our side.`);
			lines.push(`Whose: ${providerOf(item.component)}.`);
			lines.push(since);
			const more = item.reasons.filter((x) => x !== r).map((x) => `- ${describeReason(x)}`);
			if (more.length) lines.push('Also:', ...more);
			break;
		}
		case 'upstream-ended':
			lines.push(`${who} ${is} operational again: the ${providerOf(item.component)} problem is over, after ${formatDuration(item.durationMs)}.`);
			lines.push(`It began ${formatTime(item.since)}.`);
			if (why.length) lines.push('It was:', ...why);
			break;
		case 'resolved':
			lines.push(`${who} ${has} recovered, after ${formatDuration(item.durationMs)} ${word(item.previous ?? 'outage')}.`);
			lines.push(`It began ${formatTime(item.since)}.`);
			if (why.length) lines.push('It was:', ...why);
			break;
	}
	return lines;
}

/** Components with exactly the same news (one probe failing takes several down) share one paragraph. */
function blocks(items: readonly AlertItem[]): { item: AlertItem; names: string[] }[] {
	const out = new Map<string, { item: AlertItem; names: string[] }>();
	for (const item of items) {
		const { component: _c, name: _n, ...rest } = item;
		const key = JSON.stringify(rest);
		const block = out.get(key);
		if (block) block.names.push(item.name);
		else out.set(key, { item, names: [item.name] });
	}
	return [...out.values()];
}

export interface ComposeOptions {
	/** The status site's origin, `https://status.privatecrates.dev`. */
	statusUrl: string;
	production: boolean;
}

export function statusPageUrl(statusUrl: string, env: Pick<Environment, 'path'>): string {
	return `${statusUrl.replace(/\/+$/, '')}${env.path}`;
}

/** One message for one environment's news; undefined when there is none. */
export function compose(env: Environment, items: readonly AlertItem[], opts: ComposeOptions): AlertMessage | undefined {
	if (items.length === 0) return undefined;
	const sorted = [...items].sort((a, b) => ORDER.indexOf(a.kind) - ORDER.indexOf(b.kind));
	const summary = summarise(sorted);
	const priority = Math.max(...sorted.map((i) => itemPriority(i.kind, opts.production))) as AlertMessage['priority'];
	const kinds = new Set(sorted.map((i) => i.kind));

	const tags: string[] = [];
	if (kinds.has('page') || kinds.has('reminder')) tags.push(opts.production ? 'rotating_light' : 'warning');
	if (kinds.has('improved')) tags.push('small_red_triangle_down');
	if (kinds.has('upstream')) tags.push('cloud');
	if (kinds.has('resolved') || kinds.has('upstream-ended')) tags.push('white_check_mark');
	tags.push(env.id);

	const allOver = sorted.every((i) => i.kind === 'resolved' || i.kind === 'upstream-ended');
	const click = statusPageUrl(opts.statusUrl, env);
	const body = [
		...blocks(sorted).flatMap((b, n) => (n === 0 ? describeItem(b.item, b.names) : ['', ...describeItem(b.item, b.names)])),
		'',
		`${env.name} (${env.apex}). Status page: ${click}`,
	].join('\n');

	return {
		title: `${env.name}: ${summary}`,
		subject: `${allOver ? '[resolved] ' : ''}[PrivateCrates ${env.id}] ${summary}`,
		body,
		priority,
		tags,
		click,
	};
}

/** The message `pnpm alert:test` sends: urgent, so it shows whether pages get through Do Not Disturb. */
export function testMessage(statusUrl: string, nowMs: number): AlertMessage {
	return {
		title: 'Test page from PrivateCrates status',
		subject: '[PrivateCrates test] Test page',
		body: [
			`This is a test page, sent ${formatTime(nowMs)} because ALERT_TEST was set.`,
			'It was sent at priority 5 (urgent), like a production outage. If it reached you through Do Not Disturb, real pages will too.',
			'Nothing is wrong. Remove the secret with: pnpm exec wrangler secret delete ALERT_TEST',
			'',
			`Status page: ${statusPageUrl(statusUrl, { path: '/' })}`,
		].join('\n'),
		priority: 5,
		tags: ['test_tube', 'rotating_light'],
		click: statusPageUrl(statusUrl, { path: '/' }),
	};
}
