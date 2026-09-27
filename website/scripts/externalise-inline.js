// Post-build step for a strict Content-Security-Policy (`script-src 'self'; style-src 'self'`, no inline).
//
// SvelteKit writes a small inline <script> into every page to start the app. This moves each one into its own
// file under build/_app/boot/, named by content hash, and loads it with <script src>. The scripts rely on
// document.currentScript, which works for classic (non-module, non-async) external scripts too.
//
// It then checks the build: no inline scripts, style attributes or <style> elements remain, and the mock API
// (for `pnpm dev:mock` only) is nowhere in the output. Any failure fails the build.

import { createHash } from 'node:crypto';
import { mkdirSync, readdirSync, readFileSync, statSync, writeFileSync } from 'node:fs';
import { join, relative } from 'node:path';

const BUILD = 'build';
const BOOT_DIR = join(BUILD, '_app', 'boot');

function walk(dir) {
	return readdirSync(dir).flatMap((name) => {
		const path = join(dir, name);
		return statSync(path).isDirectory() ? walk(path) : [path];
	});
}

const files = walk(BUILD);
const problems = [];
let moved = 0;

mkdirSync(BOOT_DIR, { recursive: true });

for (const file of files.filter((f) => f.endsWith('.html'))) {
	let html = readFileSync(file, 'utf8');
	html = html.replace(/<script>([\s\S]*?)<\/script>/g, (_, body) => {
		const hash = createHash('sha256').update(body).digest('hex').slice(0, 16);
		writeFileSync(join(BOOT_DIR, `${hash}.js`), body.trim() + '\n');
		moved += 1;
		return `<script src="/_app/boot/${hash}.js"></script>`;
	});
	writeFileSync(file, html);

	const name = relative(BUILD, file);
	if (/<script(?![^>]*\ssrc=)[^>]*>/.test(html)) problems.push(`${name}: inline <script> remains`);
	if (/<style[\s>]/.test(html)) problems.push(`${name}: inline <style> element`);
	if (/\sstyle="/.test(html)) problems.push(`${name}: style attribute (blocked by the CSP)`);
	if (/\son[a-z]+="/.test(html)) problems.push(`${name}: inline event handler`);
}

for (const file of walk(BUILD).filter((f) => f.endsWith('.js'))) {
	if (/mockFetch|pc-mock|mock_done/.test(readFileSync(file, 'utf8'))) {
		problems.push(`${relative(BUILD, file)}: contains the development mock API`);
	}
}

if (problems.length) {
	console.error('CSP check failed:\n  ' + problems.join('\n  '));
	process.exit(1);
}
console.log(`CSP check passed: moved ${moved} inline bootstrap scripts to _app/boot/.`);
