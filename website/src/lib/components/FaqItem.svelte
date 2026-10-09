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
		border-bottom: 1px solid var(--divider);
	}
	.faq-item:first-child {
		border-top: 1px solid var(--divider);
	}
	summary {
		display: flex;
		justify-content: space-between;
		align-items: center;
		gap: 1rem;
		min-height: 3.25rem;
		padding-block: 0.75rem;
		font-weight: 700;
		font-size: var(--text-base);
		cursor: pointer;
		list-style: none;
	}
	summary::-webkit-details-marker {
		display: none;
	}
	summary:hover span {
		text-decoration: underline;
		text-decoration-color: var(--action);
		text-decoration-thickness: 2px;
		text-underline-offset: 0.25em;
	}
	svg {
		flex: none;
		width: 1.25rem;
		height: 1.25rem;
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
		padding-bottom: 1.1rem;
		color: var(--ink-muted);
		max-width: var(--container-prose);
	}
	.answer :global(p + p) {
		margin-top: 0.6rem;
	}
</style>
