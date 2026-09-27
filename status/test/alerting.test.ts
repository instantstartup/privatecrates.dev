import { describe, expect, it, vi } from 'vitest';
import {
	CONFIRM_RUNS,
	compose,
	decide,
	formatDuration,
	type AlertItem,
	type AlertMessage,
	type AlertRow,
} from '../src/alerting.ts';
import { alertEnvironment, alertTest, readAlerter, type Alerter } from '../src/alerts.ts';
import { evaluate, type Cause, type ComponentStatus, type Evaluation, type Reason } from '../src/attribution.ts';
import { readConfig } from '../src/config.ts';
import { runChecks } from '../src/index.ts';
import { COMPONENTS, type ComponentId, type Environment, type State } from '../src/model.ts';
import { buildMime, deliver, encodeWords, ntfyRequest, type Channels } from '../src/notify.ts';
import type { Fetch } from '../src/probes.ts';
import { loadLatest } from '../src/store.ts';
import { memoryD1, observation, snapshot, upstreams } from './helpers.ts';
import { EmailMessage } from './stubs/cloudflare-email.ts';

const PROD: Environment = { id: 'production', name: 'Production', apex: 'privatecrates.dev', canary: 'canary', path: '/' };
const DEV: Environment = { id: 'dev', name: 'Development', apex: 'dev.privatecrates.dev', canary: 'canary', path: '/dev' };
const T0 = Date.parse('2026-09-27T14:00:00Z');
const MIN = 60_000;
const at = (n: number) => T0 + n * MIN;

const OURS: Reason = { kind: 'ours', text: 'The registry is not answering Cargo', detail: 'Canary registry: HTTP 502' };
const ACTIONS: Reason = {
	kind: 'upstream',
	text: 'GitHub Actions is having an incident',
	href: 'https://stspg.io/actions1',
	linkText: 'Delays starting Actions jobs',
};

type Spec = Partial<Record<ComponentId, [State, Cause, Reason[]?]>>;

/** An evaluation with the given components not operational; the rest fine. */
function ev(spec: Spec = {}): Evaluation {
	const components: ComponentStatus[] = COMPONENTS.map((c) => {
		const [state, cause, reasons] = spec[c.id] ?? ['operational', 'none', []];
		return {
			id: c.id,
			name: c.name,
			state,
			cause,
			reasons: reasons ?? (cause === 'upstream' ? [ACTIONS] : cause === 'ours' ? [OURS] : []),
			notes: [],
		};
	});
	return { checkedAt: new Date(T0).toISOString(), state: 'operational', components };
}

/** Runs `decide` over a sequence of evaluations, one a minute from T0, carrying the rows; returns each run's items. */
function simulate(evals: Evaluation[], production = true, start = 0, rows = new Map<ComponentId, AlertRow>()) {
	const out: AlertItem[][] = [];
	evals.forEach((e, n) => {
		const d = decide(rows, e, at(start + n), production);
		rows = new Map(d.rows.map((r) => [r.component, r]));
		out.push(d.items);
	});
	return { items: out, rows };
}

const publishingDown = ev({ publishing: ['outage', 'ours'] });
const publishingDegraded = ev({ publishing: ['degraded', 'ours'] });
const healthy = ev();

