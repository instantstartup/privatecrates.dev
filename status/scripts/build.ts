// Build step (run by Wrangler before `deploy` and `dev`, see wrangler.jsonc; or `pnpm build`):
//   1. parse status/incidents/*.md into src/generated/incidents.json (examples and README.md are left out);
//   2. copy the website's two variable fonts (Latin subset) into public/fonts, so the page makes no third-party requests,
//      with their licences, also gathered in public/third-party-licenses.txt.
// Node runs this TypeScript directly (type stripping).
import { copyFileSync, existsSync, mkdirSync, readdirSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { parseIncident } from '../src/incidents-parse.ts';
import type { Incident } from '../src/incidents.ts';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');

/** Strips // and /* comments and trailing commas from JSONC, leaving strings alone. */
export function parseJsonc(source: string): unknown {
	let out = '';
	for (let i = 0; i < source.length; i++) {
		const ch = source[i]!;
		if (ch === '"') {
			let j = i + 1;
			while (j < source.length && source[j] !== '"') j += source[j] === '\\' ? 2 : 1;
			out += source.slice(i, j + 1);
			i = j;
		} else if (ch === '/' && source[i + 1] === '/') {
			while (i < source.length && source[i] !== '\n') i++;
			out += '\n';
		} else if (ch === '/' && source[i + 1] === '*') {
			i = source.indexOf('*/', i + 2) + 1;
		} else {
			out += ch;
		}
	}
	return JSON.parse(out.replace(/,(\s*[}\]])/g, '$1'));
}

function environmentIds(): string[] {
	const config = parseJsonc(readFileSync(join(root, 'wrangler.jsonc'), 'utf8')) as {
		vars?: { ENVIRONMENTS?: { id: string }[] };
	};
	return (config.vars?.ENVIRONMENTS ?? []).map((e) => e.id);
}

function buildIncidents(): Incident[] {
	const dir = join(root, 'incidents');
	const environments = environmentIds();
	const incidents: Incident[] = [];
	for (const name of readdirSync(dir).sort()) {
		if (!name.endsWith('.md') || name === 'README.md') continue;
		const parsed = parseIncident(`incidents/${name}`, readFileSync(join(dir, name), 'utf8'), environments);
		if (parsed.example) continue;
		incidents.push(parsed.incident);
	}
	incidents.sort((a, b) => Date.parse(b.start) - Date.parse(a.start));
	mkdirSync(join(root, 'src/generated'), { recursive: true });
	const out = join(root, 'src/generated/incidents.json');
	const json = `${JSON.stringify(incidents, null, '\t')}\n`;
	// Only write on a change: `wrangler dev` watches src/ and would otherwise rebuild for ever.
	if (!existsSync(out) || readFileSync(out, 'utf8') !== json) writeFileSync(out, json);
	return incidents;
}

function copyFonts(): void {
	const out = join(root, 'public/fonts');
	mkdirSync(out, { recursive: true });
	const fonts = [
		['@fontsource-variable/big-shoulders', 'big-shoulders-latin-opsz-normal.woff2'],
		['@fontsource-variable/atkinson-hyperlegible-next', 'atkinson-hyperlegible-next-latin-wght-normal.woff2'],
	];
	const notices = [];
	for (const [pkg, file] of fonts) {
		const dir = join(root, 'node_modules', pkg!);
		copyFileSync(join(dir, 'files', file!), join(out, file!));
		// Both are under the SIL Open Font License; it travels with the font.
		copyFileSync(join(dir, 'LICENSE'), join(out, `${file!.replace(/-latin.*$/, '')}-LICENSE.txt`));
		const { version } = JSON.parse(readFileSync(join(dir, 'package.json'), 'utf8')) as { version: string };
		notices.push(`${'='.repeat(78)}\n${pkg} ${version} (OFL-1.1)\n\n${readFileSync(join(dir, 'LICENSE'), 'utf8').trim()}\n`);
	}
	// Linked from the page's footer: the only third-party files the page serves are these fonts.
	writeFileSync(
		join(root, 'public/third-party-licenses.txt'),
		`Third-party software on the PrivateCrates status page\n\nThis page uses the following fonts, under the licence below.\n\n${notices.join('\n')}`,
	);
}

if (import.meta.url === `file://${process.argv[1]}`) {
	const incidents = buildIncidents();
	copyFonts();
	console.log(`status build: ${incidents.length} incident(s), fonts copied`);
}
