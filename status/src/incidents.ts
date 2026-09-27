/**
 * Our own incident reports: the model the Worker uses at runtime, and helpers over it.
 * Parsing the Markdown files happens at build time, in `incidents-parse.ts`.
 */
import type { ActiveIncident } from './attribution.ts';
import type { ComponentId, State } from './model.ts';

export interface IncidentUpdate {
	at: string;
	/** investigating | identified | monitoring | resolved | update */
	status: string;
	html: string;
}

export interface Incident {
	/** From the file name: `2026-09-20-publish-errors.md` is `2026-09-20-publish-errors`. */
	id: string;
	title: string;
	/** Which environment's page it belongs on: an environment id from `ENVIRONMENTS`. */
	environment: string;
	start: string;
	end?: string;
	impact: Exclude<State, 'operational'>;
	components: ComponentId[];
	/** The write-up (the Markdown body), rendered. */
	html: string;
	/** Newest first. */
	updates: IncidentUpdate[];
}

export function isActive(incident: Incident, nowMs: number): boolean {
	return Date.parse(incident.start) <= nowMs && (!incident.end || Date.parse(incident.end) > nowMs);
}

export function lastUpdated(incident: Incident): string {
	const times = [incident.start, incident.end, ...incident.updates.map((u) => u.at)].filter(
		(t): t is string => typeof t === 'string',
	);
	return times.reduce((a, b) => (Date.parse(b) > Date.parse(a) ? b : a));
}

export function forEnvironment(incidents: readonly Incident[], environment: string): Incident[] {
	return incidents
		.filter((i) => i.environment === environment)
		.sort((a, b) => Date.parse(b.start) - Date.parse(a.start));
}

export function activeIncidents(incidents: readonly Incident[], nowMs: number, pagePath: string): ActiveIncident[] {
	return incidents
		.filter((i) => isActive(i, nowMs))
		.map((i) => ({
			id: i.id,
			title: i.title,
			components: i.components,
			impact: i.impact,
			href: `${pagePath === '/' ? '' : pagePath}/#incident-${i.id}`,
		}));
}

/** Incidents overlapping a UTC day. */
export function incidentsOnDay(incidents: readonly Incident[], day: string, nowMs: number): Incident[] {
	const from = Date.parse(`${day}T00:00:00Z`);
	const to = from + 86_400_000;
	return incidents.filter((i) => {
		const start = Date.parse(i.start);
		const end = i.end ? Date.parse(i.end) : nowMs;
		return start < to && end >= from;
	});
}
