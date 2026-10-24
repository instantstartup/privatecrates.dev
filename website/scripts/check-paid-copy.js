// Fails when the legal pages, the trust centre or the footer say something that is true only during the preview
// ("preview", "draft", "not yet in force", "general availability", "as is") outside a PREVIEW branch. PREVIEW is a
// constant, so the build only ever renders one mode; this reads the source instead, so paid-mode copy cannot keep a
// preview statement nobody sees until billing is turned on.
//
// A statement is in a PREVIEW branch when it sits in `{#if PREVIEW}` (before its `{:else}`), after the `{:else}` of
// `{#if !PREVIEW}`, or in the string right after `PREVIEW ?`. Comments are skipped.
//
// Run: node scripts/check-paid-copy.js [files...] (default: the files below; `pnpm lint` runs it).

import { readFileSync } from 'node:fs';
import { argv, exit } from 'node:process';

const FILES = [
	'src/lib/components/Footer.svelte',
	'src/lib/legal/PaidTerms.svelte',
	'src/lib/trust.ts',
	'src/routes/legal/dpa/+page.svelte',
	'src/routes/legal/privacy/+page.svelte',
	'src/routes/legal/refunds/+page.svelte',
	'src/routes/legal/terms/+page.svelte',
	'src/routes/llms.txt/+server.ts',
	'src/routes/trust/+page.svelte'
];

const PREVIEW_ONLY = /\bpreview\b|\bdrafts?\b|not yet in force|general availability|provided as is/gi;

/** Replaces text with spaces, keeping newlines, so line numbers stay right. */
const blank = (text) => text.replace(/[^\n]/g, ' ');

/** Blanks comments and the preview arm of every `PREVIEW ? <string>` ternary. */
export function stripNonPaid(source) {
	const string = String.raw`'(?:[^'\\]|\\.)*'|"(?:[^"\\]|\\.)*"|` + '`(?:[^`\\\\]|\\\\.)*`';
	return source
		.replace(/<!--[\s\S]*?-->|\/\*[\s\S]*?\*\/|(?<![:'"`])\/\/[^\n]*/g, blank)
		.replace(
			new RegExp(String.raw`(\bPREVIEW\s*\?\s*)(${string})`, 'g'),
			(_, head, arm) => head + blank(arm)
		);
}

/** Every preview-only phrase that paid mode would render: `{ line, text }`. */
export function findings(source) {
	const out = [];
	/** One entry per open `{#if}`: whether the branch we are in renders in paid mode. */
	const stack = [];
	const lines = stripNonPaid(source).split('\n');
	lines.forEach((line, i) => {
		const tags = line.matchAll(/\{(#if\s+(!?)\s*PREVIEW\s*\}|#if\b|:else if\b|:else\s*\}|\/if\s*\})/g);
		let paid = stack.every((open) => open.paid);
		let checked = line;
		for (const tag of tags) {
			const [whole, body, not] = tag;
			// Text before the tag is in the branch that was open before it.
			if (paid) check(checked.slice(0, tag.index), i);
			checked = ' '.repeat(tag.index + whole.length) + checked.slice(tag.index + whole.length);
			if (body.startsWith('#if') && body.includes('PREVIEW')) stack.push({ preview: !not, paid: !!not });
			else if (body.startsWith('#if')) stack.push({ paid: true });
			else if (body.startsWith(':else') && stack.length) {
				const top = stack.at(-1);
				if ('preview' in top) top.paid = !top.paid;
			} else if (body.startsWith('/if')) stack.pop();
			paid = stack.every((open) => open.paid);
		}
		if (paid) check(checked, i);
	});
	function check(text, i) {
		for (const match of text.matchAll(PREVIEW_ONLY)) {
			if (match[0] === 'PREVIEW') continue;
			out.push({ line: i + 1, text: lines[i].trim() });
			return;
		}
	}
	return out;
}

if (import.meta.url === `file://${argv[1]}`) {
	let failed = false;
	for (const file of argv.length > 2 ? argv.slice(2) : FILES) {
		for (const { line, text } of findings(readFileSync(file, 'utf8'))) {
			console.error(`${file}:${line}: preview-only wording outside a PREVIEW branch: ${text}`);
			failed = true;
		}
	}
	if (failed) exit(1);
}
