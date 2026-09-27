/**
 * Parses an incident file (status/incidents/*.md): YAML front matter, then an optional Markdown write-up.
 * Build time only (it uses the `yaml` package, which is never bundled into the Worker). Strict: a mistake fails the
 * build with the file name and the problem, rather than publishing a wrong incident.
 */
import { parse as parseYaml } from 'yaml';
import type { Incident, IncidentUpdate } from './incidents.ts';
import { renderMarkdown } from './markdown.ts';
import { COMPONENT_IDS, isComponentId, type ComponentId } from './model.ts';

export class IncidentError extends Error {
	constructor(file: string, message: string) {
		super(`${file}: ${message}`);
		this.name = 'IncidentError';
	}
}

const KEYS = ['title', 'start', 'end', 'environment', 'impact', 'components', 'updates', 'example'];
const UPDATE_KEYS = ['at', 'status', 'text'];
const UPDATE_STATUSES = ['investigating', 'identified', 'monitoring', 'resolved', 'update'];

export interface ParsedFile {
	/** `example: true` files document the format and are left out of the build. */
	example: boolean;
	incident: Incident;
}

function isRecord(value: unknown): value is Record<string, unknown> {
	return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function timestamp(file: string, field: string, value: unknown): string {
	// Require an explicit zone, so nobody's local time is guessed.
	if (typeof value !== 'string' || !/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}(:\d{2}(\.\d+)?)?(Z|[+-]\d{2}:\d{2})$/.test(value)) {
		throw new IncidentError(file, `${field} must be a timestamp with a zone, like 2026-09-20T10:05:00Z`);
	}
	const ms = Date.parse(value);
	if (Number.isNaN(ms)) throw new IncidentError(file, `${field} is not a valid time: ${value}`);
	return new Date(ms).toISOString();
}

export function splitFrontMatter(file: string, source: string): { data: unknown; body: string } {
	const match = /^---\r?\n([\s\S]*?)\r?\n---\r?\n?([\s\S]*)$/.exec(source);
	if (!match) throw new IncidentError(file, 'must start with front matter between --- lines');
	try {
		return { data: parseYaml(match[1]!), body: match[2]! };
	} catch (e) {
		throw new IncidentError(file, `front matter is not valid YAML: ${(e as Error).message}`);
	}
}

export function parseIncident(file: string, source: string, environments: readonly string[]): ParsedFile {
	const id = file.replace(/^.*\//, '').replace(/\.md$/, '');
	if (!/^_?[a-z0-9][a-z0-9-]*$/.test(id)) {
		throw new IncidentError(file, 'the file name must be lower case letters, digits and hyphens, ending .md');
	}
	const { data, body } = splitFrontMatter(file, source);
	if (!isRecord(data)) throw new IncidentError(file, 'front matter must be a mapping');
	for (const key of Object.keys(data)) {
		if (!KEYS.includes(key)) throw new IncidentError(file, `unknown field "${key}" (expected ${KEYS.join(', ')})`);
	}

	if (typeof data.title !== 'string' || data.title.trim() === '') throw new IncidentError(file, 'title is required');
	const start = timestamp(file, 'start', data.start);
	const end = data.end === undefined || data.end === null ? undefined : timestamp(file, 'end', data.end);
	if (end && Date.parse(end) < Date.parse(start)) throw new IncidentError(file, 'end is before start');

	const environment = data.environment ?? 'production';
	if (typeof environment !== 'string' || !environments.includes(environment)) {
		throw new IncidentError(file, `environment must be one of ${environments.join(', ')}`);
	}

	const impact = data.impact ?? 'degraded';
	if (impact !== 'degraded' && impact !== 'outage') throw new IncidentError(file, 'impact must be degraded or outage');

	if (!Array.isArray(data.components) || data.components.length === 0) {
		throw new IncidentError(file, `components must list at least one of ${COMPONENT_IDS.join(', ')}`);
	}
	const components: ComponentId[] = [];
	for (const c of data.components) {
		if (typeof c !== 'string' || !isComponentId(c)) {
			throw new IncidentError(file, `unknown component "${String(c)}" (expected ${COMPONENT_IDS.join(', ')})`);
		}
		if (!components.includes(c)) components.push(c);
	}

	const updates: IncidentUpdate[] = [];
	if (data.updates !== undefined) {
		if (!Array.isArray(data.updates)) throw new IncidentError(file, 'updates must be a list');
		data.updates.forEach((u, n) => {
			const where = `updates[${n}]`;
			if (!isRecord(u)) throw new IncidentError(file, `${where} must be a mapping with at, status and text`);
			for (const key of Object.keys(u)) {
				if (!UPDATE_KEYS.includes(key)) throw new IncidentError(file, `${where}: unknown field "${key}"`);
			}
			const at = timestamp(file, `${where}.at`, u.at);
			const status = u.status ?? 'update';
			if (typeof status !== 'string' || !UPDATE_STATUSES.includes(status)) {
				throw new IncidentError(file, `${where}.status must be one of ${UPDATE_STATUSES.join(', ')}`);
			}
			if (typeof u.text !== 'string' || u.text.trim() === '') throw new IncidentError(file, `${where}.text is required`);
			updates.push({ at, status, html: renderMarkdown(u.text) });
		});
	}
	updates.sort((a, b) => Date.parse(b.at) - Date.parse(a.at));

	if (data.example !== undefined && typeof data.example !== 'boolean') {
		throw new IncidentError(file, 'example must be true or false');
	}

	return {
		example: data.example === true,
		incident: {
			id,
			title: data.title.trim(),
			environment,
			start,
			end,
			impact,
			components,
			html: renderMarkdown(body.trim()),
			updates,
		},
	};
}
