import { readFileSync, readdirSync } from 'node:fs';
import { join } from 'node:path';
import { describe, expect, it } from 'vitest';
import { activeIncidents, incidentsOnDay, isActive, lastUpdated } from '../src/incidents.ts';
import { parseIncident } from '../src/incidents-parse.ts';
import { renderInline, renderMarkdown } from '../src/markdown.ts';
import { parseJsonc } from '../scripts/build.ts';
import { root } from './helpers.ts';

const ENVS = ['production', 'dev'];

function file(front: string, body = ''): string {
	return `---\n${front}\n---\n${body}`;
}

const MINIMAL = 'title: Sign-in failing\nstart: 2026-10-02T10:05:00Z\ncomponents: [signin]';

describe('parsing incident files', () => {
	it('parses the example, which is marked as an example', () => {
		const source = readFileSync(join(root, 'incidents/_example.md'), 'utf8');
		const { example, incident } = parseIncident('incidents/_example.md', source, ENVS);
		expect(example).toBe(true);
		expect(incident).toMatchObject({
			id: '_example',
			title: 'Publishing failed for some organisations',
			environment: 'production',
			impact: 'degraded',
			start: '2026-09-20T10:05:00.000Z',
			end: '2026-09-20T11:20:00.000Z',
			components: ['publishing', 'downloads'],
		});
		expect(incident.updates.map((u) => u.status)).toEqual(['resolved', 'identified', 'investigating']);
		expect(incident.updates[2]?.html).toContain('<code>cargo publish</code>');
		expect(incident.html).toContain('<h4>What happened</h4>');
		expect(incident.html).toContain('<li>Uploads are now idempotent: a retry finds the asset GitHub already has.</li>');
	});

	it('applies defaults: production, degraded, no end, no updates', () => {
		const { example, incident } = parseIncident('2026-10-02-signin.md', file(MINIMAL), ENVS);
		expect(example).toBe(false);
		expect(incident).toMatchObject({ id: '2026-10-02-signin', environment: 'production', impact: 'degraded', end: undefined, updates: [], html: '' });
	});

	it('normalises times with an offset to UTC', () => {
		const { incident } = parseIncident('a.md', file('title: T\nstart: 2026-10-02T11:05:00+01:00\ncomponents: [website]'), ENVS);
		expect(incident.start).toBe('2026-10-02T10:05:00.000Z');
	});

	const bad: [string, string, RegExp][] = [
		['no front matter', 'just text', /front matter/],
		['invalid YAML', file('title: [unclosed'), /not valid YAML/],
		['missing title', file('start: 2026-10-02T10:05:00Z\ncomponents: [signin]'), /title is required/],
		['a time without a zone', file('title: T\nstart: 2026-10-02T10:05:00\ncomponents: [signin]'), /timestamp with a zone/],
		['end before start', file(`${MINIMAL}\nend: 2026-10-02T09:00:00Z`), /end is before start/],
		['an unknown component', file('title: T\nstart: 2026-10-02T10:05:00Z\ncomponents: [registry]'), /unknown component "registry"/],
		['no components', file('title: T\nstart: 2026-10-02T10:05:00Z\ncomponents: []'), /components must list/],
		['an unknown environment', file(`${MINIMAL}\nenvironment: staging`), /environment must be one of production, dev/],
		['an unknown field', file(`${MINIMAL}\nseverity: high`), /unknown field "severity"/],
		['a bad impact', file(`${MINIMAL}\nimpact: minor`), /impact must be degraded or outage/],
		['an update without text', file(`${MINIMAL}\nupdates:\n  - at: 2026-10-02T10:10:00Z`), /updates\[0\]\.text is required/],
		['an update with a bad status', file(`${MINIMAL}\nupdates:\n  - at: 2026-10-02T10:10:00Z\n    status: fixed\n    text: x`), /updates\[0\]\.status/],
	];
	for (const [what, source, message] of bad) {
		it(`rejects ${what}, naming the file`, () => {
			expect(() => parseIncident('incidents/x.md', source, ENVS)).toThrow(message);
			expect(() => parseIncident('incidents/x.md', source, ENVS)).toThrow(/^incidents\/x\.md: /);
		});
	}

	it('rejects file names that would make bad anchors', () => {
		expect(() => parseIncident('incidents/Big Outage.md', file(MINIMAL), ENVS)).toThrow(/file name/);
	});

	it('every real incident file in the repository parses', () => {
		for (const name of readdirSync(join(root, 'incidents'))) {
			if (!name.endsWith('.md') || name === 'README.md') continue;
			expect(() => parseIncident(name, readFileSync(join(root, 'incidents', name), 'utf8'), ENVS)).not.toThrow();
		}
	});

	it('the environments come from wrangler.jsonc', () => {
		const config = parseJsonc(readFileSync(join(root, 'wrangler.jsonc'), 'utf8')) as { vars: { ENVIRONMENTS: { id: string }[] } };
		expect(config.vars.ENVIRONMENTS.map((e) => e.id)).toEqual(ENVS);
	});
});

