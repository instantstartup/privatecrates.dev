/**
 * Parses Atlassian Statuspage's public `GET /api/v2/summary.json` (GitHub's status page is one) into the little we
 * need: each component's status and the unresolved incidents, with the components they affect.
 */
import type { Provider, State } from './model.ts';

export type UpstreamStatus =
	| 'operational'
	| 'degraded_performance'
	| 'partial_outage'
	| 'major_outage'
	| 'under_maintenance';

const UPSTREAM_STATUSES: readonly string[] = [
	'operational',
	'degraded_performance',
	'partial_outage',
	'major_outage',
	'under_maintenance',
];

export interface UpstreamComponent {
	id: string;
	name: string;
	status: UpstreamStatus;
}

export interface UpstreamIncident {
	id: string;
	name: string;
	status: string;
	impact: string;
	/** Statuspage's short link to the incident. */
	url: string;
	/** Names of the affected components; may be empty when the provider has not said. */
	components: string[];
	startedAt: string;
	updatedAt: string;
}

export interface UpstreamSnapshot {
	provider: Provider;
	/** The human status page, linked from ours. */
	pageUrl: string;
	fetchedAt: string;
	/** False when the summary could not be fetched or parsed; then components and incidents are empty. */
	available: boolean;
	error?: string;
	description: string;
	components: UpstreamComponent[];
	incidents: UpstreamIncident[];
}

/** How an upstream component's status maps onto ours. */
export function upstreamState(status: UpstreamStatus): State {
	switch (status) {
		case 'operational':
			return 'operational';
		case 'major_outage':
			return 'outage';
		default:
			return 'degraded';
	}
}

export function describeUpstreamStatus(status: UpstreamStatus): string {
	switch (status) {
		case 'operational':
			return 'operational';
		case 'degraded_performance':
			return 'degraded performance';
		case 'partial_outage':
			return 'a partial outage';
		case 'major_outage':
			return 'a major outage';
		case 'under_maintenance':
			return 'maintenance';
	}
}

function isRecord(value: unknown): value is Record<string, unknown> {
	return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function str(value: unknown): string {
	return typeof value === 'string' ? value : '';
}

export function unavailable(provider: Provider, pageUrl: string, fetchedAt: string, error: string): UpstreamSnapshot {
	return { provider, pageUrl, fetchedAt, available: false, error, description: '', components: [], incidents: [] };
}

/** Parses a summary.json body. Anything that is not a Statuspage summary gives an unavailable snapshot. */
export function parseSummary(provider: Provider, pageUrl: string, fetchedAt: string, body: unknown): UpstreamSnapshot {
	if (!isRecord(body) || !Array.isArray(body.components) || !Array.isArray(body.incidents)) {
		return unavailable(provider, pageUrl, fetchedAt, 'not a Statuspage summary');
	}
	const components: UpstreamComponent[] = [];
	for (const c of body.components) {
		// Groups carry no status of their own that we need; their children are listed separately.
		if (!isRecord(c) || c.group === true) continue;
		const status = str(c.status);
		if (!UPSTREAM_STATUSES.includes(status)) continue;
		components.push({ id: str(c.id), name: str(c.name), status: status as UpstreamStatus });
	}
	const byId = new Map(components.map((c) => [c.id, c.name]));
	const incidents: UpstreamIncident[] = [];
	for (const i of body.incidents) {
		if (!isRecord(i)) continue;
		const status = str(i.status);
		if (status === 'resolved' || status === 'postmortem') continue;
		const names = new Set<string>();
		if (Array.isArray(i.components)) {
			for (const c of i.components) if (isRecord(c) && str(c.name)) names.add(str(c.name));
		}
		// Older incidents name components only in their updates.
		if (Array.isArray(i.incident_updates)) {
			for (const u of i.incident_updates) {
				if (!isRecord(u) || !Array.isArray(u.affected_components)) continue;
				for (const c of u.affected_components) {
					if (!isRecord(c)) continue;
					const name = str(c.name) || byId.get(str(c.code)) || '';
					if (name) names.add(name);
				}
			}
		}
		incidents.push({
			id: str(i.id),
			name: str(i.name) || 'Incident',
			status,
			impact: str(i.impact),
			url: safeUrl(str(i.shortlink)) ?? pageUrl,
			components: [...names],
			startedAt: str(i.started_at) || str(i.created_at),
			updatedAt: str(i.updated_at),
		});
	}
	const status = isRecord(body.status) ? str(body.status.description) : '';
	return { provider, pageUrl, fetchedAt, available: true, description: status, components, incidents };
}

/** Only https links from upstream data reach our page. */
export function safeUrl(value: string): string | undefined {
	try {
		const url = new URL(value);
		return url.protocol === 'https:' ? url.toString() : undefined;
	} catch {
		return undefined;
	}
}

/** The status of a named component, or of the provider as a whole for `*` (its worst component). */
export function componentStatus(snapshot: UpstreamSnapshot, name: string): UpstreamStatus | undefined {
	if (!snapshot.available) return undefined;
	if (name === '*') {
		const order: UpstreamStatus[] = [
			'major_outage',
			'partial_outage',
			'degraded_performance',
			'under_maintenance',
			'operational',
		];
		const present = new Set(snapshot.components.map((c) => c.status));
		return order.find((s) => present.has(s)) ?? 'operational';
	}
	return snapshot.components.find((c) => c.name === name)?.status;
}

/** The unresolved incident that names this component (or any incident, for `*`), if the provider has linked one. */
export function incidentFor(snapshot: UpstreamSnapshot, name: string): UpstreamIncident | undefined {
	if (name === '*') return snapshot.incidents[0];
	return snapshot.incidents.find((i) => i.components.includes(name));
}