describe('deciding when to page', () => {
	it('needs two consecutive runs: one run of degraded says nothing', () => {
		expect(CONFIRM_RUNS).toBe(2);
		const { items } = simulate([healthy, publishingDegraded, healthy, publishingDegraded, healthy]);
		expect(items.flat()).toEqual([]);
	});

	it('pages on the second run, not again while it lasts, and says when it recovers and for how long', () => {
		const { items } = simulate([healthy, publishingDegraded, publishingDegraded, publishingDegraded, publishingDegraded, healthy]);
		expect(items[1]).toEqual([]);
		expect(items[2]).toHaveLength(1);
		expect(items[2]![0]).toMatchObject({ kind: 'page', component: 'publishing', state: 'degraded', cause: 'ours', since: at(1) });
		expect(items[3]).toEqual([]);
		expect(items[4]).toEqual([]);
		expect(items[5]).toEqual([
			expect.objectContaining({ kind: 'resolved', component: 'publishing', previous: 'degraded', since: at(1), durationMs: 4 * MIN }),
		]);
	});

	it('pages an escalation from degraded to down at once, and an improvement', () => {
		const { items } = simulate([publishingDegraded, publishingDegraded, publishingDown, publishingDegraded]);
		expect(items[1]![0]).toMatchObject({ kind: 'page', state: 'degraded' });
		// Within the flap window: held back.
		expect(items[2]).toEqual([]);
		const later = simulate([publishingDegraded, publishingDegraded, ...Array(10).fill(publishingDegraded), publishingDown, publishingDown]);
		expect(later.items[12]![0]).toMatchObject({ kind: 'page', state: 'outage', previous: 'degraded', since: at(0) });
		const better = simulate([publishingDegraded], true, 23, later.rows);
		expect(better.items[0]![0]).toMatchObject({ kind: 'improved', state: 'degraded', previous: 'outage' });
	});

	it('pages for errors calling GitHub that GitHub has not reported (not upstream-attributed)', () => {
		const e = ev({ reads: ['degraded', 'api-errors', [{ kind: 'api-errors', text: "Errors calling GitHub's API" }]] });
		const { items } = simulate([e, e]);
		expect(items[1]![0]).toMatchObject({ kind: 'page', cause: 'api-errors' });
		const msg = compose(PROD, items[1]!, { statusUrl: 'https://status.privatecrates.dev', production: true })!;
		expect(msg.body).toContain('Whose: unclear.');
		expect(msg.priority).toBe(5);
	});

	it('says nothing for a component raised only by an incident we wrote', () => {
		const e = ev({ publishing: ['degraded', 'incident'] });
		expect(simulate([e, e, e]).items.flat()).toEqual([]);
	});
});

describe('upstream problems', () => {
	const actions = ev({ publishing: ['degraded', 'upstream'] });

	it('notify at low priority, blaming GitHub with the incident link, and again when over', () => {
		const { items } = simulate([healthy, actions, actions, actions, healthy]);
		expect(items[1]).toEqual([]);
		expect(items[2]![0]).toMatchObject({ kind: 'upstream', state: 'degraded' });
		expect(items[3]).toEqual([]);
		expect(items[4]![0]).toMatchObject({ kind: 'upstream-ended', durationMs: 3 * MIN });

		const msg = compose(PROD, items[2]!, { statusUrl: 'https://status.privatecrates.dev', production: true })!;
		expect(msg.priority).toBe(2);
		expect(msg.tags).toEqual(['cloud', 'production']);
		expect(msg.title).toBe('Production: Publishing degraded: GitHub incident');
		expect(msg.body.split('\n')[0]).toBe(
			'Publishing degraded: GitHub Actions is having an incident: Delays starting Actions jobs (https://stspg.io/actions1). Nothing to fix on our side.',
		);
		const over = compose(PROD, items[4]!, { statusUrl: 'https://status.privatecrates.dev', production: true })!;
		expect(over.priority).toBe(2);
		expect(over.subject).toBe('[resolved] [PrivateCrates production] Publishing: GitHub problem over');
		expect(over.body).toContain('https://stspg.io/actions1');
	});

	it('are never reminded, while ours are', () => {
		const long = simulate([actions, ...Array(60).fill(actions)]);
		expect(long.items.flat().map((i) => i.kind)).toEqual(['upstream']);
	});

	it('page when the blame moves to us, urgently', () => {
		const { items } = simulate([actions, actions, ...Array(10).fill(actions), publishingDown, publishingDown]);
		expect(items[13]![0]).toMatchObject({ kind: 'page', state: 'outage', since: at(12) });
		expect(compose(PROD, items[13]!, { statusUrl: 'https://s', production: true })!.priority).toBe(5);
	});

	it('when the blame moves from us to GitHub, say ours is resolved at once, then notify (after the flap window)', () => {
		const { items } = simulate([publishingDown, publishingDown, actions, actions, actions, ...Array(8).fill(actions)]);
		expect(items[1]![0]).toMatchObject({ kind: 'page' });
		expect(items[3]!.map((i) => i.kind)).toEqual(['resolved']);
		expect(items.slice(4, 11).flat()).toEqual([]);
		expect(items[11]!.map((i) => [i.kind, i.since])).toEqual([['upstream', at(2)]]);
	});

	it('come from the real attribution: a GitHub Actions incident is upstream, a failed canary is ours', () => {
		const github = snapshot('github', { Actions: 'partial_outage' }, [{ name: 'Actions delays', components: ['Actions'] }]);
		const up = evaluate(observation(), upstreams(github));
		const ours = evaluate(observation({ canary: { ok: false, status: 502, ms: 30, error: 'HTTP 502' } }), upstreams());
		const a = simulate([up, up]).items[1]!;
		expect(a.map((i) => [i.component, i.kind])).toEqual([['publishing', 'upstream']]);
		const b = simulate([ours, ours]).items[1]!;
		expect(b.map((i) => [i.component, i.kind])).toEqual([
			['reads', 'page'],
			['downloads', 'page'],
			['publishing', 'page'],
		]);
	});
});

