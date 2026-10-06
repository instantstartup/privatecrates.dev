<script lang="ts">
	import {
		api,
		ApiError,
		hasSubscription,
		isActive,
		planOf,
		priceOf,
		type Org,
		type Tenant,
		type Terms
	} from '$lib/api';
	import Callout from '$lib/components/Callout.svelte';
	import {
		FREE_MEMBER_LIMIT,
		MEMBER_PRICE_USD,
		PRICE_CAP_USD,
		READ_GRACE_DAYS,
		TRIAL_REMINDER_DAYS
	} from '$lib/site';
	import ErrorNotice from './ErrorNotice.svelte';
	import { addDays, daysUntil, formatDate, plural } from './format';
	import { page } from '$app/state';
	import Tabs, { tabIds, type Tab } from '$lib/components/Tabs.svelte';
	import ComplianceView from './ComplianceView.svelte';
	import GettingStarted from './GettingStarted.svelte';
	import TermsBanner from './TermsBanner.svelte';
	import TrialForm from './TrialForm.svelte';
	import VerifierCard from './VerifierCard.svelte';

	interface Props {
		org: Org;
		tenant: Tenant;
		/** The plan step's detail from the onboarding document, when the page has one (e.g. billing not configured). */
		planDetail?: string;
		/**
		 * Whether the registry works. The page decides: usually the plan is not `inactive`, but with billing not
		 * configured the onboarding document can say every step is done anyway.
		 */
		live: boolean;
		/** Re-reads the session, after starting a trial from here. */
		onchange?: () => void;
		/** The preview (session.preview): free for everyone, so no plan, trial or billing controls. */
		preview?: boolean;
		/** The current terms (session.terms), for a registry whose organisation has not accepted them. */
		terms?: Terms;
		/** Re-reads the session, after an admin accepts the terms here. */
		onterms?: () => void;
	}

	let { org, tenant, planDetail, live, onchange, preview = false, terms, onterms }: Props = $props();

	const tabs: Tab[] = [
		{ id: 'guide', label: 'Getting started' },
		{ id: 'compliance', label: 'Compliance' }
	];
	// ?tab=compliance opens the dashboard directly, e.g. from a link shared with an auditor.
	let view = $state(page.url.searchParams.get('tab') === 'compliance' ? 'compliance' : 'guide');
	let complianceOpened = $state(false);
	$effect(() => {
		if (view === 'compliance') complianceOpened = true;
	});

	const admin = $derived(org.role === 'admin');
	const plan = $derived(planOf(org));
	const price = $derived(priceOf(org));
	const limit = $derived(org.free_member_limit ?? FREE_MEMBER_LIMIT);
	const members = $derived(org.members ?? null);
	const trialEndsIso = $derived(org.trial_ends_at ?? tenant.trial_ends_at);
	const trialEnds = $derived(formatDate(trialEndsIso));
	const trialDaysLeft = $derived(daysUntil(trialEndsIso));
	const periodEndIso = $derived(org.current_period_end ?? tenant.current_period_end);
	const periodEnd = $derived(formatDate(periodEndIso));
	const readsUntil = $derived(periodEndIso ? formatDate(addDays(periodEndIso, READ_GRACE_DAYS)) : null);
	const hasCard = $derived(org.has_payment_method ?? plan === 'paid');
	/** A trial or grace period with no card: said throughout, here and in every `cargo publish` (billing model). */
	const trialNoCard = $derived(!preview && plan === 'trial' && !hasCard);
	/** Its last days, when the reminder becomes a warning. */
	const trialEnding = $derived(trialNoCard && trialDaysLeft !== null && trialDaysLeft <= TRIAL_REMINDER_DAYS);
	/** Not the first trial: the organisation grew past the free limit again after an earlier one ended. */
	const graceName = $derived(org.grace_period ? 'grace period' : 'free trial');
	/** The tighter of the organisation's two GitHub App allowances, as of our latest call (null before one). */
	const allowance = $derived(tenant.github_allowance ?? null);
	const allowanceResets = $derived(
		allowance
			? new Date(allowance.resets_at).toLocaleTimeString('en-GB', {
					hour: '2-digit',
					minute: '2-digit',
					timeZone: 'UTC',
					timeZoneName: 'short'
				})
			: null
	);
	/** Free again with a subscription still running: admins may cancel it (we never cancel automatically). */
	const freeAgain = $derived(plan === 'free' && isActive(tenant.status));
	const portal = $derived(admin && hasSubscription(org));
	const trialEndedWithoutCard = $derived(
		!hasCard && trialDaysLeft === 0 && !!trialEnds && (!periodEndIso || periodEndIso === trialEndsIso)
	);

	let busy = $state<'portal' | 'checkout' | null>(null);
	let failed = $state<'portal' | 'checkout' | null>(null);
	let error = $state<ApiError | null>(null);

	async function go(kind: 'portal' | 'checkout') {
		busy = kind;
		error = null;
		try {
			const { url } = kind === 'portal' ? await api.portal(org.login) : await api.checkout(org.login);
			location.assign(url);
		} catch (e) {
			error = e instanceof ApiError ? e : new ApiError(0, [{ detail: String(e) }]);
			failed = kind;
			busy = null;
		}
	}

	const errorTitles = {
		portal: 'Billing could not be opened',
		checkout: 'Checkout could not be opened'
	};
