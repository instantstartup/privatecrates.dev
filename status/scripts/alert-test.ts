// pnpm alert:test: sends one test page (ntfy, priority 5) and one email through the deployed Worker's real bindings.
//
// It sets the ALERT_TEST secret to a fresh value; the next scheduled run (within a minute or so) sees a value it has
// not sent before, records it in D1 and sends the test once. Nothing is reachable over HTTP: only someone who can
// deploy the Worker can set a secret. See docs/deploy.md §14.5.
import { spawnSync } from 'node:child_process';

const id = `test-${new Date().toISOString()}`;
const put = spawnSync('wrangler', ['secret', 'put', 'ALERT_TEST'], {
	input: id,
	stdio: ['pipe', 'inherit', 'inherit'],
	shell: process.platform === 'win32',
});
if (put.status !== 0) {
	console.error('alert:test: `wrangler secret put ALERT_TEST` failed');
	process.exit(put.status ?? 1);
}
console.log(`\nALERT_TEST = ${id}.
Within about a minute the next cron run sends one urgent test push to NTFY_TOPIC and one email to ALERT_EMAIL_TO.
Watch it with:  pnpm exec wrangler tail privatecrates-status   (look for "alert test: ntfy sent, email sent")
Afterwards, optionally:  pnpm exec wrangler secret delete ALERT_TEST   (each value is only ever sent once)`);
