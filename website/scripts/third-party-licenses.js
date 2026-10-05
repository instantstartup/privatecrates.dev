// A Vite plugin that writes /third-party-licenses.txt: every npm package whose code or files reach the browser (the
// Svelte runtime, the fonts), with its licence text. MIT and the SIL Open Font License both require the notice to
// travel with what we serve, and the minifier drops the comments that carried it.
import { readdirSync, readFileSync } from 'node:fs';
import { join } from 'node:path';

/**
 * The package directory and name of a file under node_modules, or null for our own files.
 * @param {string} file
 * @returns {{ name: string, dir: string } | null}
 */
function packageOf(file) {
	const path = file.replace(/\\/g, '/').replace(/^\0/, '').split('?')[0];
	const at = path.lastIndexOf('/node_modules/');
	if (at < 0) return null;
	const rest = path.slice(at + '/node_modules/'.length).split('/');
	const name = rest[0].startsWith('@') ? `${rest[0]}/${rest[1]}` : rest[0];
	return { name, dir: path.slice(0, at) + '/node_modules/' + name };
}

/**
 * @param {string} dir
 * @param {string} name
 */
function licenceText(dir, name) {
	const file = readdirSync(dir).find((f) => /^(licen[cs]e|copying|ofl)/i.test(f));
	if (!file) throw new Error(`third-party-licenses: ${name} has no licence file in ${dir}`);
	return readFileSync(join(dir, file), 'utf8').trim();
}

/** @returns {import('vite').Plugin} */
export function thirdPartyLicenses() {
	return {
		name: 'third-party-licenses',
		apply: 'build',
		generateBundle(_options, bundle) {
			// SvelteKit also builds a server bundle for prerendering; only the client's reaches browsers.
			if (this.environment?.name !== 'client') return;
			/** @type {Map<string, string>} */
			const packages = new Map();
			/** @param {string | undefined} file */
			const add = (file) => {
				const found = file && packageOf(file);
				if (found) packages.set(found.name, found.dir);
			};
			for (const output of Object.values(bundle)) {
				if (output.type === 'chunk') Object.keys(output.modules).forEach(add);
				else (output.originalFileNames ?? []).forEach(add);
			}
			const names = [...packages.keys()].sort();
			const sections = names.map((name) => {
				const dir = /** @type {string} */ (packages.get(name));
				const { version, license } = JSON.parse(readFileSync(join(dir, 'package.json'), 'utf8'));
				return `${'='.repeat(78)}\n${name} ${version} (${license})\n\n${licenceText(dir, name)}\n`;
			});
			this.emitFile({
				type: 'asset',
				fileName: 'third-party-licenses.txt',
				source:
					'Third-party software on privatecrates.dev\n\n' +
					'This website includes the following third-party code and fonts, under the licences below.\n\n' +
					names.map((n) => `  - ${n}`).join('\n') +
					'\n\n' +
					sections.join('\n')
			});
		}
	};
}