describe('production and dev', () => {
	it('production pages at priority 5 with a siren; dev at 3 with a warning, never urgent', () => {
		const prodItems = simulate([publishingDown, publishingDown], true).items[1]!;
		const devItems = simulate([publishingDown, publishingDown], false).items[1]!;
		const prod = compose(PROD, prodItems, { statusUrl: 'https://status.privatecrates.dev', production: true })!;
		const dev = compose(DEV, devItems, { statusUrl: 'https://status.privatecrates.dev', production: false })!;
		expect(prod).toMatchObject({ priority: 5, tags: ['rotating_light', 'production'], click: 'https://status.privatecrates.dev/' });
		expect(prod.title).toBe('Production: Publishing down');
		expect(prod.subject).toBe('[PrivateCrates production] Publishing down');
		expect(dev).toMatchObject({ priority: 3, tags: ['warning', 'dev'], click: 'https://status.privatecrates.dev/dev' });
		expect(dev.subject).toBe('[PrivateCrates dev] Publishing down');
	});

	it('recoveries are priority 3 in both', () => {
		const items = simulate([publishingDown, publishingDown, healthy]).items[2]!;
		const msg = compose(PROD, items, { statusUrl: 'https://s', production: true })!;
		expect(msg).toMatchObject({ priority: 3, tags: ['white_check_mark', 'production'] });
		// Down from minute 0 (first seen), paged at 1, recovered at 2.
		expect(msg.subject).toBe('[resolved] [PrivateCrates production] Publishing recovered after 2 min');
		expect(msg.body).toContain('Publishing has recovered, after 2 min down.');
		expect(msg.body).toContain('It began 2026-09-27 14:00 UTC.');
	});
});

describe('reminders', () => {
	it('re-page an ongoing production problem of ours every 30 minutes', () => {
		const { items } = simulate(Array(70).fill(publishingDown));
		const at_ = items.flatMap((list, n) => list.map((i) => [n, i.kind]));
		expect(at_).toEqual([
			[1, 'page'],
			[31, 'reminder'],
			[61, 'reminder'],
		]);
		const reminder = items[31]![0]!;
		expect(reminder).toMatchObject({ since: at(0), durationMs: 31 * MIN });
		const msg = compose(PROD, [reminder], { statusUrl: 'https://s', production: true })!;
		expect(msg.priority).toBe(5);
		expect(msg.title).toBe('Production: Publishing still down (31 min)');
	});

	it('are not sent for dev', () => {
		const { items } = simulate(Array(70).fill(publishingDown), false);
		expect(items.flat().map((i) => i.kind)).toEqual(['page']);
	});
});

