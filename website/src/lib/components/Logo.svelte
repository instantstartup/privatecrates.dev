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
		gap: 0.55rem;
		min-height: 2.75rem;
		color: var(--ink);
		text-decoration: none;
	}
	.logo :global(.mark) {
		width: 2.1rem;
		height: auto;
	}
	.logo:hover .word {
		text-decoration: underline;
		text-decoration-color: var(--signal);
		text-decoration-thickness: 3px;
		text-underline-offset: 0.2em;
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
		font-size: var(--text-xl);
		line-height: 1;
		letter-spacing: 0.01em;
	}
</style>
