/**
 * The status page: plain HTML, no framework, no inline script or style (the CSP forbids both), no third-party
 * requests. Styles and the bars' keyboard script are same-origin static assets (public/).
 */
import { headline, stateLabel, type ComponentStatus, type Evaluation, type Reason } from '../attribution.ts';
import {
	availability,
	barState,
	lastDays,
	RETENTION_DAYS,
	totalAvailability,
	type DaySummary,
} from '../history.ts';
import { incidentsOnDay, isActive, type Incident } from '../incidents.ts';
import { escapeHtml as e } from '../markdown.ts';
import { COMPONENTS, componentName, PROVIDER_NAMES, type Environment, type Provider, type State } from '../model.ts';
import type { Latest } from '../store.ts';
import { safeUrl } from '../statuspage.ts';
import { formatAgo, formatDateTime, formatDay, formatMinutes, formatPercent, formatTime, joinNames } from './format.ts';

/** After this long without a check, the page says the checks are delayed. */
export const STALE_AFTER_MS = 5 * 60_000;

/** Days shown on narrow screens (the rest are hidden by CSS). */
const NARROW_DAYS = 30;

export interface PageInput {
	env: Environment;
	environments: readonly Environment[];
	nowMs: number;
	latest?: Latest;
	days: ReadonlyMap<string, DaySummary>;
	/** This environment's incidents, newest first. */
	incidents: readonly Incident[];
}

export function prefix(env: Environment): string {
	return env.path === '/' ? '' : env.path;
}

const STATE_ICON: Record<State | 'nodata', string> = {
	operational: '<svg class="icon" viewBox="0 0 16 16" aria-hidden="true" focusable="false"><path d="M3 8.5 6.5 12 13 4.5"/></svg>',
	degraded: '<svg class="icon" viewBox="0 0 16 16" aria-hidden="true" focusable="false"><path d="M8 3v6M8 12.5v.5"/></svg>',
	outage: '<svg class="icon" viewBox="0 0 16 16" aria-hidden="true" focusable="false"><path d="M4 4l8 8M12 4l-8 8"/></svg>',
	nodata: '<svg class="icon" viewBox="0 0 16 16" aria-hidden="true" focusable="false"><path d="M4 8h8"/></svg>',
};

function pill(state: State | 'nodata', label = state === 'nodata' ? 'No data' : stateLabel(state)): string {
	return `<span class="pill pill-${state}">${STATE_ICON[state]}${e(label)}</span>`;
}

function reasonHtml(r: Reason): string {
	const href = r.href && (r.href.startsWith('/') ? r.href : safeUrl(r.href));
	const link = href ? `: <a href="${e(href)}">${e(r.linkText ?? 'details')}</a>` : '';
	const detail = r.detail ? ` <span class="detail">(${e(r.detail)})</span>` : '';
	return `${e(r.text)}${link}${detail}`;
}

function verb(state: State): string {
	return state === 'outage' ? 'down' : 'degraded';
}

function banner(input: PageInput): string {
	const { latest, nowMs } = input;
	if (!latest) {
		return `<section class="banner banner-nodata" aria-labelledby="now">
	<h2 id="now">${pill('nodata')} Waiting for the first check</h2>
	<p>Checks run every minute; this page fills in once the first one has finished.</p>
</section>`;
	}
	const ev: Evaluation = latest.evaluation;
	const affected = ev.components.filter((c) => c.state !== 'operational');
	const fine = ev.components.filter((c) => c.state === 'operational').map((c) => c.name);
	const stale = nowMs - Date.parse(latest.checkedAt) > STALE_AFTER_MS;
	const lines = affected.map(
		(c) =>
			`<li><strong>${e(c.name)} ${verb(c.state)}</strong>${c.reasons[0] ? `: ${reasonHtml(c.reasons[0])}` : ''}.</li>`,
	);
	const unaffected =
		affected.length > 0 && fine.length > 0
			? `<p>${e(joinNames(fine))} ${fine.length === 1 ? 'is' : 'are'} unaffected.</p>`
			: '';
	return `<section class="banner banner-${ev.state}" aria-labelledby="now">
	<h2 id="now">${pill(ev.state)} ${e(headline(ev.state))}</h2>
	${lines.length ? `<ul class="affected">${lines.join('')}</ul>` : ''}
	${unaffected}
	<p class="checked">Last checked <time datetime="${e(latest.checkedAt)}">${e(formatDateTime(latest.checkedAt))}</time> (${e(formatAgo(latest.checkedAt, nowMs))}).${
		stale ? ' <strong>Our checks are delayed</strong>; what is shown may be out of date.' : ''
	}</p>
</section>`;
}