describe('the flap limit', () => {
	it('allows one page per component per 10 minutes, but always sends recoveries', () => {
		// Down, up, down again (confirmed at minute 4), up, down, ... every few minutes.
		const flappy = [publishingDown, publishingDown, healthy, publishingDown, publishingDown, healthy, publishingDown, publishingDown];
		const { items } = simulate([...flappy, ...Array(6).fill(publishingDown)]);
		const kinds = items.flatMap((list, n) => list.map((i) => [n, i.kind]));
		expect(kinds).toEqual([
			[1, 'page'],
			[2, 'resolved'],
			// Minutes 4 and 7: confirmed again, but within 10 minutes of the page at 1: held back.
			// Minute 11: still down, the window has passed.
			[11, 'page'],
		]);
		expect(items[11]![0]).toMatchObject({ since: at(6) });
	});

	it('is per component', () => {
		const both = ev({ publishing: ['outage', 'ours'], signin: ['outage', 'ours'] });
		const { items } = simulate([publishingDown, publishingDown, both, both]);
		expect(items[3]!.map((i) => i.component)).toEqual(['signin']);
	});
});

describe('grouping', () => {
	it('makes one message per environment for simultaneous changes', () => {
		const down = evaluate(observation({ healthz: { ok: false, status: 0, ms: 10_000, error: 'timeout' }, server: undefined }), upstreams());
		const items = simulate([down, down]).items[1]!;
		expect(items.map((i) => i.component)).toEqual(['reads', 'downloads', 'publishing', 'signin', 'website']);
		const msg = compose(PROD, items, { statusUrl: 'https://status.privatecrates.dev', production: true })!;
		expect(msg.title).toBe('Production: Registry reads, Downloads, Publishing, Sign-in and Website and account down');
		// One paragraph for the five, as they share a cause.
		expect(msg.body.match(/Whose: ours/g)).toHaveLength(1);
		expect(msg.body).toContain('Registry reads, Downloads, Publishing, Sign-in and Website and account are down.');
		expect(msg.body).toContain('Our server is not responding (Health check: timeout)');
		expect(msg.body).toContain('Since 2026-09-27 14:00 UTC');
		expect(msg.body.trimEnd().endsWith('Production (privatecrates.dev). Status page: https://status.privatecrates.dev/')).toBe(true);
	});

	it('mixes kinds in one message, the urgent first, at the highest priority', () => {
		const actions = ev({ publishing: ['degraded', 'upstream'] });
		const mixed = ev({ publishing: ['degraded', 'upstream'], signin: ['outage', 'ours'] });
		const { items } = simulate([actions, actions, ...Array(10).fill(actions), mixed, mixed]);
		// Publishing is still GitHub's (already notified); only Sign-in is news.
		const msg = compose(PROD, items[13]!, { statusUrl: 'https://s', production: true })!;
		expect(msg.title).toBe('Production: Sign-in down');

		// Then GitHub recovers while Sign-in is still down: one message with both.
		const onlySignin = ev({ signin: ['outage', 'ours'] });
		const { items: after } = simulate([onlySignin, onlySignin, ...Array(9).fill(mixed), actions, actions], true, 14, simulate([actions, actions, ...Array(10).fill(actions), mixed, mixed]).rows);
		expect(after[0]!.map((i) => i.kind)).toEqual(['upstream-ended']);
		// GitHub again from minute 16 (notified at 17); Sign-in recovers at 25, on its own.
		expect(after[11]!.map((i) => [i.component, i.kind])).toEqual([['signin', 'resolved']]);

		const merged = compose(PROD, [after[0]![0]!, items[13]![0]!], { statusUrl: 'https://s', production: true })!;
		expect(merged.priority).toBe(5);
		expect(merged.tags).toEqual(['rotating_light', 'white_check_mark', 'production']);
		expect(merged.title).toBe('Production: Sign-in down; Publishing: GitHub problem over');
		expect(merged.subject.startsWith('[resolved]')).toBe(false);
		expect(merged.body.indexOf('Sign-in is down.')).toBeLessThan(merged.body.indexOf('Publishing is operational again'));
	});

	it('formats durations', () => {
		expect(formatDuration(0)).toBe('0 min');
		expect(formatDuration(59 * MIN)).toBe('59 min');
		expect(formatDuration(95 * MIN)).toBe('1 h 35 min');
		expect(formatDuration(26 * 60 * MIN)).toBe('1 d 2 h');
	});
});

