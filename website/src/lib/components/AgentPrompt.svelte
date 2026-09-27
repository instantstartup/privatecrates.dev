<script lang="ts">
	import type { Snippet } from 'svelte';
	import { copyText } from './clipboard';

	interface Props {
		/** Unique on the page: ids for the heading and the prompt. */
		id: string;
		title: string;
		/** The full prompt, copied as it is. */
		prompt: string;
		/** What the prompt does and what it will ask of the reader. */
		children: Snippet;
		/** A primary button, when copying is the main action here (not beside a checklist step's own). */
		primary?: boolean;
	}

	let { id, title, prompt, children, primary = false }: Props = $props();

	let open = $state(false);
	let status = $state('');
	let failed = $state(false);
	let timer: ReturnType<typeof setTimeout> | undefined;

	async function copy() {
		failed = !(await copyText(prompt + '\n'));
		status = failed
			? 'The prompt could not be copied. Open the whole prompt and select it instead.'
			: 'Copied. Paste it into your coding agent.';
		if (failed) open = true;
		clearTimeout(timer);
		timer = setTimeout(() => (status = ''), failed ? 8000 : 4000);
	}
</script>

<section class="agent" aria-labelledby="{id}-title">
	<div class="intro">
		<h4 id="{id}-title">{title}</h4>
		{@render children()}
	</div>
	<div class={['prompt', open && 'open']} id="{id}-prompt">
		<pre>{prompt}</pre>
	</div>
	<div class="row">
		<button class={['btn', primary ? 'btn-primary' : 'btn-quiet']} type="button" onclick={copy}>
			<svg viewBox="0 0 20 20" aria-hidden="true" focusable="false">
				<rect x="6" y="6" width="11" height="11" rx="2" />
				<path d="M13 3.5H5A1.5 1.5 0 0 0 3.5 5v8" />
			</svg>
			Copy prompt
		</button>
		<button
			class="btn btn-quiet"
			type="button"
			aria-expanded={open}
			aria-controls="{id}-prompt"
			onclick={() => (open = !open)}
		>
			{open ? 'Show less' : 'Show the whole prompt'}
		</button>
		<p class={['status', failed && 'failed']} role="status">{status}</p>
	</div>
</section>

<style>
	.agent {
		display: grid;
		gap: 1rem;
		padding: 1.25rem;
		border: 2px dashed var(--line);
		border-radius: 10px;
		background: var(--deck);
		min-width: 0;
	}
	.intro {
		display: grid;
		gap: 0.35rem;
	}
	h4 {
		font-family: var(--font-display);
		font-weight: 800;
		font-size: var(--text-xl);
		line-height: 1.1;
	}
	.intro :global(p) {
		color: var(--ink-soft);
		max-width: 40rem;
	}
	.prompt {
		position: relative;
		border: 2px solid var(--line);
		border-radius: 8px;
		background: var(--code-bg);
		color: var(--code-ink);
		/* Collapsed, the prompt is a preview: its first lines, fading out. */
		max-height: 9.5rem;
		overflow: hidden;
	}
	.prompt:not(.open)::after {
		content: '';
		position: absolute;
		inset: auto 0 0;
		height: 3.5rem;
		background: linear-gradient(transparent, var(--code-bg));
		pointer-events: none;
	}
	.prompt.open {
		max-height: none;
	}
	pre {
		margin: 0;
		padding: 0.9rem 1rem 1rem;
		font-family: var(--font-mono);
		font-size: var(--text-sm);
		line-height: 1.6;
		white-space: pre-wrap;
		overflow-wrap: anywhere;
	}
	.row {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 0.75rem;
	}
	.btn svg {
		width: 1.1rem;
		height: 1.1rem;
		fill: none;
		stroke: currentColor;
		stroke-width: 1.75;
		stroke-linejoin: round;
	}
	.status {
		font-size: var(--text-sm);
		font-weight: 700;
		color: var(--ok);
	}
	.status.failed {
		color: var(--danger);
	}
</style>
