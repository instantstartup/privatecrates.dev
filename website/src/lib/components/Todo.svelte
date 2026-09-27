<script lang="ts">
	import type { Snippet } from 'svelte';

	// A fact the owner has not decided or confirmed yet. Marked, never invented.
	let { children }: { children: Snippet } = $props();
</script>

<mark class="todo">TODO: {@render children()}</mark>

<style>
	.todo {
		background: var(--warn-bg);
		color: var(--ink);
		padding: 0 0.25em;
		border-radius: 3px;
		border-bottom: 2px dashed var(--signal);
		font-weight: 700;
		box-decoration-break: clone;
		-webkit-box-decoration-break: clone;
	}
</style>