describe('incident helpers', () => {
	const { incident } = parseIncident(
		'x.md',
		file(`${MINIMAL}\nend: 2026-10-02T12:00:00Z\nupdates:\n  - at: 2026-10-02T12:30:00Z\n    status: resolved\n    text: Done.`),
		ENVS,
	);

	it('is active between start and end only', () => {
		expect(isActive(incident, Date.parse('2026-10-02T10:00:00Z'))).toBe(false);
		expect(isActive(incident, Date.parse('2026-10-02T11:00:00Z'))).toBe(true);
		expect(isActive(incident, Date.parse('2026-10-02T12:00:00Z'))).toBe(false);
		expect(isActive({ ...incident, end: undefined }, Date.parse('2027-01-01T00:00:00Z'))).toBe(true);
	});

	it('is updated at its latest time', () => {
		expect(lastUpdated(incident)).toBe('2026-10-02T12:30:00.000Z');
	});

	it('links active incidents to the environment’s page', () => {
		const now = Date.parse('2026-10-02T11:00:00Z');
		expect(activeIncidents([incident], now, '/')[0]?.href).toBe('/#incident-x');
		expect(activeIncidents([incident], now, '/dev')[0]?.href).toBe('/dev/#incident-x');
	});

	it('finds incidents overlapping a day', () => {
		const now = Date.parse('2026-10-05T00:00:00Z');
		expect(incidentsOnDay([incident], '2026-10-02', now)).toHaveLength(1);
		expect(incidentsOnDay([incident], '2026-10-01', now)).toHaveLength(0);
		expect(incidentsOnDay([{ ...incident, end: undefined }], '2026-10-04', now)).toHaveLength(1);
	});
});

describe('the Markdown subset', () => {
	it('escapes HTML', () => {
		expect(renderMarkdown('<script>alert(1)</script> & "x"')).toBe('<p>&lt;script&gt;alert(1)&lt;/script&gt; &amp; &quot;x&quot;</p>');
	});

	it('renders links, but only safe ones', () => {
		expect(renderInline('[GitHub](https://www.githubstatus.com)')).toBe('<a href="https://www.githubstatus.com/">GitHub</a>');
		expect(renderInline('[x](javascript:alert(1))')).not.toContain('href');
	});

	it('keeps code spans literal', () => {
		expect(renderInline('`**not bold** <b>`')).toBe('<code>**not bold** &lt;b&gt;</code>');
		expect(renderInline('**bold** and *em*')).toBe('<strong>bold</strong> and <em>em</em>');
	});

	it('renders lists and paragraphs', () => {
		expect(renderMarkdown('One\ntwo\n\n- a\n- b\n\n1. c')).toBe('<p>One two</p>\n<ul><li>a</li><li>b</li></ul>\n<ol><li>c</li></ol>');
	});
});