// ---------------------------------------------------------------------------------------------------------------

const MESSAGE: AlertMessage = {
	title: 'Production: Publishing down',
	subject: '[PrivateCrates production] Publishing down',
	body: 'Publishing is down.\nWhose: ours.',
	priority: 5,
	tags: ['rotating_light', 'production'],
	click: 'https://status.privatecrates.dev/',
};

describe('ntfy', () => {
	it('POSTs the body as plain text to the topic, with Title, Priority, Tags and Click', () => {
		const { url, init } = ntfyRequest({ url: 'https://ntfy.sh/', topic: 'pc-alerts-9f2c1d' }, MESSAGE);
		expect(url).toBe('https://ntfy.sh/pc-alerts-9f2c1d');
		expect(init.method).toBe('POST');
		expect(init.body).toBe(MESSAGE.body);
		expect(init.headers).toEqual({
			'content-type': 'text/plain; charset=utf-8',
			Title: 'Production: Publishing down',
			Priority: '5',
			Tags: 'rotating_light,production',
			Click: 'https://status.privatecrates.dev/',
		});
	});

	it('sends a bearer token when there is one', () => {
		const { url, init } = ntfyRequest({ url: 'https://ntfy.example.org', topic: 'alerts', token: 'tk_abc' }, { ...MESSAGE, priority: 2 });
		expect(url).toBe('https://ntfy.example.org/alerts');
		expect(init.headers).toMatchObject({ Authorization: 'Bearer tk_abc', Priority: '2' });
	});

	it('encodes non-ASCII titles (RFC 2047) and strips line breaks', () => {
		const { init } = ntfyRequest({ url: 'https://ntfy.sh', topic: 't' }, { ...MESSAGE, title: 'Café\r\nX-Evil: 1' });
		const title = (init.headers as Record<string, string>).Title!;
		expect(title).toMatch(/^=\?UTF-8\?B\?[A-Za-z0-9+/=]+\?=$/);
		expect(Buffer.from(title.slice(10, -2), 'base64').toString('utf8')).toBe('Café X-Evil: 1');
	});

	it('splits long encoded words at 75 characters without cutting a character', () => {
		const words = encodeWords('é'.repeat(60));
		expect(words.length).toBeGreaterThan(1);
		for (const w of words) expect(w.length).toBeLessThanOrEqual(75);
		expect(words.map((w) => Buffer.from(w.slice(10, -2), 'base64').toString('utf8')).join('')).toBe('é'.repeat(60));
	});
});

describe('email', () => {
	const base = {
		from: 'alerts@privatecrates.dev',
		fromName: 'PrivateCrates status',
		to: 'owner@example.com',
		subject: '[PrivateCrates production] Publishing down',
		body: 'Publishing is down.\nWhose: ours.',
		dateMs: Date.parse('2026-09-27T14:31:00Z'),
		id: '0b7c',
	};

	it('is a well-formed RFC 5322 message with CRLF line endings', () => {
		const raw = buildMime(base);
		const [head, body] = raw.split('\r\n\r\n');
		expect(head!.split('\r\n')).toEqual([
			'From: "PrivateCrates status" <alerts@privatecrates.dev>',
			'To: <owner@example.com>',
			'Subject: [PrivateCrates production] Publishing down',
			'Date: Sun, 27 Sep 2026 14:31:00 +0000',
			'Message-ID: <0b7c@privatecrates.dev>',
			'MIME-Version: 1.0',
			'Content-Type: text/plain; charset=utf-8',
			'Content-Transfer-Encoding: 7bit',
			'Auto-Submitted: auto-generated',
		]);
		expect(body).toBe('Publishing is down.\r\nWhose: ours.\r\n');
		expect(raw.replace(/\r\n/g, '')).not.toMatch(/[\r\n]/);
	});

	it('base64-encodes a non-ASCII body and subject', () => {
		const raw = buildMime({ ...base, subject: '[resolved] Café down', body: 'Incident: “Delays” — see link\n'.repeat(5) });
		expect(raw).toContain('Content-Transfer-Encoding: base64');
		expect(raw).toMatch(/Subject: =\?UTF-8\?B\?[A-Za-z0-9+/=]+\?=/);
		const body = raw.split('\r\n\r\n')[1]!;
		for (const line of body.trimEnd().split('\r\n')) expect(line.length).toBeLessThanOrEqual(76);
		expect(Buffer.from(body.replace(/\r\n/g, ''), 'base64').toString('utf8')).toBe(
			'Incident: “Delays” — see link\r\n'.repeat(5),
		);
	});
});

