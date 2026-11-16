// The paid-copy check: which copy counts as preview-only, and which branches paid mode renders.
// Run: node --test scripts/check-paid-copy.test.js (`pnpm lint` runs it).

import assert from 'node:assert/strict';
import { test } from 'node:test';
import { findings } from './check-paid-copy.js';

/** Each case: source, and the lines paid mode would render a preview-only phrase on. */
const cases = [
	['plain copy', '<p>Billing is off.</p>', [1]],
	[
		'every phrase',
		'the preview\nin preview\npreview terms\ndraft\nnot yet in force\ngeneral availability\nprovided as is',
		[1, 2, 3, 4, 5, 6, 7]
	],
	['the word preview alone is fine', '<p>Preview your crate.</p>', []],
	['a phrase wrapped across lines', '<li>best-effort during the\n\tpreview.</li>', [1]],
	['{#if PREVIEW} hides its branch', '{#if PREVIEW}\n<p>the preview</p>\n{/if}', []],
	['{:else} of {#if PREVIEW} is paid', '{#if PREVIEW}\n<p>ok</p>\n{:else}\n<p>the preview</p>\n{/if}', [4]],
	[
		'{#if !preview} hides its {:else}',
		'{#if !preview}\n<p>paid</p>\n{:else}\n<p>billing is off</p>\n{/if}',
		[]
	],
	['{#if !preview} renders its first branch', '{#if !preview}\n<p>billing is off</p>\n{/if}', [2]],
	['an inline branch', '<p>basis{#if PREVIEW} during the preview{/if}.</p>', []],
	[
		'an unrelated {#if} inside a preview branch',
		'{#if PREVIEW}\n{#if x}\n<p>the preview</p>\n{/if}\n{/if}',
		[]
	],
	[
		'a preview branch inside an unrelated {#if}',
		'{#if x}\n{#if PREVIEW}\n<p>the preview</p>\n{/if}\n<p>draft</p>\n{/if}',
		[5]
	],
	['a ternary string arm', "const a = PREVIEW ? 'the preview' : 'paid';", []],
	['a ternary paid arm', "const a = PREVIEW ? 'ok' : 'the preview';", [1]],
	['a ternary array arm', "const a = PREVIEW\n\t? ['the preview', 'draft']\n\t: ['paid'];", []],
	['a negated ternary is not a preview arm', "const a = !preview ? 'billing is off' : 'paid';", [1]],
	['if (preview) return', "if (preview)\n\treturn { body: 'billing is off' };\nreturn { body: 'paid' };", []],
	['a longer name is not the switch', "const s = { '--size-preview': 'draft' };", [1]],
	['comments are skipped', '<!-- the preview -->\n// draft\n/* billing is off */', []],
	['styles are skipped', '<style>\n.draft { color: red; }\n</style>', []]
];

for (const [name, source, lines] of cases)
	test(name, () => {
		assert.deepEqual(
			findings(source).map((finding) => finding.line),
			lines
		);
	});
