<script lang="ts">
	import type { Snippet } from 'svelte';

	interface Props {
		tone?: 'note' | 'warn' | 'danger' | 'ok';
		title?: string;
		/** "alert" for errors that appear in response to an action. */
		role?: 'alert' | 'status';
		children: Snippet;
	}

	let { tone = 'note', title, role, children }: Props = $props();
</script>

<div class={['callout', tone]} {role}>
	{#if title}<p class="title">{title}</p>{/if}
	{@render children()}
</div>

<style>
	.callout {
		border-left: var(--border-bar) solid var(--info);
		background: var(--surface-raised);
		padding: var(--space-callout-y) var(--space-5);
		border-radius: 0 var(--radius-card) var(--radius-card) 0;
	}
	.callout > :global(* + *) {
		margin-top: var(--space-3);
	}
	.warn {
		border-color: var(--action);
		background: var(--warn-bg);
	}
	.danger {
		border-color: var(--danger);
		background: var(--danger-bg);
	}
	.ok {
		border-color: var(--ok);
		background: var(--ok-bg);
	}
	.title {
		font-weight: 700;
	}
</style>
