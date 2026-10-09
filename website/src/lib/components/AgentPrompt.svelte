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
		gap: var(--space-5);
		padding: var(--space-6);
		border: var(--border-outline) dashed var(--line);
		border-radius: var(--radius-panel);
		background: var(--surface-raised);
		min-width: 0;
	}
	.intro {
		display: grid;
		gap: var(--space-2);
	}
	h4 {
		font-family: var(--font-display);
		font-weight: 800;
		font-size: var(--text-title);
		line-height: 1.1;
	}
	.intro :global(p) {
		color: var(--ink-muted);
		max-width: var(--container-measure);
	}
	.prompt {
		position: relative;
		border: var(--border-outline) solid var(--line);
		border-radius: var(--radius-card);
		background: var(--code-bg);
		color: var(--code-ink);
		/* Collapsed, the prompt is a preview: its first lines, fading out. */
		max-height: var(--size-preview);
		overflow: hidden;
	}
	.prompt:not(.open)::after {
		content: '';
		position: absolute;
		inset: auto 0 0;
		height: var(--size-preview-fade);
		background: linear-gradient(transparent, var(--code-bg));
		pointer-events: none;
	}
	.prompt.open {
		max-height: none;
	}
	pre {
		margin: 0;
		padding: var(--space-code-block-top) var(--space-5) var(--space-5);
		font-family: var(--font-mono);
		font-size: var(--text-meta);
		line-height: 1.6;
		white-space: pre-wrap;
		overflow-wrap: anywhere;
	}
	.row {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: var(--space-4);
	}
	.btn svg {
		width: var(--size-icon-small);
		height: var(--size-icon-small);
		fill: none;
		stroke: currentColor;
		stroke-width: 1.75;
		stroke-linejoin: round;
	}
	.status {
		font-size: var(--text-meta);
		font-weight: 700;
		color: var(--ok);
	}
	.status.failed {
		color: var(--danger);
	}
</style>