function dayDescription(
	name: string,
	day: string,
	summary: DaySummary | undefined,
	id: ComponentStatus['id'],
	incidents: readonly Incident[],
	nowMs: number,
): { state: ReturnType<typeof barState>; text: string } {
	const counts = summary?.counts[id];
	const state = barState(counts);
	const parts = [`${formatDay(day)}, ${name}:`];
	if (!counts || state === 'nodata') {
		parts.push('no data.');
	} else {
		const [, degraded, down] = counts;
		if (state === 'operational') parts.push('no problems seen.');
		else {
			const bits = [];
			if (down) bits.push(`down for ${formatMinutes(down)}`);
			if (degraded) bits.push(`degraded for ${formatMinutes(degraded)}`);
			parts.push(`${bits.join(', ')}.`);
			const reason = summary?.reasons[id];
			if (reason) parts.push(`${reason}.`);
		}
		const share = availability(counts);
		if (share !== undefined) parts.push(`${formatPercent(share)} available.`);
	}
	const related = incidentsOnDay(incidents, day, nowMs).filter((i) => i.components.includes(id));
	for (const i of related) parts.push(`Incident: ${i.title}.`);
	return { state, text: parts.join(' ') };
}

function bars(input: PageInput, c: ComponentStatus, days: readonly string[]): string {
	const items = days.map((day) => {
		const { state, text } = dayDescription(c.name, day, input.days.get(day), c.id, input.incidents, input.nowMs);
		return `<li><span class="bar bar-${state}" tabindex="0" role="img" aria-label="${e(text)}"></span><span class="tip" aria-hidden="true">${e(text)}</span></li>`;
	});
	const total = totalAvailability(
		days.map((d) => input.days.get(d)),
		c.id,
	);
	const totalNarrow = totalAvailability(
		days.slice(-NARROW_DAYS).map((d) => input.days.get(d)),
		c.id,
	);
	return `<div class="history">
	<ol class="bars" aria-label="${e(c.name)}: one bar per day, oldest first">${items.join('')}</ol>
	<p class="axis"><span><span class="wide">${RETENTION_DAYS} days ago</span><span class="narrow">${NARROW_DAYS} days ago</span></span><span class="uptime"><span class="wide">${
		total === undefined ? 'No data yet' : `${formatPercent(total)} available`
	}</span><span class="narrow">${totalNarrow === undefined ? 'No data yet' : `${formatPercent(totalNarrow)} available`}</span></span><span>Today</span></p>
</div>`;
}

