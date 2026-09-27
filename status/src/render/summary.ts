/** `/api/summary.json`: the current state, machine-readable. Stable field names in snake_case. */
import type { Reason } from '../attribution.ts';
import { isActive, type Incident } from '../incidents.ts';
import type { Environment } from '../model.ts';
import type { Latest } from '../store.ts';
import { STALE_AFTER_MS } from './page.ts';

function reason(r: Reason) {
	return { kind: r.kind, text: r.text, detail: r.detail ?? null, url: r.href ?? null, link_text: r.linkText ?? null };
}

export function renderSummary(
	env: Environment,
	latest: Latest | undefined,
	incidents: readonly Incident[],
	origin: string,
	nowMs: number,
) {
	const base = `${origin}${env.path === '/' ? '' : env.path}`;
	return {
		environment: { id: env.id, name: env.name, apex: env.apex },
		page: `${base}/`,
		checked_at: latest?.checkedAt ?? null,
		stale: latest ? nowMs - Date.parse(latest.checkedAt) > STALE_AFTER_MS : true,
		state: latest?.evaluation.state ?? null,
		components: (latest?.evaluation.components ?? []).map((c) => ({
			id: c.id,
			name: c.name,
			state: c.state,
			cause: c.cause,
			reasons: c.reasons.map(reason),
			notes: c.notes.map(reason),
		})),
		upstream: Object.fromEntries(
			Object.entries(latest?.upstreams ?? {}).map(([provider, s]) => [
				provider,
				{
					status_page: s.pageUrl,
					available: s.available,
					description: s.description || null,
					incidents: s.incidents.map((i) => ({ name: i.name, url: i.url, components: i.components, impact: i.impact })),
				},
			]),
		),
		incidents: incidents
			.filter((i) => Date.parse(i.start) <= nowMs)
			.slice(0, 20)
			.map((i) => ({
				id: i.id,
				title: i.title,
				active: isActive(i, nowMs),
				start: i.start,
				end: i.end ?? null,
				impact: i.impact,
				components: i.components,
				url: `${base}/#incident-${i.id}`,
			})),
	};
}
