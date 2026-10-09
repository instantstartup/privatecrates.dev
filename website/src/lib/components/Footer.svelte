<script lang="ts">
	import Anchor from '$lib/illustrations/Anchor.svelte';
	import { COMPANY, docsNav } from '$lib/site';
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
				<li><a href="/legal/privacy">Privacy</a></li>
				<li><a href="/legal/dpa">DPA (draft)</a></li>
				<li><a href="/third-party-licenses.txt">Third-party licences</a></li>
			</ul>
		</nav>
	</div>
	<!-- Required of a UK company on its website (Companies (Trading Disclosures) Regulations 2008). -->
	<p class="page company">
		PrivateCrates is operated by {COMPANY.name}, a private company limited by shares registered in
		{COMPANY.registeredIn}, company number {COMPANY.number}. Registered office: {COMPANY.registeredOffice}.
	</p>
</footer>

<style lang="postcss">
	@reference '../../app.css';

	.company {
		margin-top: var(--space-9);
		color: var(--ink-muted);
	}
	.site-footer {
		margin-top: var(--space-section);
		border-top: var(--border-outline) solid var(--line);
		background: var(--surface-raised);
		padding-block: var(--space-10) var(--space-11);
		font-size: var(--text-meta);
	}
	.grid {
		display: grid;
		gap: var(--space-9);
	}
	@media (width >= theme(--breakpoint-2)) {
		.grid {
			grid-template-columns: repeat(3, 1fr);
		}
		.about {
			grid-column: 1 / -1;
		}
	}
	@media (width >= theme(--breakpoint-8)) {
		.grid {
			grid-template-columns: 2fr 1fr 1fr 1fr;
		}
		.about {
			grid-column: auto;
		}
	}
	.about {
		display: flex;
		gap: var(--space-5);
		align-items: flex-start;
		color: var(--ink-muted);
		max-width: var(--container-blurb);
	}
	.about :global(.anchor) {
		flex: none;
		width: var(--size-control-compact);
		color: var(--ink);
	}
	h2 {
		font-size: var(--text-lead);
		margin-bottom: var(--space-after-title);
	}
	li a {
		display: inline-block;
		padding-block: var(--space-link-list);
		color: var(--ink);
	}
</style>
