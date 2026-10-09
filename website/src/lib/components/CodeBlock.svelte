<script lang="ts">
	import { copyText } from './clipboard';

	interface Props {
		code: string;
		/** File name or language shown above the code, e.g. ".cargo/config.toml" or "shell". */
		caption?: string;
		/** Languages whose comments start with "#" get them dimmed. */
		hashComments?: boolean;
	}

	let { code, caption, hashComments = true }: Props = $props();

	const lines = $derived(
		code
			.replace(/^\n+|\s+$/g, '')
			.split('\n')
			.map((line, n) => {
				// Each line after the first starts with its newline, so the markup needs no separators.
				const nl = n > 0 ? '\n' : '';
				const i = hashComments ? line.search(/(^|\s)#(?![{[])/) : -1;
				if (i < 0) return { text: nl + line, comment: '', trailing: false };
				const at = line[i] === '#' ? i : i + 1;
				return {
					text: nl + line.slice(0, at),
					comment: line.slice(at),
					trailing: at > 0 && line.slice(0, at).trim() !== ''
				};
			})
	);
	const plain = $derived(lines.map((l) => l.text + l.comment).join(''));

	let status = $state('');
	let timer: ReturnType<typeof setTimeout> | undefined;

	async function copy() {
		status = (await copyText(plain + '\n')) ? 'Copied' : 'Copy failed: select the text instead';
		clearTimeout(timer);
		timer = setTimeout(() => (status = ''), 2500);
	}
</script>

<figure class="code">
	<figcaption>
		<span class="caption">{caption ?? ''}</span>
		<button type="button" class="copy" onclick={copy}>
			<svg viewBox="0 0 20 20" aria-hidden="true" focusable="false">
				<rect x="6" y="6" width="11" height="11" rx="2" />
				<path d="M13 3.5H5A1.5 1.5 0 0 0 3.5 5v8" />
			</svg>
			{status === 'Copied' ? 'Copied' : 'Copy'}<span class="visually-hidden"> {caption ?? 'code'}</span>
		</button>
		<span class="visually-hidden" role="status">{status}</span>
	</figcaption>
	<pre><code
			>{#each lines as line, i (i)}{line.text}{#if line.comment}<span
						class={['comment', line.trailing && 'trailing']}>{line.comment}</span
					>{/if}{/each}</code
		></pre>
</figure>

<style>
	.code {
		margin: 0;
		border: var(--border-outline) solid var(--line);
		border-radius: var(--radius-card);
		background: var(--code-bg);
		color: var(--code-ink);
		overflow: hidden;
		min-width: 0;
	}
	figcaption {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: var(--space-5);
		padding: var(--space-2) var(--space-2) var(--space-2) var(--space-5);
		border-bottom: var(--border-hairline) solid color-mix(in srgb, var(--code-ink) 22%, transparent);
		font-size: var(--text-meta);
		color: var(--code-comment);
	}
	.caption {
		font-family: var(--font-mono);
		overflow-wrap: anywhere;
	}
	.copy {
		display: inline-flex;
		align-items: center;
		gap: var(--space-2);
		min-height: var(--size-control-compact);
		min-width: var(--size-control-label);
		justify-content: center;
		padding: var(--space-2) var(--space-4);
		border-radius: var(--radius-control);
		color: var(--code-ink);
		font-weight: 700;
		font-size: var(--text-meta);
		cursor: pointer;
	}
	.copy:hover {
		background: color-mix(in srgb, var(--code-ink) 14%, transparent);
	}
	.copy:focus-visible {
		outline-color: var(--code-focus);
	}
	.copy svg {
		width: var(--size-icon-small);
		height: var(--size-icon-small);
		fill: none;
		stroke: currentColor;
		stroke-width: 1.75;
		stroke-linejoin: round;
	}
	pre {
		margin: 0;
		padding: var(--space-code-block-top) var(--space-5) var(--space-5);
		font-size: var(--text-meta);
		line-height: 1.6;
		tab-size: 2;
		/* Long lines fold inside the column rather than hiding behind a sideways scroll. */
		white-space: pre-wrap;
		overflow-wrap: anywhere;
		overflow-x: auto;
	}
	.comment {
		color: var(--code-comment);
	}
	/* A short comment after code moves to the next line whole, rather than breaking mid-phrase. */
	.trailing {
		white-space: nowrap;
	}
</style>
