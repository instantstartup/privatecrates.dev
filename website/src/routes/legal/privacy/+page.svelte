<script lang="ts">
	import DraftBanner from '$lib/components/DraftBanner.svelte';
	import Seo from '$lib/components/Seo.svelte';
</script>

<Seo
	title="Privacy notice (draft)"
	description="Draft privacy notice for PrivateCrates. A placeholder, not yet legal terms."
	path="/legal/privacy"
/>

<div class="page">
	<article class="prose legal">
		<h1>Privacy notice</h1>
		<DraftBanner />

		<h2>1. Controller</h2>
		<p>
			<mark>TODO: legal entity name and registered address</mark>. Contact for privacy questions:
			<mark>TODO: contact email</mark>.
		</p>

		<h2>2. What we process</h2>
		<ul>
			<li>
				<strong>GitHub identity:</strong> your GitHub login, name and avatar, and the organisations you belong to
				with your role, read from GitHub when you sign in.
			</li>
			<li>
				<strong>Tokens:</strong> GitHub tokens sent with registry requests are used only to ask GitHub what you
				can access. They are never stored or logged; caches are keyed by a hash of the token.
			</li>
			<li>
				<strong>Session:</strong> after signing in on this website, your GitHub token is kept only in an encrypted
				cookie in your browser, valid for 8 hours. Nothing is stored on our side.
			</li>
			<li>
				<strong>Billing:</strong> we count your organisation’s active members through GitHub, to tell whether it
				is on the free plan; we keep the count for up to 24 hours and not the members’ names. Stripe processes payment
				details; we receive the subscription status and the organisation it belongs to.
			</li>
			<li>
				<strong>Crates:</strong> crate files pass through the service while being published and are stored in your
				organisation’s own repository.
			</li>
		</ul>

		<h2>3. Cookies</h2>
		<p>
			One strictly necessary cookie, <code>pc_session</code>, holds your encrypted sign-in. This website uses
			no analytics or advertising cookies. Your theme choice is kept in your browser’s local storage.
		</p>

		<h2>4. Processors</h2>
		<ul>
			<li>GitHub, which stores your crates and provides identity.</li>
			<li>Stripe, for payments.</li>
			<li><mark>TODO: hosting provider and key management service.</mark></li>
		</ul>

		<h2>5. Retention</h2>
		<p>
			Service logs are deleted after 30 days. Nothing else about you is stored by us: caches live in memory
			and expire within minutes. <mark>TODO: retention period for billing records held by Stripe.</mark>
		</p>

		<h2>6. Your rights</h2>
		<p><mark>TODO: access, correction, deletion and complaint rights, and how to exercise them.</mark></p>

		<h2>7. International transfers</h2>
		<p><mark>TODO: transfer mechanisms.</mark></p>
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
