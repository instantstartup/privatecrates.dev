<script lang="ts">
	import FaqItem from '$lib/components/FaqItem.svelte';
	import Seo from '$lib/components/Seo.svelte';
	import CargoShip from '$lib/illustrations/CargoShip.svelte';
	import Crate from '$lib/illustrations/Crate.svelte';
	import {
		FREE_MEMBER_LIMIT,
		PRICE_USD,
		READ_GRACE_DAYS,
		TRIAL_MONTHS,
		TRIAL_REMINDER_DAYS
	} from '$lib/site';

	const included = [
		[
			'Every developer',
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
	description="PrivateCrates is free for GitHub organisations with up to {FREE_MEMBER_LIMIT} members. Larger organisations pay ${PRICE_USD} per month, with unlimited users, SSO included and a 3-month free trial that needs no card."
	path="/pricing"
/>

<div class="page">
	<header class="intro">
		<h1>Free for small teams, one price for the rest</h1>
		<p class="lede">
			No per-seat maths and no enterprise tier to unlock SSO. Both plans have every feature; the only
			difference is how many people are in your GitHub organisation.
		</p>
	</header>

	<section class="plans" aria-label="Plans">
		<article class="plan panel" aria-labelledby="plan-free">
			<div class="plan-head">
				<h2 id="plan-free">Free</h2>
				<p class="price">
					<span class="figure">$0</span>
					<span class="per">for organisations with up to {FREE_MEMBER_LIMIT} members</span>
				</p>
				<a class="btn btn-quiet" href="/account">Set up a free registry</a>
				<p class="small">No card, no time limit. Outside collaborators do not count as members.</p>
			</div>
			<div class="stack" aria-hidden="true">
				{#each ['signal', 'harbour', 'container'] as const as tone (tone)}
					<Crate {tone} class="stack-crate" />
				{/each}
			</div>
		</article>

		<article class="plan panel team" aria-labelledby="plan-team">
			<div class="plan-head">
				<h2 id="plan-team">Team</h2>
				<p class="price">
					<span class="figure">${PRICE_USD}</span>
					<span class="per"
						>per GitHub organisation per month, for more than {FREE_MEMBER_LIMIT} members. Unlimited users.</span
					>
				</p>
				<a class="btn btn-primary" href="/account">Start {TRIAL_MONTHS}-month free trial</a>
				<p class="small">
					{TRIAL_MONTHS} months free, and no card to start. Add one any time before the trial ends.
				</p>
			</div>
			<CargoShip class="plan-ship" />
		</article>
	</section>

	<section class="included panel" aria-labelledby="included">
		<h2 id="included">Everything is in both plans</h2>
		<p class="note">No SSO tax, no feature gates: a team of three gets what a team of three hundred gets.</p>
		<dl>
			{#each included as [term, desc] (term)}
				<div>
					<dt>{term}</dt>
					<dd>{desc}</dd>
				</div>
			{/each}
		</dl>
		<p class="note trust-link">
			Evaluating us for your security team? The <a href="/trust">trust centre</a> lists what we hold, our subprocessors
			and our security controls.
		</p>
	</section>

	<section class="faq" aria-labelledby="pricing-faq">
		<h2 id="pricing-faq">Billing questions</h2>
		<div class="faq-list">
			<FaqItem question="Who counts as a member?">
				<p>
					The active members of your GitHub organisation, as GitHub lists them. Outside collaborators and
					pending invitations do not count, and neither do CI jobs. One organisation gets one registry at
					<code>your-name.privatecrates.dev</code>; a second organisation has its own plan.
				</p>
			</FaqItem>
			<FaqItem question="What happens when we grow past {FREE_MEMBER_LIMIT} members?">
				<p>
					Nothing breaks. The {TRIAL_MONTHS}-month free trial starts by itself, with no card, and the account
					page shows how long is left. Organisations that already have more than {FREE_MEMBER_LIMIT}
					members start the same trial with one click when they set up.
				</p>
				<p>
					Each organisation gets one trial. An organisation that subscribes again later pays from the first
					day.
				</p>
			</FaqItem>
			<FaqItem question="What if the trial ends and we have not added a card?">
				<p>
					Publishing stops when the trial ends. Builds keep reading crates for {READ_GRACE_DAYS} more days, so nothing
					breaks overnight, and adding a card or subscribing turns publishing back on. We remind you in the last
					{TRIAL_REMINDER_DAYS} days: on the account page, and as a warning in
					<code>cargo publish</code>.
				</p>
			</FaqItem>
			<FaqItem question="Can we cancel?">
				<p>
					Yes, at any time, under Manage billing on the <a href="/account">account page</a>. Publishing stops
					at the end of the paid month; reading keeps working for {READ_GRACE_DAYS} days after that, then stops
					too.
				</p>
				<p>
					If your organisation shrinks to {FREE_MEMBER_LIMIT} members or fewer, it is free again straight away.
					The account page then suggests cancelling; we do not cancel for you, because member counts go up and down.
				</p>
				<p>Nothing is deleted: your index and every crate file are in your own repository, and stay there.</p>
			</FaqItem>
			<FaqItem question="What if a payment fails?">
				<p>
					Your registry keeps working while Stripe retries the payment, and the account page says so. An admin
					can update the card under Manage billing.
				</p>
			</FaqItem>
			<FaqItem question="Who manages billing?">
				<p>
					Admins of the GitHub organisation: they start the trial, add a card and cancel, from the
					<a href="/account">account page</a>. Stripe handles cards and invoices. Members see the plan and the
					registry’s status, but cannot change them.
				</p>
			</FaqItem>
		</div>
	</section>
</div>

<style>
	.intro {
		padding-top: 3.5rem;
		margin-bottom: 2.5rem;
	}
	h1 {
		font-size: var(--text-3xl);
		margin-bottom: 1rem;
	}
	.plans {
		display: grid;
		gap: 1.5rem;
	}
	@media (min-width: 52rem) {
		.plans {
			grid-template-columns: 2fr 3fr;
		}
	}
	.plan {
		display: flex;
		flex-direction: column;
		border: 2px solid var(--ink);
		overflow: hidden;
	}
	.plan.team {
		box-shadow: 6px 6px 0 var(--ink);
	}
	.plan-head {
		padding: 2rem 1.5rem 1rem;
		display: flex;
		flex-direction: column;
		align-items: flex-start;
		gap: 1rem;
	}
	h2 {
		font-size: var(--text-2xl);
	}
	.price {
		display: flex;
		flex-direction: column;
		gap: 0.35rem;
	}
	.figure {
		font-family: var(--font-display);
		font-weight: 800;
		font-size: var(--text-display);
		line-height: 0.9;
	}
	.per,
	.small,
	.note {
		color: var(--ink-soft);
	}
	.small {
		font-size: var(--text-sm);
	}
	.plan :global(.plan-ship) {
		width: 100%;
		height: auto;
		margin-top: auto;
		padding: 0 1rem 0.5rem;
	}
	/* Three crates on the quay: a small team's cargo. */
	.stack {
		margin-top: auto;
		display: flex;
		align-items: flex-end;
		gap: 0.25rem;
		padding: 1rem 1.5rem 1.5rem;
	}
	.stack :global(.stack-crate) {
		width: 4.5rem;
		height: auto;
	}
	.included {
		margin-top: 2.5rem;
		padding: 2rem 1.5rem;
	}
	@media (min-width: 40rem) {
		.included {
			padding: 2rem;
		}
	}
	.included h2 {
		margin-bottom: 0.5rem;
	}
	.note {
		margin-bottom: 1.5rem;
		max-width: 40rem;
	}
	.trust-link {
		margin: 1.75rem 0 0;
		padding-top: 1.25rem;
		border-top: 1px solid var(--rule);
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
	@media (min-width: 64rem) {
		dl {
			grid-template-columns: 1fr 1fr 1fr;
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
