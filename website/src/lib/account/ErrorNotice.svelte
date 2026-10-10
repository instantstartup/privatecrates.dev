<script lang="ts">
	import type { Snippet } from 'svelte';
	import type { ApiError } from '$lib/api';
	import Callout from '$lib/components/Callout.svelte';

	interface Props {
		error: ApiError;
		title?: string;
		/** Extra controls, e.g. a retry button. */
		children?: Snippet;
	}

	let { error, title = 'That did not work', children }: Props = $props();
</script>

<Callout tone="danger" role="alert" {title}>
	{#each error.errors as item, i (i)}
		<p>
			{item.detail}
			{#if item.code}
				<span class="code">(<a href="/docs/errors#{item.code}"><code>{item.code}</code></a>)</span>
			{/if}
		</p>
	{/each}
	{#if error.status === 401}
		<p>
			Your sign-in may have expired. <a href="/account" data-sveltekit-reload>Reload the page</a> to sign in again.
		</p>
	{/if}
	{@render children?.()}
</Callout>

<style>
	.code {
		white-space: nowrap;
		font-size: inherit;
	}
</style>
