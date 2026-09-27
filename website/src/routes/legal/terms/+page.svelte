<script lang="ts">
	import DraftBanner from '$lib/components/DraftBanner.svelte';
	import Seo from '$lib/components/Seo.svelte';
</script>

<Seo
	title="Terms of service (draft)"
	description="Draft terms of service for PrivateCrates. A placeholder, not yet legal terms."
	path="/legal/terms"
/>

<div class="page">
	<article class="prose legal">
		<h1>Terms of service</h1>
		<DraftBanner />

		<h2>1. Who we are</h2>
		<p>
			PrivateCrates is provided by <mark>TODO: legal entity name</mark>, registered at
			<mark>TODO: registered address</mark>. Contact: <mark>TODO: contact email</mark>.
		</p>

		<h2>2. The service</h2>
		<p>
			PrivateCrates operates a Cargo registry for a GitHub organisation. Crates, the registry index and
			settings are stored in a repository the customer owns; the service keeps no durable copy of them.
		</p>

		<h2>3. Accounts and access</h2>
		<p>
			Access is granted and removed through GitHub. The customer is responsible for who has access to its
			GitHub organisation and repositories. <mark>TODO: acceptable use.</mark>
		</p>

		<h2>4. Subscription, trial and billing</h2>
		<ul>
			<li>$100 per GitHub organisation per month, billed through Stripe.</li>
			<li>A 14-day free trial; the first charge is taken when it ends unless cancelled first.</li>
			<li>
				On cancellation or non-payment, publishing stops at once and reads continue for 14 days after the paid
				period ends.
			</li>
			<li><mark>TODO: taxes, refunds, price changes and notice periods.</mark></li>
		</ul>

		<h2>5. Customer data</h2>
		<p>
			The customer’s crates and index remain in the customer’s own GitHub repository and belong to the
			customer. <mark>TODO: licence granted to operate the service.</mark>
		</p>

		<h2>6. Availability and support</h2>
		<p><mark>TODO: service levels, maintenance and support channels.</mark></p>

		<h2>7. Liability</h2>
		<p><mark>TODO: warranties, limitation of liability and indemnities.</mark></p>

		<h2>8. Ending the agreement</h2>
		<p><mark>TODO: termination by either party and what happens to access.</mark></p>

		<h2>9. Governing law</h2>
		<p><mark>TODO: governing law and jurisdiction.</mark></p>
	</article>
</div>

<style>
	.legal {
		padding-top: 2.5rem;
	}
	h1 {
		font-size: var(--text-3xl);
	}
	mark {
		background: var(--warn-bg);
		color: var(--ink);
		padding: 0 0.2em;
		border-radius: 3px;
		font-weight: 700;
	}
</style>