// ---------------------------------------------------------------------------------------------------------------

interface Sent {
	ntfy: { url: string; init: RequestInit }[];
	email: EmailMessage[];
	logs: string[];
}

function fakeAlerter(opts: { ntfyFails?: boolean; emailFails?: boolean } = {}): Alerter & { sent: Sent } {
	const sent: Sent = { ntfy: [], email: [], logs: [] };
	const fetcher: Fetch = async (url, init) => {
		if (opts.ntfyFails) throw new TypeError('network error');
		sent.ntfy.push({ url, init: init ?? {} });
		return new Response('{}');
	};
	const binding = {
		async send(m: EmailMessage) {
			if (opts.emailFails) throw new Error('destination address not verified');
			sent.email.push(m);
			return { messageId: 'x' };
		},
	} as unknown as SendEmail;
	const alerter = readAlerter(
		{ NTFY_TOPIC: 'topic', ALERT_EMAIL_TO: 'owner@example.com', ALERT_EMAIL: binding },
		(from, to, raw) => new EmailMessage(from, to, raw) as unknown as globalThis.EmailMessage,
		fetcher,
		(m) => sent.logs.push(m),
		() => T0,
	)!;
	return Object.assign(alerter, { sent });
}

describe('alerting with D1 state', () => {
	it('pages once, on both channels, with the same content', async () => {
		const db = memoryD1();
		const a = fakeAlerter();
		expect(await alertEnvironment(a, db, PROD, publishingDown, at(0))).toBeUndefined();
		const r = await alertEnvironment(a, db, PROD, publishingDown, at(1));
		expect(r?.delivery).toEqual({ ntfy: 'sent', email: 'sent' });
		expect(a.sent.ntfy).toHaveLength(1);
		expect(a.sent.email).toHaveLength(1);
		const email = a.sent.email[0]!;
		expect(email.from).toBe('alerts@privatecrates.dev');
		expect(email.to).toBe('owner@example.com');
		expect(email.raw).toContain('Subject: [PrivateCrates production] Publishing down');
		expect(email.raw).toContain(String(a.sent.ntfy[0]!.init.body).replace(/\n/g, '\r\n'));
		for (let n = 2; n < 10; n++) await alertEnvironment(a, db, PROD, publishingDown, at(n));
		expect(a.sent.ntfy).toHaveLength(1);
	});

	it('never pages twice for one minute (overlapping or retried runs), nor for a late run', async () => {
		const db = memoryD1();
		const a = fakeAlerter();
		await alertEnvironment(a, db, PROD, publishingDown, at(0));
		const [x, y] = await Promise.all([
			alertEnvironment(a, db, PROD, publishingDown, at(1)),
			alertEnvironment(a, db, PROD, publishingDown, at(1)),
		]);
		expect([x, y].filter(Boolean)).toHaveLength(1);
		expect(await alertEnvironment(a, db, PROD, healthy, at(0))).toBeUndefined();
		expect(a.sent.ntfy).toHaveLength(1);
	});

	it('does not save or send when a later run has overtaken it', async () => {
		const db = memoryD1();
		const a = fakeAlerter();
		await alertEnvironment(a, db, PROD, publishingDown, at(0));
		// Run 1 has claimed its minute and read the state when run 2 claims minute 2.
		db.sqlite.prepare("UPDATE alert_runs SET minute = ?1 WHERE env = 'production'").run(Math.floor(at(2) / MIN));
		const { saveAlertRows, loadAlertRows } = await import('../src/store.ts');
		const prev = await loadAlertRows(db, 'production');
		const d = decide(prev, publishingDown, at(1), true);
		expect(d.items).toHaveLength(1);
		expect(await saveAlertRows(db, 'production', Math.floor(at(1) / MIN), prev, d.rows)).toBe(false);
	});

	it('remembers across restarts: a new Worker instance neither re-pages nor forgets to resolve', async () => {
		const db = memoryD1();
		const first = fakeAlerter();
		await alertEnvironment(first, db, PROD, publishingDown, at(0));
		await alertEnvironment(first, db, PROD, publishingDown, at(1));
		expect(first.sent.ntfy).toHaveLength(1);

		const second = fakeAlerter();
		for (let n = 2; n < 20; n++) await alertEnvironment(second, db, PROD, publishingDown, at(n));
		expect(second.sent.ntfy).toHaveLength(0);
		await alertEnvironment(second, db, PROD, healthy, at(20));
		expect(second.sent.ntfy).toHaveLength(1);
		expect((second.sent.ntfy[0]!.init.headers as Record<string, string>).Title).toBe('Production: Publishing recovered after 20 min');

		const third = fakeAlerter();
		await alertEnvironment(third, db, PROD, publishingDown, at(40));
		await alertEnvironment(third, db, PROD, publishingDown, at(41));
		for (let n = 42; n < 71; n++) await alertEnvironment(third, db, PROD, publishingDown, at(n));
		expect(third.sent.ntfy).toHaveLength(1);
		await alertEnvironment(fakeAlerter(), db, PROD, publishingDown, at(71));
		const fourth = fakeAlerter();
		await alertEnvironment(fourth, db, PROD, publishingDown, at(72));
		expect(fourth.sent.ntfy).toHaveLength(0);
	});

	it('keeps environments apart', async () => {
		const db = memoryD1();
		const a = fakeAlerter();
		for (const n of [0, 1]) {
			await alertEnvironment(a, db, PROD, publishingDown, at(n));
			await alertEnvironment(a, db, DEV, publishingDown, at(n));
		}
		expect(a.sent.ntfy.map((s) => (s.init.headers as Record<string, string>).Priority)).toEqual(['5', '3']);
	});

	it('writes nothing to alert_state while nothing changes', async () => {
		const db = memoryD1();
		const a = fakeAlerter();
		for (let n = 0; n < 5; n++) await alertEnvironment(a, db, PROD, healthy, at(n));
		expect(db.sqlite.prepare('SELECT count(*) AS n FROM alert_state').get()).toEqual({ n: 0 });
	});
});

