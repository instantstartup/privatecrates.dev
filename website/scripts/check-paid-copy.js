// Fails when the website says something that is true only during the preview ("the preview", "preview terms",
// "billing is off", "draft", "not yet in force", "general availability", "as is") outside a branch on the preview
// switch. PREVIEW is a constant, so the build only ever renders one mode; this reads the source instead, so paid-mode
// copy cannot keep a preview statement nobody sees until billing is turned on.
//
// The switch is `PREVIEW` ($lib/site) on static pages and `preview` (the server's PREVIEW, from `session.preview`) on
// the account page. A statement is in a preview branch when it sits in `{#if preview}` (before its `{:else if}` or
// `{:else}`), after the `{:else}` of `{#if !preview}`, in the string or bracketed expression right after `preview ?`,
// or in the one after `if (preview)` or `if (preview) return`. Every Svelte block is followed, so the `{:else}` of an
// `{#each}` belongs to the `{#each}`. Comments and style blocks are skipped.
//
// It also fails on two words a branch edge runs together, as Svelte trims the whitespace at a branch's edges.
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
/** A Svelte block tag; `not` and `on` are set when it is an `{#if}` or `{:else if}` on the switch alone. */
const BLOCK = new RegExp(
	String.raw`\{(?:(?<open>#(?<kind>\w+))|(?<branch>:else\s+if|:else|:then|:catch)|\/(?<close>\w+)\s*\})(?:\s*(?<not>!?)\s*(?<on>PREVIEW|preview)\s*\})?`,
	'g'
);
/**
 * Words a branch edge would run together, as Svelte trims the whitespace at the start and end of every branch: text
 * right before an `{#if}` or `{:else}` whose branch starts with whitespace, or a branch ending in whitespace right
 * before `{/if}` and a word. The separating space goes in an expression instead: `basis{#if PREVIEW}{' '}during`.
 */
const TRIMMED = /[^\s>]\{(?:#if|:else)\b[^}\n]*\}\s+[^\s{]|\s\{\/if\s*\}\w/g;
/** Which modes can render: paid, preview, both. */
const PAID = 1;
const PREVIEW_MODE = 2;
const BOTH = PAID | PREVIEW_MODE;
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

/** The modes the branch an `{#if}` or `{:else if}` opens renders in, and the modes left for the rest of its chain. */
function narrow(reach, { not, on }) {
	if (!on) return [reach, reach];
	const branch = not ? PAID : PREVIEW_MODE;
	return [reach & branch, reach & ~branch];
}

/**
 * The source with everything paid mode does not render blanked: comments, styles and every preview branch. Throws on
 * a block closed by the wrong tag, so a block it cannot follow fails the check rather than passing it.
 */
export function paidText(source) {
	/** One entry per open block: its kind, the modes its current branch renders in, and those left for its chain. */
	const stack = [];
	let paid = true;
	return stripNonPaid(source)
		.split('\n')
		.map((line, number) => {
			let out = '';
			let last = 0;
			for (const tag of line.matchAll(BLOCK)) {
				const { open, kind, branch, close } = tag.groups;
				const before = line.slice(last, tag.index);
				out += (paid ? before : blank(before)) + blank(tag[0]);
				last = tag.index + tag[0].length;
				const reach = stack.length ? stack.at(-1).branch : BOTH;
				if (open && kind === 'if') {
					const [now, rest] = narrow(reach, tag.groups);
					stack.push({ kind, branch: now, rest });
				} else if (open) stack.push({ kind, branch: reach, rest: reach });
				else if (branch && stack.length) {
					const top = stack.at(-1);
					if (top.kind === 'if')
						[top.branch, top.rest] = narrow(top.rest, branch === ':else' ? {} : tag.groups);
					else top.branch = top.rest;
				} else if (close) {
					if (stack.at(-1)?.kind !== close)
						throw new Error(`line ${number + 1}: {/${close}} closes ${stack.at(-1)?.kind ?? 'nothing'}`);
					stack.pop();
				}
				paid = !stack.length || (stack.at(-1).branch & PAID) !== 0;
			}
			const rest = line.slice(last);
			return out + (paid ? rest : blank(rest));
		})
		.join('\n');
}

/** Line numbers, sorted and once each, of the matches of `pattern` in `text`. */
function lines(text, pattern) {
	const at = new Set();
	for (const match of text.matchAll(pattern)) at.add(text.slice(0, match.index + 1).split('\n').length);
	return [...at].sort((x, y) => x - y);
}

/**
 * Every line where paid mode would render a preview-only phrase, and every line where a branch edge would run two
 * words together: `{ line, text, problem }`, sorted by line.
 */
export function findings(source) {
	const original = source.split('\n');
	const found = (problem) => (line) => ({ line, text: original[line - 1].trim(), problem });
	return [
		...lines(paidText(source), PREVIEW_ONLY).map(found('preview-only wording outside a preview branch')),
		...lines(stripNonPaid(source), TRIMMED).map(found('Svelte trims the space at this branch edge'))
	].sort((x, y) => x.line - y.line);
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
		for (const { line, text, problem } of findings(readFileSync(file, 'utf8'))) {
			console.error(`${file}:${line}: ${problem}: ${text}`);
			failed = true;
		}
	}
	if (failed) exit(1);
}
