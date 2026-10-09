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
		border-left: 5px solid var(--link);
		background: var(--surface-raised);
		padding: 0.85rem 1rem;
		border-radius: 0 8px 8px 0;
	}
	.callout > :global(* + *) {
		margin-top: 0.5rem;
	}
	.warn {
		border-color: var(--primary);
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
