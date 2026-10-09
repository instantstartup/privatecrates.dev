<script lang="ts">
	import FaqItem from '$lib/components/FaqItem.svelte';
	import Seo from '$lib/components/Seo.svelte';
	import CargoShip from '$lib/illustrations/CargoShip.svelte';
	import Crate from '$lib/illustrations/Crate.svelte';
	import {
		FREE_MEMBER_LIMIT,
		GA_TARGET,
		PREVIEW,
		MEMBER_PRICE_USD,
		PRICE_CAP_USD,
		PRICE_PHRASE,
		READ_GRACE_DAYS,
		TERMS_PATH,
		TRIAL_MONTHS
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
		['The open-source verifier', 'Run it in your own CI, on every publish, to check everything we write.'],
		['Search', 'cargo search returns your private crates first, then crates.io results.'],
		['Your data stays yours', 'Index and crates live in a repository your organisation owns.']
	];

	const description = PREVIEW
		? `PrivateCrates is free during the preview. Planned pricing from general availability: always free for personal GitHub accounts, and for organisations ${PRICE_PHRASE}, with unlimited users and SSO included.`
		: `PrivateCrates is always free for personal GitHub accounts, and for organisations ${PRICE_PHRASE}, with unlimited users, SSO included and a ${TRIAL_MONTHS}-month free trial that needs no card.`;
</script>

<Seo title="Pricing" {description} path="/pricing" />