function componentCard(input: PageInput, c: ComponentStatus, days: readonly string[], known: boolean): string {
	const def = COMPONENTS.find((d) => d.id === c.id);
	const detail = def?.detail ? `<p class="gloss">${e(def.detail).replace(/`([^`]+)`/g, '<code>$1</code>')}</p>` : '';
	const reasons = c.reasons.length ? `<ul class="reasons">${c.reasons.map((r) => `<li>${reasonHtml(r)}</li>`).join('')}</ul>` : '';
	const notes = c.notes.length
		? `<ul class="notes">${c.notes.map((r) => `<li>${reasonHtml(r)}</li>`).join('')}</ul>`
		: '';
	return `<li class="component panel" id="component-${e(c.id)}">
	<div class="component-head">
		<h3>${e(c.name)}</h3>
		${pill(known ? c.state : 'nodata')}
	</div>
	${detail}
	${reasons}
	${notes}
	${bars(input, c, days)}
</li>`;
}

function components(input: PageInput): string {
	const days = lastDays(input.nowMs);
	const current: ComponentStatus[] =
		input.latest?.evaluation.components ??
		COMPONENTS.map((d) => ({ id: d.id, name: d.name, state: 'operational', cause: 'none', reasons: [], notes: [] }));
	const cards = current.map((c) => componentCard(input, c, days, input.latest !== undefined));
	return `<section aria-labelledby="components">
	<h2 id="components">Components</h2>
	<p class="hint">Each bar is a day (UTC). Hover over or focus a bar for that day; use the arrow keys to move between days.</p>
	<ul class="components">${cards.join('')}</ul>
</section>`;
}

function upstreamSection(input: PageInput): string {
	const providers: Provider[] = ['github', 'stripe'];
	const pages: Record<Provider, string> = { github: 'https://www.githubstatus.com', stripe: 'https://status.stripe.com' };
	const items = providers.map((p) => {
		const snap = input.latest?.upstreams[p];
		const page = (snap && safeUrl(snap.pageUrl)) ?? pages[p];
		let status: string;
		if (!snap) status = 'Not checked yet.';
		else if (!snap.available) status = 'We could not read its status automatically; see its status page.';
		else {
			status = `${e(snap.description || 'Status read')}.`;
			if (snap.incidents.length) {
				status += `<ul>${snap.incidents
					.map((i) => `<li><a href="${e(i.url)}">${e(i.name)}</a>${i.components.length ? ` (${e(joinNames(i.components))})` : ''}</li>`)
					.join('')}</ul>`;
			}
		}
		const uses = p === 'github' ? 'API Requests, Git Operations, Webhooks and Actions' : 'billing';
		return `<li><h3>${PROVIDER_NAMES[p]}</h3><p class="gloss">We depend on it for ${uses}.</p><p>${status}</p><p><a href="${e(page)}">${PROVIDER_NAMES[p]}'s status page</a></p></li>`;
	});
	return `<section aria-labelledby="upstream">
	<h2 id="upstream">Services we depend on</h2>
	<ul class="upstream">${items.join('')}</ul>
</section>`;
}

function incidentArticle(i: Incident, nowMs: number): string {
	const active = isActive(i, nowMs);
	const when = i.end
		? `<time datetime="${e(i.start)}">${e(formatDateTime(i.start))}</time> to <time datetime="${e(i.end)}">${
				formatDay(i.end) === formatDay(i.start) ? e(formatTime(i.end)) : e(formatDateTime(i.end))
			}</time>`
		: `Since <time datetime="${e(i.start)}">${e(formatDateTime(i.start))}</time>`;
	const updates = i.updates
		.map(
			(u) =>
				`<li><p class="update-head"><strong>${e(u.status.charAt(0).toUpperCase() + u.status.slice(1))}</strong> <time datetime="${e(u.at)}">${e(formatDateTime(u.at))}</time></p>${u.html}</li>`,
		)
		.join('');
	return `<article class="incident panel${active ? ' incident-active' : ''}" id="incident-${e(i.id)}" aria-labelledby="incident-${e(i.id)}-title">
	<h3 id="incident-${e(i.id)}-title">${e(i.title)}</h3>
	<p class="meta">${active ? pill(i.impact, 'Ongoing') + ' ' : ''}${when}. Affects ${e(joinNames(i.components.map(componentName)))}.</p>
	${updates ? `<ol class="updates">${updates}</ol>` : ''}
	${i.html ? `<div class="writeup">${i.html}</div>` : ''}
</article>`;
}

