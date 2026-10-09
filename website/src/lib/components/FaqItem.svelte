<script lang="ts">
	import type { Snippet } from 'svelte';

	interface Props {
		question: string;
		children: Snippet;
	}

	let { question, children }: Props = $props();
</script>

<details class="faq-item">
	<summary>
		<span>{question}</span>
		<svg viewBox="0 0 20 20" aria-hidden="true" focusable="false"><path d="M5 8l5 5 5-5" /></svg>
	</summary>
	<div class="answer">
		{@render children()}
	</div>
</details>

<style>
	.faq-item {
		border-bottom: var(--border-hairline) solid var(--divider);
	}
	.faq-item:first-child {
		border-top: var(--border-hairline) solid var(--divider);
	}
	summary {
		display: flex;
		justify-content: space-between;
		align-items: center;
		gap: var(--space-5);
		min-height: var(--size-disclosure);
		padding-block: var(--space-4);
		font-weight: 700;
		font-size: var(--text-body);
		cursor: pointer;
		list-style: none;
	}
	summary::-webkit-details-marker {
		display: none;
	}
	summary:hover span {
		text-decoration: underline;
		text-decoration-color: var(--action);
		text-decoration-thickness: var(--border-outline);
		text-underline-offset: var(--underline-offset);
	}
	svg {
		flex: none;
		width: var(--size-icon);
		height: var(--size-icon);
		fill: none;
		stroke: currentColor;
		stroke-width: 2.25;
		stroke-linecap: round;
		stroke-linejoin: round;
		transition: transform 150ms ease;
	}
	details[open] svg {
		transform: rotate(180deg);
	}
	.answer {
		padding-bottom: var(--space-5);
		color: var(--ink-muted);
		max-width: var(--container-prose);
	}
	.answer :global(p + p) {
		margin-top: var(--space-3);
	}
</style>
