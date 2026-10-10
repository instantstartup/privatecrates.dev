<script lang="ts">
	import Crate from '$lib/illustrations/Crate.svelte';
</script>

<a href="/" class="logo">
	<Crate class="mark" />
	<span class="word">PrivateCrates</span>
</a>

<style>
	.logo {
		display: inline-flex;
		align-items: center;
		gap: var(--space-3);
		min-height: var(--size-control);
		color: var(--ink);
		text-decoration: none;
	}
	.logo :global(.mark) {
		width: var(--size-marker);
		height: auto;
	}
	.logo:hover .word {
		text-decoration: underline;
		text-decoration-color: var(--action);
		text-decoration-thickness: var(--border-strong);
		text-underline-offset: var(--underline-offset);
	}
	.logo:hover :global(.mark) {
		transform: rotate(-6deg);
	}
	.logo :global(.mark) {
		transition: transform 150ms ease;
	}
	.word {
		font-family: var(--font-display);
		font-weight: 800;
		font-size: var(--text-title);
		line-height: 1;
		letter-spacing: var(--tracking-heading);
	}
</style>
