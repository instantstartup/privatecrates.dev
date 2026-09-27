<script lang="ts">
	import FaqItem from '$lib/components/FaqItem.svelte';
	import Seo from '$lib/components/Seo.svelte';
	import CargoShip from '$lib/illustrations/CargoShip.svelte';
	import { PRICE_USD, TRIAL_DAYS } from '$lib/site';

	const included = [
		[
			'Unlimited developers',
			'Everyone in the organisation, and outside collaborators on the repositories they can read.'
		],
		[
			'SSO and SCIM through GitHub',
			'SAML SSO, provisioning and offboarding apply as they already do. No SSO tax.'
		],
		['Trusted publishing from GitHub Actions', 'OIDC in CI: no secrets to store for reading or publishing.'],
		[
			'Provenance and immutable releases',
			'Every CI-published version carries GitHub-signed proof of the workflow that built it.'
		],
		['The open-source verifier', 'Run it hourly in your own CI to check everything we write.'],
		['Search', 'cargo search returns your private crates first, then crates.io results.'],
		['Your data stays yours', 'Index and crates live in a repository your organisation owns.']
	];
</script>

<Seo
	title="Pricing"
	description="PrivateCrates costs $100 per GitHub organisation per month, with unlimited users, SSO included and a 14-day free trial."
	path="/pricing"
/>

<div class="page">
	<header class="intro">
		<h1>One organisation, one price</h1>
		<p class="lede">
			No per-seat maths and no enterprise tier to unlock SSO. Every feature is in the one plan.
		</p>
	</header>

	<section class="plan panel" aria-labelledby="plan-name">
		<div class="plan-head">
			<h2 id="plan-name">Organisation</h2>
			<p class="price">
				<span class="figure">${PRICE_USD}</span>
				<span class="per"
					>per GitHub organisation per month, billed monthly in US dollars. Taxes may apply.</span
				>
			</p>
			<a class="btn btn-primary" href="/account">Start {TRIAL_DAYS}-day free trial</a>
			<p class="small">
				Card required at checkout. You are not charged until the trial ends, and you can cancel before then.
			</p>
			<CargoShip class="plan-ship" />
		</div>
		<div class="plan-body">
			<h3>Everything is included</h3>
			<dl>
				{#each included as [term, desc] (term)}
					<div>
						<dt>{term}</dt>
						<dd>{desc}</dd>
					</div>
				{/each}
			</dl>
		</div>
	</section>

	<section class="faq" aria-labelledby="pricing-faq">
		<h2 id="pricing-faq">Billing questions</h2>
		<div class="faq-list">
			<FaqItem question="What counts as an organisation?">
				<p>
					One GitHub organisation that installs both PrivateCrates Apps, and gets one registry at
					<code>your-name.privatecrates.dev</code>. Every repository, member and outside collaborator in that
					organisation is covered. A second organisation is a second subscription.
				</p>
			</FaqItem>
			<FaqItem question="How does the free trial work?">
				<p>
					An organisation admin starts the {TRIAL_DAYS}-day trial from the account page, through Stripe
					Checkout. The first charge of ${PRICE_USD} is taken when the trial ends, unless you cancel first. The
					trial is for an organisation’s first subscription; an organisation that subscribes again starts paying
					straight away.
				</p>
			</FaqItem>
			<FaqItem question="What happens when we cancel?">
				<p>
					Publishing stops straight away. Reading keeps working for 14 days after the paid period ends, so
					builds do not break overnight while you move away. After that, reads stop too.
				</p>
				<p>Nothing is deleted: your index and every crate file are in your own repository, and stay there.</p>
			</FaqItem>
			<FaqItem question="What if a payment fails?">
				<p>
					Your registry keeps working while Stripe retries the payment. An admin can update the card from
					<a href="/account">Account and billing</a>.
				</p>
			</FaqItem>
			<FaqItem question="Who can manage billing?">
				<p>
					Admins of the GitHub organisation. Members see the registry’s status but cannot start, change or
					cancel the subscription.
				</p>
			</FaqItem>
		</div>
	</section>
</div>

<style>
	.intro {
		padding-block: 3.5rem 2.5rem;
	}
	h1 {
		font-size: var(--text-3xl);
		margin-bottom: 1rem;
	}
	.plan {
		display: grid;
		border: 2px solid var(--ink);
		overflow: hidden;
	}
	@media (min-width: 56rem) {
		.plan {
			grid-template-columns: 22rem 1fr;
		}
	}
	.plan-head {
		padding: 2rem 1.5rem;
		background: var(--fog);
		border-bottom: 2px solid var(--ink);
		display: flex;
		flex-direction: column;
		align-items: flex-start;
		gap: 1rem;
	}
	@media (min-width: 56rem) {
		.plan-head {
			border-bottom: 0;
			border-right: 2px solid var(--ink);
		}
	}
	h2 {
		font-size: var(--text-2xl);
	}
	.price {
		display: flex;
		flex-direction: column;
	}
	.figure {
		font-family: var(--font-display);
		font-weight: 800;
		font-size: var(--text-display);
		line-height: 0.9;
	}
	.per,
	.small {
		color: var(--ink-soft);
	}
	.small {
		font-size: var(--text-sm);
	}
	.plan-head :global(.plan-ship) {
		width: 100%;
		height: auto;
		margin-top: auto;
		padding-top: 1rem;
	}
	.plan-body {
		padding: 2rem 1.5rem;
	}
	h3 {
		font-size: var(--text-2xl);
		margin-bottom: 1rem;
	}
	dl {
		display: grid;
		gap: 1.1rem;
	}
	@media (min-width: 40rem) {
		dl {
			grid-template-columns: 1fr 1fr;
			gap: 1.4rem 2rem;
		}
	}
	dt {
		font-weight: 700;
	}
	dd {
		color: var(--ink-soft);
	}
	.faq {
		padding-top: 4.5rem;
	}
	.faq h2 {
		font-size: var(--text-3xl);
		margin-bottom: 1.25rem;
	}
	.faq-list {
		max-width: var(--container-prose);
	}
</style>
