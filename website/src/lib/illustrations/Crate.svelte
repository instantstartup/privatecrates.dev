<script lang="ts">
	import type { IllustrationProps, Tone } from './types';

	interface Props extends IllustrationProps {
		tone?: Tone;
		/** Stencilled on the front, e.g. a crate name or version. */
		label?: string;
	}

	let { tone = 'action', label, title, class: klass, x, y, width, height }: Props = $props();
</script>

<svg
	viewBox="0 0 64 48"
	class={['crate', `tone-${tone}`, klass]}
	{x}
	{y}
	{width}
	{height}
	role={title ? 'img' : undefined}
	aria-label={title}
	aria-hidden={title ? undefined : 'true'}
	focusable="false"
>
	<rect class="body" x="2" y="2" width="60" height="44" rx="2" />
	<path class="batten" d="M2 9.5h60M2 38.5h60M11 2v44M53 2v44" />
	{#if label}
		<text class="label" x="32" y="29.5" text-anchor="middle">{label}</text>
	{:else}
		<path class="batten" d="M11 9.5 53 38.5" />
	{/if}
</svg>

<style>
	.crate {
		overflow: visible;
	}
	.body {
		stroke: var(--line);
		stroke-width: 2.5;
		stroke-linejoin: round;
	}
	.batten {
		fill: none;
		stroke: var(--line);
		stroke-width: 2;
		stroke-linecap: round;
	}
	.tone-action .body {
		fill: var(--action);
	}
	.tone-accent .body {
		fill: var(--accent);
	}
	.tone-muted .body {
		fill: var(--scene-sea-light);
	}
	.tone-raised .body {
		fill: var(--surface-raised);
	}
	.label {
		font-family: var(--font-display);
		font-weight: 800;
		font-size: 11px;
		letter-spacing: 0.04em;
		fill: var(--ink);
	}
	.tone-action .label {
		fill: var(--action-ink);
	}
	.tone-accent .label {
		fill: var(--accent-ink);
	}
	.tone-muted .label {
		fill: var(--scene-sea-light-ink);
	}
</style>