</script>

{#snippet allowanceFact()}
	{#if allowance}
		<div>
			<dt>GitHub API</dt>
			<dd>
				{allowance.remaining.toLocaleString('en-GB')} of {allowance.limit.toLocaleString('en-GB')} calls left this
				hour.
				<span class="detail"
					>Each organisation has its own hourly allowance from GitHub; builds and publishes use it. <a
						href="/docs/setup#limits">About limits</a
					></span
				>
			</dd>
		</div>
	{/if}
{/snippet}

{#snippet memberLine()}
	{#if members === null}
		Free for organisations with up to {limit} members.
	{:else if members <= limit}
		{members} of {limit} members, so {org.login} is on the free plan.
	{:else}
		{plural(members, 'member')}. The free plan covers up to {limit}.
	{/if}
{/snippet}

<div class="registry">
	{#if terms && org.terms_accepted === false}
		<TermsBanner org={org.login} {terms} {admin} onaccepted={() => onterms?.()} />
	{/if}
	{#if org.billing_email_missing && admin && !preview}
		<Callout tone="warn" role="status" title="Add a billing email">
			<p>
				{org.login}’s free trial started by itself when it grew past {limit} members. Stripe needs an address to
				send the reminder before the trial ends.
			</p>
			<TrialForm org={org.login} {price} purpose="email" onstarted={() => onchange?.()} />
		</Callout>
	{/if}
	{#if allowance?.running_low}
		<Callout tone="warn" role="status" title="GitHub’s hourly allowance is running low">
			<p>
				{org.login} has {allowance.remaining.toLocaleString('en-GB')} of its {allowance.limit.toLocaleString(
					'en-GB'
				)} GitHub API calls left{allowanceResets ? `, until ${allowanceResets}` : ''}. If they run out,
				downloads and publishing pause until GitHub refills them. Busy CI is the usual cause:
				<a href="/docs/setup#limits">how to use fewer</a>.
			</p>
		</Callout>
	{/if}
	{#if trialNoCard}
		<Callout
			tone={trialEnding ? 'warn' : undefined}
			role="status"
			title={`The ${graceName} ends ${trialEnds ? `on ${trialEnds}` : 'soon'}`}
		>
			<p>
				{org.login} has {members === null ? `more than ${limit} members` : plural(members, 'member')}, more
				than the free {limit}, so it is on a {graceName}{trialDaysLeft === null
					? ''
					: trialDaysLeft === 0
						? ', and today is its last day'
						: ` with ${plural(trialDaysLeft, 'day')} left`}. After it, ${price} a month. There is no card on file:
				without one, publishing stops when the {graceName} ends, and builds can read crates for {READ_GRACE_DAYS}
				more days.
			</p>
			{#if admin}
				<p>
					<button class="btn btn-primary" type="button" onclick={() => go('portal')} disabled={busy !== null}>
						{busy === 'portal' ? 'Opening billing…' : 'Add a card'}
					</button>
				</p>
			{:else}
				<p>Ask an admin of {org.login} to add a card.</p>
			{/if}
		</Callout>
	{:else if plan === 'past_due' && !preview}
		<Callout tone="warn" role="status" title="The last payment failed">
			<p>
				Your registry keeps working while Stripe retries the card.
				{admin
					? 'Update the card under Manage billing to settle it.'
					: 'Ask an admin of this organisation to update the card.'}
			</p>
		</Callout>
	{/if}

	{#if live}
		{#if org.role === 'admin'}
			<VerifierCard org={org.login} />
		{/if}
		<div class="views">
			<Tabs {tabs} bind:selected={view} prefix="registry-{org.login}" label="Registry" />
			<div
				role="tabpanel"
				id={tabIds(`registry-${org.login}`, 'guide').panel}
				aria-labelledby={tabIds(`registry-${org.login}`, 'guide').tab}
				hidden={view !== 'guide'}
			>
				<GettingStarted org={org.login} {tenant} />
			</div>
			<div
				role="tabpanel"
				id={tabIds(`registry-${org.login}`, 'compliance').panel}
				aria-labelledby={tabIds(`registry-${org.login}`, 'compliance').tab}
				hidden={view !== 'compliance'}
			>
				<!-- Loaded the first time the tab is opened, then kept, so switching back does not reload it. -->
				{#if complianceOpened}
					<ComplianceView org={org.login} slug={tenant.slug} />
				{/if}
			</div>
		</div>
	{:else}
		<section class="paused" aria-labelledby="paused-{org.login}">
			<h3 id="paused-{org.login}">Your registry is paused</h3>
			<p class="url"><a href={tenant.registry_url}>{tenant.registry_url}</a></p>
			<Callout tone="danger" title="Publishing is stopped">
				<p>
					{org.login} has no active subscription{trialEndedWithoutCard
						? `: the free trial ended on ${trialEnds} without a card`
						: periodEnd
							? ` since ${periodEnd}`
							: ''}. Builds can still read crates{readsUntil
						? ` until ${readsUntil}`
						: ` for ${READ_GRACE_DAYS} days after the subscription ends`}, then reads stop too. Your crates
					and index stay in your storage repository.
				</p>
				{#if !admin}<p>Ask an admin of this organisation to subscribe.</p>{/if}
			</Callout>
		</section>
	{/if}

	{#if preview}
		<section class="billing" aria-labelledby="billing-{org.login}">
			<h3 id="billing-{org.login}">Plan</h3>
			<dl class="facts">
				<div>
					<dt>Plan</dt>
					<dd>
						<strong>Free during the preview.</strong>
						<span class="detail"
							>Billing is off until general availability, and starts only after at least 30 days’ notice. See
							the
							<a href="/pricing">planned pricing</a>.</span
						>
					</dd>
				</div>
				{@render allowanceFact()}
			</dl>
		</section>
	{:else}
		<section class="billing" aria-labelledby="billing-{org.login}">
			<h3 id="billing-{org.login}">Plan and billing</h3>
			<dl class="facts">
				<div>
					<dt>Plan</dt>
					<dd>
						{#if plan === 'free'}
							Free. Every feature, no card.
						{:else if plan === 'trial'}
							{org.grace_period ? 'Grace period' : 'Free trial'}{trialEnds
								? ` until ${trialEnds}`
								: ''}{trialDaysLeft !== null ? ` (${plural(trialDaysLeft, 'day')} left)` : ''}. Then ${price} a
							month{hasCard ? ', charged to the card on file' : ' once an admin adds a card'}.
						{:else if plan === 'paid'}
							${price} a month{periodEnd ? `, renews ${periodEnd}` : ''}.
							<span class="fine"
								>${MEMBER_PRICE_USD} for each member past {FREE_MEMBER_LIMIT}, never more than ${PRICE_CAP_USD}.</span
							>
						{:else if plan === 'past_due'}
							${price} a month. Payment overdue: Stripe is retrying the card.
						{:else if live}
							No subscription needed.
						{:else}
							No active subscription.
						{/if}
						{#if planDetail}<span class="detail">{planDetail}</span>{/if}
					</dd>
				</div>
				<div>
					<dt>Members</dt>
					<dd>{@render memberLine()}</dd>
				</div>
				{@render allowanceFact()}
			</dl>

			{#if freeAgain && admin}
				<Callout title={`${org.login} is free again`}>
					<p>
						It has {members === null ? `${limit} or fewer members` : plural(members, 'member')}, so its
						subscription costs nothing until it grows past {limit} again, when it picks up where it left off. You
						can cancel it under Manage billing, but there is no need.
					</p>
				</Callout>
			{/if}

			{#if error && failed}
				<ErrorNotice {error} title={errorTitles[failed]} />
			{/if}

			{#if admin && !live && org.trial_available}
				<TrialForm org={org.login} {price} onstarted={() => onchange?.()} />
			{/if}
			{#if admin && (portal || (!live && !org.trial_available))}
				<div class="row">
					{#if !live && !org.trial_available && hasSubscription(org)}
						<button
							class="btn btn-primary"
							type="button"
							onclick={() => go('checkout')}
							disabled={busy !== null}
						>
							{busy === 'checkout' ? 'Opening Stripe Checkout…' : `Subscribe, $${price} a month`}
						</button>
					{/if}
					{#if portal}
						{#if plan === 'trial' && !hasCard && !trialNoCard}
							<button
								class="btn btn-quiet"
								type="button"
								onclick={() => go('portal')}
								disabled={busy !== null}
							>
								{busy === 'portal' ? 'Opening billing…' : 'Add a card'}
							</button>
						{/if}
						<button class="btn btn-quiet" type="button" onclick={() => go('portal')} disabled={busy !== null}>
							{busy === 'portal' ? 'Opening billing…' : 'Manage billing'}
						</button>
						<span class="fine">Cards, invoices and cancellation are on Stripe.</span>
					{/if}
				</div>
			{:else if !admin && plan !== 'free'}
				<p class="fine">Admins of {org.login} on GitHub manage billing.</p>
			{/if}
		</section>
	{/if}
</div>

<style>
	.registry {
		display: grid;
		gap: 2.5rem;
	}
	.views {
		display: grid;
		gap: 1.75rem;
		min-width: 0;
	}
	[role='tabpanel'] {
		min-width: 0;
	}
	.paused,
	.billing {
		display: grid;
		gap: 1rem;
	}
	h3 {
		font-size: var(--text-2xl);
	}
	.paused h3 {
		font-size: var(--text-3xl);
	}
	.billing {
		padding-top: 1.5rem;
		border-top: 2px solid var(--line);
	}
	.url {
		font-family: var(--font-mono);
		font-size: var(--text-base);
		overflow-wrap: anywhere;
		margin-top: -0.5rem;
	}
	.facts {
		display: grid;
		gap: 0.75rem;
	}
	.facts > div {
		display: grid;
		gap: 0.1rem 1rem;
	}
	@media (min-width: 40rem) {
		.facts > div {
			grid-template-columns: 8rem 1fr;
		}
	}
	dt {
		font-size: var(--text-sm);
		color: var(--ink-soft);
		padding-top: 0.15rem;
	}
	.detail {
		display: block;
		color: var(--ink-soft);
	}
	.row {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 0.75rem;
	}
	.fine {
		font-size: var(--text-sm);
		color: var(--ink-soft);
	}
</style>
