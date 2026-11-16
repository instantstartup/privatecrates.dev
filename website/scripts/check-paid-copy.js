// Fails when the website says something that is true only during the preview ("the preview", "preview terms",
// "billing is off", "draft", "not yet in force", "general availability", "as is") outside a branch on the preview
// switch. PREVIEW is a constant, so the build only ever renders one mode; this reads the source instead, so paid-mode
// copy cannot keep a preview statement nobody sees until billing is turned on.
//
// The switch is `PREVIEW` ($lib/site) on static pages and `preview` (the server's PREVIEW, from `session.preview`) on
// the account page. A statement is in a preview branch when it sits in `{#if preview}` (before its `{:else}`), after
// the `{:else}` of `{#if !preview}`, in the string or bracketed expression right after `preview ?`, or in the one
// after `if (preview)` or `if (preview) return`. Comments and style blocks are skipped.
//
// Run: node scripts/check-paid-copy.js [files...] (default: every page, component and module under src and static,
// except the mock API, which replays the server's own preview responses; `pnpm lint` runs it).

import { readdirSync, readFileSync } from 'node:fs';
import { join } from 'node:path';
import { argv, exit } from 'node:process';

const ROOTS = ['src', 'static'];
const SKIP = join('src', 'lib', 'mock');
const EXTENSIONS = /\.(svelte|ts|js|txt|html)$/;

/** Matched across line breaks, as prose wraps. */
const PREVIEW_ONLY =
	/\bthe\s+(?:private\s+)?preview\b|\bin\s+preview\b|\bpreview\s+terms\b|\bbilling\s+is\s+off\b|\bdrafts?\b|\bnot\s+yet\s+in\s+force\b|\bgeneral\s+availability\b|\bprovided\s+as\s+is\b/gi;

/** The switch as a condition on its own: not negated, and not part of a longer name (`--size-preview`). */
const SWITCH = String.raw`(?<!!\s*)(?<![\w$-])(?:PREVIEW|preview)\b`;
/** What comes right before a preview-only arm. */
const ARM = new RegExp(
	String.raw`${SWITCH}\s*\?(?![?.])\s*|\bif\s*\(\s*${SWITCH}\s*\)\s*(?:return\s+)?`,
	'g'
);
const BLOCK = new RegExp(
	String.raw`\{(#if\s+(!?)\s*(?:PREVIEW|preview)\s*\}|#if\b|:else if\b|:else\s*\}|\/if\s*\})`,
	'g'
);
const QUOTES = `'"\``;
const CLOSE = { '(': ')', '[': ']', '{': '}' };

/** Replaces text with spaces, keeping newlines, so line numbers stay right. */
const blank = (text) => text.replace(/[^\n]/g, ' ');

/** The end of the string or bracketed expression starting at `i`, or `i` when there is none. */
function armEnd(source, i) {
	const open = source[i];
	if (QUOTES.includes(open)) {
		for (let j = i + 1; j < source.length; j++) {
			if (source[j] === '\\') j++;
			else if (open === '`' && source.startsWith('${', j)) j = armEnd(source, j + 1) - 1;
			else if (source[j] === open) return j + 1;
		}
		return i;
	}
	if (!CLOSE[open]) return i;
	const closing = [];
	for (let j = i; j < source.length; j++) {
		const c = source[j];
		if (QUOTES.includes(c)) j = armEnd(source, j) - 1;
		else if (CLOSE[c]) closing.push(CLOSE[c]);
		else if (c === closing.at(-1)) {
			closing.pop();
			if (!closing.length) return j + 1;
		}
	}
	return i;
}

/** Blanks comments, style blocks, and the preview arm of every `preview ? …` and `if (preview) …`. */
export function stripNonPaid(source) {
	let out = source.replace(
		/<!--[\s\S]*?-->|<style[\s\S]*?<\/style>|\/\*[\s\S]*?\*\/|(?<![:'"`])\/\/[^\n]*/g,
		blank
	);
	for (const match of out.matchAll(ARM)) {
		const start = match.index + match[0].length;
		const end = armEnd(out, start);
		out = out.slice(0, start) + blank(out.slice(start, end)) + out.slice(end);
	}
	return out;
}

/** The source with everything paid mode does not render blanked: comments, styles and every preview branch. */
export function paidText(source) {
	/** One entry per open `{#if}`: whether the branch we are in renders in paid mode. */
	const stack = [];
	let paid = true;
	return stripNonPaid(source)
		.split('\n')
		.map((line) => {
			let out = '';
			let last = 0;
			for (const tag of line.matchAll(BLOCK)) {
				const [whole, body, not] = tag;
				const before = line.slice(last, tag.index);
				out += (paid ? before : blank(before)) + blank(whole);
				last = tag.index + whole.length;
				if (body.startsWith('#if') && /preview/i.test(body)) stack.push({ preview: !not, paid: !!not });
				else if (body.startsWith('#if')) stack.push({ paid: true });
				else if (body.startsWith(':else') && stack.length) {
					const top = stack.at(-1);
					if ('preview' in top) top.paid = !top.paid;
				} else if (body.startsWith('/if')) stack.pop();
				paid = stack.every((open) => open.paid);
			}
			const rest = line.slice(last);
			return out + (paid ? rest : blank(rest));
		})
		.join('\n');
}

/** Every line where paid mode would render a preview-only phrase: `{ line, text }`, one per line. */
export function findings(source) {
	const original = source.split('\n');
	const text = paidText(source);
	const lines = new Set();
	for (const match of text.matchAll(PREVIEW_ONLY)) lines.add(text.slice(0, match.index).split('\n').length);
	return [...lines].map((line) => ({ line, text: original[line - 1].trim() }));
}

/** The files checked by default, sorted. */
function sources() {
	return ROOTS.flatMap((root) =>
		readdirSync(root, { recursive: true, withFileTypes: true })
			.filter((entry) => entry.isFile())
			.map((entry) => join(entry.parentPath, entry.name))
			.filter((file) => EXTENSIONS.test(file) && !file.startsWith(SKIP))
	).sort();
}

if (import.meta.url === `file://${argv[1]}`) {
	let failed = false;
	for (const file of argv.length > 2 ? argv.slice(2) : sources()) {
		for (const { line, text } of findings(readFileSync(file, 'utf8'))) {
			console.error(`${file}:${line}: preview-only wording outside a preview branch: ${text}`);
			failed = true;
		}
	}
	if (failed) exit(1);
}
