<script lang="ts">
	import Anchor from '$lib/illustrations/Anchor.svelte';
	import { docsNav } from '$lib/site';
	import { STATUS_URL } from '$lib/trust';
</script>

<footer class="site-footer">
	<div class="page grid">
		<div class="about">
			<Anchor class="anchor" />
			<p>
				A private Cargo registry for Rust teams on GitHub. Your crates stay in your organisation; we run the
				registry protocol in front of them.
			</p>
		</div>
		<nav aria-label="Documentation links">
			<h2>Docs</h2>
			<ul>
				{#each docsNav as link (link.href)}
					<li><a href={link.href}>{link.label}</a></li>
				{/each}
			</ul>
		</nav>
		<nav aria-label="Product">
			<h2>Product</h2>
			<ul>
				<li><a href="/pricing">Pricing</a></li>
				<li><a href="/account">Account</a></li>
				<li><a href={STATUS_URL}>Status</a></li>
			</ul>
		</nav>
		<nav aria-label="Trust">
			<h2>Trust</h2>
			<ul>
				<li><a href="/trust">Trust centre</a></li>
				<li><a href="/security">Report a vulnerability</a></li>
				<li><a href="/legal/terms">Preview terms</a></li>
				<li><a href="/legal/privacy">Privacy (draft)</a></li>
				<li><a href="/legal/dpa">DPA (draft)</a></li>
			</ul>
		</nav>
	</div>
</footer>

<style>
	.site-footer {
		margin-top: 5rem;
		border-top: 2px solid var(--line);
		background: var(--deck);
		padding-block: 2.5rem 3rem;
		font-size: var(--text-sm);
	}
	.grid {
		display: grid;
		gap: 2rem;
	}
	@media (min-width: 36rem) {
		.grid {
			grid-template-columns: repeat(3, 1fr);
		}
		.about {
			grid-column: 1 / -1;
		}
	}
	@media (min-width: 60rem) {
		.grid {
			grid-template-columns: 2fr 1fr 1fr 1fr;
		}
		.about {
			grid-column: auto;
		}
	}
	.about {
		display: flex;
		gap: 1rem;
		align-items: flex-start;
		color: var(--ink-soft);
		max-width: 26rem;
	}
	.about :global(.anchor) {
		flex: none;
		width: 2.25rem;
		color: var(--ink);
	}
	h2 {
		font-size: var(--text-lg);
		margin-bottom: 0.4rem;
	}
	li a {
		display: inline-block;
		padding-block: 0.3rem;
		color: var(--ink);
	}
</style>
