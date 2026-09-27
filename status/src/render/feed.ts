/** The Atom feed of one environment's incidents (RFC 4287). */
import { isActive, lastUpdated, type Incident } from '../incidents.ts';
import { escapeHtml as e } from '../markdown.ts';
import { componentName, type Environment } from '../model.ts';
import { formatDateTime } from './format.ts';
import { prefix } from './page.ts';

/** `updated` for a feed with no entries: fixed, so the feed does not look new on every fetch. */
const EMPTY_FEED_UPDATED = '2026-09-27T00:00:00.000Z';

function entryHtml(i: Incident, nowMs: number): string {
	const parts = [
		`<p>${isActive(i, nowMs) ? 'Ongoing. ' : ''}Affects ${e(i.components.map(componentName).join(', '))}.</p>`,
	];
	for (const u of i.updates) {
		parts.push(`<p><strong>${e(u.status)}</strong>, ${e(formatDateTime(u.at))}</p>${u.html}`);
	}
	if (i.html) parts.push(i.html);
	return parts.join('\n');
}

export function renderFeed(env: Environment, incidents: readonly Incident[], origin: string, nowMs: number): string {
	const base = `${origin}${prefix(env)}`;
	const visible = incidents.filter((i) => Date.parse(i.start) <= nowMs);
	const updated = visible.length
		? visible.map(lastUpdated).reduce((a, b) => (Date.parse(b) > Date.parse(a) ? b : a))
		: EMPTY_FEED_UPDATED;
	const entries = visible.map(
		(i) => `	<entry>
		<title>${e(i.title)}</title>
		<id>${e(`${base}/#incident-${i.id}`)}</id>
		<link rel="alternate" type="text/html" href="${e(`${base}/#incident-${i.id}`)}"/>
		<published>${e(i.start)}</published>
		<updated>${e(lastUpdated(i))}</updated>
		${i.components.map((c) => `<category term="${e(c)}" label="${e(componentName(c))}"/>`).join('')}
		<content type="html">${e(entryHtml(i, nowMs))}</content>
	</entry>`,
	);
	return `<?xml version="1.0" encoding="utf-8"?>
<feed xmlns="http://www.w3.org/2005/Atom" xml:lang="en-GB">
	<title>${e(env.path === '/' ? 'PrivateCrates status' : `PrivateCrates status: ${env.name}`)}: incidents</title>
	<id>${e(`${base}/feed.xml`)}</id>
	<link rel="self" type="application/atom+xml" href="${e(`${base}/feed.xml`)}"/>
	<link rel="alternate" type="text/html" href="${e(`${base}/`)}"/>
	<updated>${e(updated)}</updated>
	<author><name>PrivateCrates</name><uri>https://privatecrates.dev</uri></author>
${entries.join('\n')}
</feed>
`;
}