describe('failures and configuration', () => {
	it('is off, silently, with neither channel configured', () => {
		const make = () => ({}) as globalThis.EmailMessage;
		const f: Fetch = async () => new Response();
		expect(readAlerter({}, make, f)).toBeUndefined();
		expect(readAlerter({ NTFY_TOPIC: ' ', ALERT_EMAIL_TO: '' }, make, f)).toBeUndefined();
		// An address without the binding (or the binding without an address) is not a channel.
		expect(readAlerter({ ALERT_EMAIL_TO: 'owner@example.com' }, make, f)).toBeUndefined();
		const ntfyOnly = readAlerter({ NTFY_TOPIC: 'topic', NTFY_TOKEN: 'tk_x' }, make, f)!;
		expect(ntfyOnly.channels.ntfy).toEqual({ url: 'https://ntfy.sh', topic: 'topic', token: 'tk_x' });
		expect(ntfyOnly.channels.email).toBeUndefined();
		expect(ntfyOnly.statusUrl).toBe('https://status.privatecrates.dev');
	});

	it('logs a failing channel and still uses the other', async () => {
		const a = fakeAlerter({ ntfyFails: true });
		expect(await deliver(a.channels, MESSAGE)).toEqual({ ntfy: 'failed', email: 'sent' });
		expect(a.sent.logs).toEqual(['alert: ntfy failed for "Production: Publishing down": network error']);

		const b = fakeAlerter();
		const channels: Channels = { ...b.channels, fetch: async () => new Response('limit', { status: 429 }) };
		expect(await deliver(channels, MESSAGE)).toEqual({ ntfy: 'failed', email: 'sent' });
		expect(b.sent.logs[0]).toContain('HTTP 429');
	});

	it('both channels failing never breaks the cron run, and the probes are still recorded', async () => {
		const config = readConfig({
			ENVIRONMENTS: [PROD, DEV],
			GITHUB_STATUS_URL: 'https://www.githubstatus.com/api/v2/summary.json',
			GITHUB_STATUS_PAGE: 'https://www.githubstatus.com',
			STRIPE_STATUS_URL: 'https://status.stripe.com/api/v2/summary.json',
			STRIPE_STATUS_PAGE: 'https://status.stripe.com',
			PROBE_TIMEOUT_MS: '1000',
		});
		// Every probe fails: our server is down everywhere.
		const probes: Fetch = async () => {
			throw new TypeError('network error');
		};
		const a = fakeAlerter({ ntfyFails: true, emailFails: true });
		const errors = vi.spyOn(console, 'error').mockImplementation(() => {});
		const logs = vi.spyOn(console, 'log').mockImplementation(() => {});
		try {
			const db = memoryD1();
			await runChecks(config, db, at(0), probes, [], () => at(0), a);
			await runChecks(config, db, at(1), probes, [], () => at(1), a);
			expect((await loadLatest(db, 'production'))?.evaluation.state).toBe('outage');
			expect((await loadLatest(db, 'dev'))?.checkedAt).toBe(new Date(at(1)).toISOString());
			expect(a.sent.logs.filter((l) => l.startsWith('alert: ntfy failed'))).toHaveLength(2);
			expect(a.sent.logs.filter((l) => l.startsWith('alert: email failed'))).toHaveLength(2);
			// The state was saved before sending: the failed page is not retried every minute.
			await runChecks(config, db, at(2), probes, [], () => at(2), a);
			expect(a.sent.logs).toHaveLength(4);
		} finally {
			errors.mockRestore();
			logs.mockRestore();
		}
	});

	it('a D1 failure in alerting is logged and the run goes on', async () => {
		const config = readConfig({
			ENVIRONMENTS: [PROD],
			GITHUB_STATUS_URL: 'https://g',
			GITHUB_STATUS_PAGE: 'https://g',
			STRIPE_STATUS_URL: 'https://s',
			STRIPE_STATUS_PAGE: 'https://s',
			PROBE_TIMEOUT_MS: '1000',
		});
		const db = memoryD1();
		db.sqlite.exec('DROP TABLE alert_runs');
		const errors = vi.spyOn(console, 'error').mockImplementation(() => {});
		try {
			await expect(
				runChecks(config, db, at(0), async () => new Response('ok'), [], () => at(0), fakeAlerter()),
			).resolves.toBeUndefined();
			expect(errors.mock.calls[0]?.[0]).toMatch(/^alert: production: .*alert_runs/);
			expect(await loadLatest(db, 'production')).toBeDefined();
		} finally {
			errors.mockRestore();
		}
	});
});

describe('the test page', () => {
	it('is sent once per ALERT_TEST value, urgently, on both channels', async () => {
		const db = memoryD1();
		const a = fakeAlerter();
		const logs = vi.spyOn(console, 'log').mockImplementation(() => {});
		try {
			expect(await alertTest(a, db, 'test-1', at(0))).toEqual({ ntfy: 'sent', email: 'sent' });
			expect(await alertTest(a, db, 'test-1', at(1))).toBeUndefined();
			expect(a.sent.ntfy).toHaveLength(1);
			expect(a.sent.ntfy[0]!.init.headers).toMatchObject({ Priority: '5', Title: 'Test page from PrivateCrates status' });
			expect(a.sent.email[0]!.raw).toContain('Subject: [PrivateCrates test] Test page');
			expect(await alertTest(a, db, 'test-2', at(2))).toEqual({ ntfy: 'sent', email: 'sent' });
		} finally {
			logs.mockRestore();
		}
	});
});