function incidentSections(input: PageInput): { active: string; past: string } {
	const cutoff = input.nowMs - RETENTION_DAYS * 86_400_000;
	const active = input.incidents.filter((i) => isActive(i, input.nowMs));
	const past = input.incidents.filter(
		(i) => !isActive(i, input.nowMs) && Date.parse(i.end ?? i.start) >= cutoff && Date.parse(i.start) <= input.nowMs,
	);
	return {
		active: active.length
			? `<section aria-labelledby="active-incidents">
	<h2 id="active-incidents">Ongoing incidents</h2>
	${active.map((i) => incidentArticle(i, input.nowMs)).join('')}
</section>`
			: '',
		past: `<section aria-labelledby="past-incidents">
	<h2 id="past-incidents">Past incidents</h2>
	${past.length ? past.map((i) => incidentArticle(i, input.nowMs)).join('') : `<p>No incidents in the last ${RETENTION_DAYS} days.</p>`}
</section>`,
	};
}

const LOGO = `<svg class="mark" viewBox="0 0 64 48" aria-hidden="true" focusable="false"><rect class="crate-body" x="2" y="2" width="60" height="44" rx="2"/><path class="crate-batten" d="M2 9.5h60M2 38.5h60M11 2v44M53 2v44M11 9.5 53 38.5"/></svg>`;

export function renderPage(input: PageInput): string {
	const { env, environments } = input;
	const p = prefix(env);
	const production = env.path === '/';
	const title = production ? 'PrivateCrates status' : `PrivateCrates status: ${env.name}`;
	const nav = environments
		.map(
			(x) =>
				`<li><a href="${e(x.path)}"${x.id === env.id ? ' aria-current="page"' : ''}>${e(x.name)}</a></li>`,
		)
		.join('');
	const incidents = incidentSections(input);
	const devNotice = production
		? ''
		: `<p class="env-notice" role="note"><strong>Development environment.</strong> This page watches <code>${e(env.apex)}</code>, where we try changes before production. It is not for customers and breaks more often; production is on <a href="/">the main status page</a>.</p>`;
	return `<!doctype html>
<html lang="en-GB">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>${e(title)}</title>
<meta name="description" content="Live status of ${e(env.apex)}: the PrivateCrates registry, downloads, publishing, sign-in, website and billing, and the GitHub and Stripe services they rely on.">
<meta name="color-scheme" content="light dark">
${production ? '' : '<meta name="robots" content="noindex">\n'}<link rel="icon" href="/favicon.svg" type="image/svg+xml">
<link rel="preload" href="/fonts/big-shoulders-latin-opsz-normal.woff2" as="font" type="font/woff2" crossorigin>
<link rel="preload" href="/fonts/atkinson-hyperlegible-next-latin-wght-normal.woff2" as="font" type="font/woff2" crossorigin>
<link rel="stylesheet" href="/status.css">
<link rel="alternate" type="application/atom+xml" title="${e(title)}: incidents" href="${p}/feed.xml">
<script src="/bars.js" defer></script>
</head>
<body${production ? '' : ' class="env-dev"'}>
<a class="skip" href="#main">Skip to content</a>
<header class="site-header">
	<div class="page header-bar">
		<a class="logo" href="/">${LOGO}<span class="word">PrivateCrates <span class="word-status">status</span></span></a>
		<nav aria-label="Environments"><ul>${nav}</ul></nav>
	</div>
</header>
<main id="main" class="page" tabindex="-1">
	<h1>${production ? 'Service status' : `${e(env.name)} status`}</h1>
	${devNotice}
	${banner(input)}
	${incidents.active}
	${components(input)}
	${upstreamSection(input)}
	${incidents.past}
</main>
<footer class="site-footer">
	<div class="page">
		<p>Checked every minute from Cloudflare, independently of our servers: <code>/healthz</code> and <code>/api/status</code> on <code>${e(env.apex)}</code>, and a canary registry answering Cargo. Times are UTC.</p>
		<ul class="links">
			<li><a href="${p}/feed.xml">Incident feed (Atom)</a></li>
			<li><a href="${p}/api/summary.json">Current status as JSON</a></li>
			<li><a href="https://${e(env.apex)}/">${e(env.apex)}</a></li>
		</ul>
	</div>
</footer>
</body>
</html>
`;
}
