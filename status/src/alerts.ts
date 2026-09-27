/**
 * Alerting, wired up: reads the channels from the Worker's vars, secrets and bindings, keeps state in D1, and sends.
 * Everything here fails soft: a problem is logged and the cron run carries on.
 */
import { compose, decide, isProduction, testMessage, type AlertMessage } from './alerting.ts';
import type { Evaluation } from './attribution.ts';
import { epochMinute } from './history.ts';
import type { Environment } from './model.ts';
import { deliver, type Channels, type Delivery, type MakeEmail } from './notify.ts';
import type { Fetch } from './probes.ts';
import { claimAlertRun, claimAlertTest, loadAlertRows, saveAlertRows } from './store.ts';

/** The Worker's alerting vars (wrangler.jsonc), secrets (`wrangler secret put`) and binding; all optional. */
export interface AlertEnv {
	/** The status site's origin, for links: `https://status.privatecrates.dev`. */
	STATUS_URL?: string;
	/** Var: the ntfy server, `https://ntfy.sh` by default. */
	NTFY_URL?: string;
	/** Secret: the ntfy topic. Without it, no pushes. */
	NTFY_TOPIC?: string;
	/** Secret, optional: an ntfy access token, sent as `Authorization: Bearer`. */
	NTFY_TOKEN?: string;
	/** Var: the sender, on the Email Routing domain. */
	ALERT_EMAIL_FROM?: string;
	/** Var: a verified Email Routing destination address. Empty: no email. */
	ALERT_EMAIL_TO?: string;
	/** The `send_email` binding. */
	ALERT_EMAIL?: SendEmail;
	/** Secret, set by `pnpm alert:test`: each new value sends one test page and email. */
	ALERT_TEST?: string;
}

export interface Alerter {
	channels: Channels;
	statusUrl: string;
}

const DEFAULT_STATUS_URL = 'https://status.privatecrates.dev';

/** The configured channels, or undefined when there are none: then alerting is silently off. */
export function readAlerter(
	env: AlertEnv,
	make: MakeEmail,
	fetcher: Fetch,
	log: (message: string) => void = (m) => console.error(m),
	now: () => number = Date.now,
): Alerter | undefined {
	const topic = env.NTFY_TOPIC?.trim();
	const to = env.ALERT_EMAIL_TO?.trim();
	const from = env.ALERT_EMAIL_FROM?.trim() || 'alerts@privatecrates.dev';
	const channels: Channels = { fetch: fetcher, log, now, id: () => crypto.randomUUID() };
	if (topic) {
		channels.ntfy = { url: env.NTFY_URL?.trim() || 'https://ntfy.sh', topic };
		const token = env.NTFY_TOKEN?.trim();
		if (token) channels.ntfy.token = token;
	}
	if (to && env.ALERT_EMAIL) channels.email = { binding: env.ALERT_EMAIL, from, to, make };
	if (!channels.ntfy && !channels.email) return undefined;
	return { channels, statusUrl: env.STATUS_URL?.trim() || DEFAULT_STATUS_URL };
}

/**
 * One environment's alerting for one run: claim the minute, compare with what we last said, save, then send. State
 * is saved before sending, so a crash between the two loses a message rather than repeating one.
 */
export async function alertEnvironment(
	alerter: Alerter,
	db: D1Database,
	env: Environment,
	evaluation: Pick<Evaluation, 'components'>,
	runMs: number,
): Promise<{ message: AlertMessage; delivery: Delivery } | undefined> {
	const minute = epochMinute(runMs);
	if (!(await claimAlertRun(db, env.id, minute))) return undefined;
	const previous = await loadAlertRows(db, env.id);
	const production = isProduction(env);
	const { rows, items } = decide(previous, evaluation, runMs, production);
	if (!(await saveAlertRows(db, env.id, minute, previous, rows))) return undefined;
	const message = compose(env, items, { statusUrl: alerter.statusUrl, production });
	if (!message) return undefined;
	const delivery = await deliver(alerter.channels, message);
	console.log(`alert: ${env.id}: "${message.title}" (priority ${message.priority}): ntfy ${delivery.ntfy}, email ${delivery.email}`);
	return { message, delivery };
}

/** Sends the test page once per value of `ALERT_TEST`. */
export async function alertTest(
	alerter: Alerter | undefined,
	db: D1Database,
	id: string,
	runMs: number,
): Promise<Delivery | undefined> {
	if (!(await claimAlertTest(db, id, new Date(runMs).toISOString()))) return undefined;
	if (!alerter) {
		console.error('alert test: ALERT_TEST is set but neither NTFY_TOPIC nor ALERT_EMAIL_TO is configured');
		return { ntfy: 'off', email: 'off' };
	}
	const delivery = await deliver(alerter.channels, testMessage(alerter.statusUrl, runMs));
	console.log(`alert test: ntfy ${delivery.ntfy}, email ${delivery.email}`);
	return delivery;
}