<div class="page">
	<header class="intro">
		{#if PREVIEW}
			<h1>Free during the preview</h1>
			<p class="lede">
				PrivateCrates takes no payments until general availability, planned for {GA_TARGET}. Below is what we
				plan to charge then, so you can judge it now. Billing starts only after at least 30 days’ notice.
			</p>
		{:else}
			<h1>Free for small teams, never more than ${PRICE_CAP_USD} a month</h1>
			<p class="lede">
				No enterprise tier to unlock SSO, and a price that stops growing: ${MEMBER_PRICE_USD} a month for each member
				past {FREE_MEMBER_LIMIT}, and never more than ${PRICE_CAP_USD} a month however large your organisation.
				Both plans have every feature.
			</p>
		{/if}
	</header>

	{#if PREVIEW}
		<div class="planned-head">
			<h2 id="planned">Planned pricing, from general availability</h2>
			<p class="note">
				Free during the preview, whatever your size. No per-seat maths and no enterprise tier to unlock SSO:
				both plans will have every feature.
			</p>
		</div>
	{/if}

	<section
		class={['plans', PREVIEW && 'planned']}
		aria-label={PREVIEW ? undefined : 'Plans'}
		aria-labelledby={PREVIEW ? 'planned' : undefined}
	>
		<article class="plan panel" aria-labelledby="plan-free">
			<div class="plan-head">
				<h2 id="plan-free">
					Free{#if PREVIEW}<span class="stamp">Planned</span>{/if}
				</h2>
				<p class="price">
					<span class="figure">$0</span>
					<span class="per"
						>for personal GitHub accounts, always, and for organisations with up to {FREE_MEMBER_LIMIT} members</span
					>
				</p>
				<a class="btn btn-quiet" href="/account">Set up a free registry</a>
				<p class="small">No card, no time limit. Outside collaborators do not count as members.</p>
			</div>
			<div class="stack" aria-hidden="true">
				{#each ['action', 'muted', 'accent'] as const as tone (tone)}
					<Crate {tone} class="stack-crate" />
				{/each}
			</div>
		</article>

		<article class="plan panel team" aria-labelledby="plan-team">
			<div class="plan-head">
				<h2 id="plan-team">
					Team{#if PREVIEW}<span class="stamp">Planned</span>{/if}
				</h2>
				<p class="price">
					<span class="figure">${MEMBER_PRICE_USD}</span>
					<span class="per"
						>a month for each member past {FREE_MEMBER_LIMIT}, never more than ${PRICE_CAP_USD} a month per organisation.
						Unlimited users.</span
					>
				</p>
				{#if PREVIEW}
					<a class="btn btn-primary" href="/account">Set up your registry</a>
					<p class="small">
						Free during the preview. From general availability, {TRIAL_MONTHS} months free with no card, then at
						most ${PRICE_CAP_USD} a month.
					</p>
				{:else}
					<a class="btn btn-primary" href="/account">Start {TRIAL_MONTHS}-month free trial</a>
					<p class="small">
						{TRIAL_MONTHS} months free, and no card to start. Add one any time before the trial ends.
					</p>
				{/if}
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
			{#if PREVIEW}
				<FaqItem question="Do we pay anything during the preview?">
					<p>
						No. Billing is off: we take no payments and ask for no card, whatever the size of your
						organisation. The preview is provided as is and at your own risk; the <a href={TERMS_PATH}
							>preview terms</a
						> say what that means.
					</p>
				</FaqItem>
				<FaqItem question="When does billing start?">
					<p>
						Only after general availability, planned for {GA_TARGET}, and only after at least 30 days’ notice
						to organisations already using PrivateCrates. Paying will need new terms, which an admin accepts.
						No charge is ever taken without a card an admin has added.
					</p>
				</FaqItem>
				<FaqItem question="What will organisations with more than {FREE_MEMBER_LIMIT} members pay?">
					<p>
						${MEMBER_PRICE_USD} a month for each member past {FREE_MEMBER_LIMIT}, and never more than ${PRICE_CAP_USD}
						a month per organisation, with unlimited users, after a {TRIAL_MONTHS}-month free trial that needs
						no card. Organisations with {FREE_MEMBER_LIMIT} members or fewer will stay free. The notice before billing
						starts confirms the details.
					</p>
				</FaqItem>
				<FaqItem question="Who counts as a member?">
					<p>
						The active members of your GitHub organisation, as GitHub lists them. Outside collaborators and
						pending invitations do not count, and neither do CI jobs. One organisation gets one registry at
						<code>your-name.privatecrates.dev</code>; a second organisation has its own plan.
					</p>
				</FaqItem>
				<FaqItem question="Can we stop?">
					<p>
						Yes, at any time: uninstall the PrivateCrates GitHub Apps from your organisation. Nothing is
						deleted: your index and every crate file are in your own repository, and stay there.
					</p>
				</FaqItem>
			{:else}
				<FaqItem question="Who counts as a member?">
					<p>
						The active members of your GitHub organisation, as GitHub lists them. Outside collaborators and
						pending invitations do not count, and neither do CI jobs. One organisation gets one registry at
						<code>your-name.privatecrates.dev</code>; a second organisation has its own plan.
					</p>
				</FaqItem>
				<FaqItem question="What happens when we grow past {FREE_MEMBER_LIMIT} members?">
					<p>
						Nothing breaks. The {TRIAL_MONTHS}-month free trial starts by itself, with no card. The account
						page, and a warning in every <code>cargo publish</code>, say when it ends and what you will pay
						after it, until an admin adds a card. A paying organisation that grows pays the new price from its
						next invoice, up to the ${PRICE_CAP_USD} cap. Organisations that already have more than {FREE_MEMBER_LIMIT}
						members start the same trial with one click when they set up.
					</p>
					<p>
						Each organisation gets one trial. If it ends while you have {FREE_MEMBER_LIMIT} members or fewer, growing
						past {FREE_MEMBER_LIMIT} again later starts a 14-day grace period instead, again with nothing broken
						and no card needed.
					</p>
				</FaqItem>
				<FaqItem question="What if the trial ends and we have not added a card?">
					<p>
						Publishing stops when the trial ends. Builds keep reading crates for {READ_GRACE_DAYS} more days, so
						nothing breaks overnight, and adding a card or subscribing turns publishing back on. We say so from
						the first day of the trial, until a card is added: on the account page, and as a warning in every
						<code>cargo publish</code>, with the date and what it will cost.
					</p>
				</FaqItem>
				<FaqItem question="Can we cancel?">
					<p>
						Yes, at any time, under Manage billing on the <a href="/account">account page</a>. Publishing
						stops at the end of the paid month; reading keeps working for {READ_GRACE_DAYS} days after that, then
						stops too.
					</p>
					<p>
						If your organisation shrinks to {FREE_MEMBER_LIMIT} members or fewer, it is free again straight away:
						the subscription stays, costing nothing, and picks up again if you grow. There is no need to cancel.
					</p>
					<p>
						Nothing is deleted: your index and every crate file are in your own repository, and stay there.
					</p>
				</FaqItem>
				<FaqItem question="What if a payment fails?">
					<p>
						Your registry keeps working while Stripe retries the payment, and the account page says so. An
						admin can update the card under Manage billing.
					</p>
				</FaqItem>
				<FaqItem question="Who manages billing?">
					<p>
						Admins of the GitHub organisation: they start the trial, add a card and cancel, from the
						<a href="/account">account page</a>. Stripe handles cards and invoices. Members see the plan and
						the registry’s status, but cannot change them.
					</p>
				</FaqItem>
			{/if}
		</div>
	</section>
</div>

<style>
	@reference '../../app.css';

	.intro {
		padding-top: var(--space-break);
		margin-bottom: var(--space-10);
	}
	h1 {
		font-size: var(--text-headline);
		margin-bottom: var(--space-5);
	}
	.plans {
		display: grid;
		gap: var(--space-7);
	}
	@media (width >= theme(--breakpoint-6)) {
		.plans {
			grid-template-columns: 2fr 3fr;
		}
	}
	.plan {
		display: flex;
		flex-direction: column;
		border: var(--border-outline) solid var(--line);
		overflow: hidden;
	}
	.plan.team {
		box-shadow: var(--shadow-offset-feature) var(--shadow-offset-feature) 0 var(--line);
	}
	.plan-head {
		padding: var(--space-9) var(--space-7) var(--space-5);
		display: flex;
		flex-direction: column;
		align-items: flex-start;
		gap: var(--space-5);
	}
	h2 {
		font-size: var(--text-heading);
	}
	.planned-head {
		margin-bottom: var(--space-6);
	}
	.planned-head h2 {
		margin-bottom: var(--space-after-title);
	}
	/* Planned, not charged: stamped on each plan, like the trust centre's manifest. */
	.stamp {
		display: inline-block;
		margin-left: var(--space-4);
		padding: var(--space-1) var(--space-3);
		border: var(--border-strong) solid var(--info);
		border-radius: var(--radius-inline);
		color: var(--info);
		font-size: var(--text-lead);
		line-height: 1;
		vertical-align: middle;
		transform: rotate(-3deg);
	}
	.price {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
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
		color: var(--ink-muted);
	}
	.small {
		font-size: var(--text-meta);
	}
	.plan :global(.plan-ship) {
		width: 100%;
		height: auto;
		margin-top: auto;
		padding: 0 var(--space-5) var(--space-3);
	}
	/* Three crates on the quay: a small team's cargo. */
	.stack {
		margin-top: auto;
		display: flex;
		align-items: flex-end;
		gap: var(--space-2);
		padding: var(--space-5) var(--space-7) var(--space-7);
	}
	.stack :global(.stack-crate) {
		width: var(--size-crate);
		height: auto;
	}
	.included {
		margin-top: var(--space-10);
		padding: var(--space-9) var(--space-7);
	}
	@media (width >= theme(--breakpoint-3)) {
		.included {
			padding: var(--space-9);
		}
	}
	.included h2 {
		margin-bottom: var(--space-3);
	}
	.note {
		margin-bottom: var(--space-7);
		max-width: var(--container-measure);
	}
	.trust-link {
		margin: var(--space-8) 0 0;
		padding-top: var(--space-6);
		border-top: var(--border-hairline) solid var(--divider);
	}
	dl {
		display: grid;
		gap: var(--space-definition);
	}
	@media (width >= theme(--breakpoint-3)) {
		dl {
			grid-template-columns: 1fr 1fr;
			gap: var(--space-definition-wide) var(--space-9);
		}
	}
	@media (width >= theme(--breakpoint-9)) {
		dl {
			grid-template-columns: 1fr 1fr 1fr;
		}
	}
	dt {
		font-weight: 700;
	}
	dd {
		color: var(--ink-muted);
	}
	.faq {
		padding-top: var(--space-section-inset);
	}
	.faq h2 {
		font-size: var(--text-headline);
		margin-bottom: var(--space-6);
	}
	.faq-list {
		max-width: var(--container-prose);
	}
</style>
