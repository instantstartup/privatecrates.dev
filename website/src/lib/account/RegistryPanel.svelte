<script lang="ts">
	import { api, ApiError, hasSubscription, isActive, planOf, type Org, type Tenant } from '$lib/api';
	import Callout from '$lib/components/Callout.svelte';
	import {
		FREE_MEMBER_LIMIT,
		PRICE_USD,
		READ_GRACE_DAYS,
		TRIAL_MONTHS,
		TRIAL_REMINDER_DAYS
	} from '$lib/site';
	import ErrorNotice from './ErrorNotice.svelte';
	import { addDays, daysUntil, formatDate, plural } from './format';
	import GettingStarted from './GettingStarted.svelte';

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
	}

	let { org, tenant, planDetail, live, onchange }: Props = $props();

	const admin = $derived(org.role === 'admin');
	const plan = $derived(planOf(org));
	const limit = $derived(org.free_member_limit ?? FREE_MEMBER_LIMIT);
	const members = $derived(org.members ?? null);
	const trialEndsIso = $derived(org.trial_ends_at ?? tenant.trial_ends_at);
	const trialEnds = $derived(formatDate(trialEndsIso));
	const trialDaysLeft = $derived(daysUntil(trialEndsIso));
	const periodEndIso = $derived(org.current_period_end ?? tenant.current_period_end);
	const periodEnd = $derived(formatDate(periodEndIso));
	const readsUntil = $derived(periodEndIso ? formatDate(addDays(periodEndIso, READ_GRACE_DAYS)) : null);
	const hasCard = $derived(org.has_payment_method ?? plan === 'paid');
	/** The last days of a trial with no card: the page and Cargo both remind (billing model). */
	const trialEnding = $derived(
		plan === 'trial' && !hasCard && trialDaysLeft !== null && trialDaysLeft <= TRIAL_REMINDER_DAYS
	);
	/** Free again with a subscription still running: admins may cancel it (we never cancel automatically). */
	const freeAgain = $derived(plan === 'free' && isActive(tenant.status));
	const portal = $derived(admin && hasSubscription(org));
	const trialEndedWithoutCard = $derived(
		!hasCard && trialDaysLeft === 0 && !!trialEnds && (!periodEndIso || periodEndIso === trialEndsIso)
	);

	let busy = $state<'portal' | 'checkout' | 'trial' | null>(null);
	let failed = $state<'portal' | 'checkout' | 'trial' | null>(null);
	let error = $state<ApiError | null>(null);

	async function go(kind: 'portal' | 'checkout' | 'trial') {
		busy = kind;
		error = null;
		try {
			if (kind === 'trial') {
				await api.trial(org.login);
				busy = null;
				onchange?.();
				return;
			}
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
		checkout: 'Checkout could not be opened',
		trial: 'The free trial could not start'
	};
</script>

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
	{#if trialEnding}
		<Callout
			tone="warn"
			role="status"
			title={`The free trial ends ${trialEnds ? `on ${trialEnds}` : 'soon'}`}
		>
			<p>
				{trialDaysLeft === 0 ? 'Today is the last day.' : `${plural(trialDaysLeft ?? 0, 'day')} left.`}
				{org.login} has no card on file. Without one, publishing stops when the trial ends, and builds can read
				crates for {READ_GRACE_DAYS} more days.
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
	{:else if plan === 'past_due'}
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
		<GettingStarted org={org.login} {tenant} />
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

	<section class="billing" aria-labelledby="billing-{org.login}">
		<h3 id="billing-{org.login}">Plan and billing</h3>
		<dl class="facts">
			<div>
				<dt>Plan</dt>
				<dd>
					{#if plan === 'free'}
						Free. Every feature, no card.
					{:else if plan === 'trial'}
						Free trial{trialEnds ? ` until ${trialEnds}` : ''}{trialDaysLeft !== null
							? ` (${plural(trialDaysLeft, 'day')} left)`
							: ''}. Then ${PRICE_USD} per month{hasCard
							? ', charged to the card on file'
							: ' once you add a card'}.
					{:else if plan === 'paid'}
						${PRICE_USD} per month{periodEnd ? `, renews ${periodEnd}` : ''}.
					{:else if plan === 'past_due'}
						${PRICE_USD} per month. Payment overdue: Stripe is retrying the card.
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
		</dl>

		{#if freeAgain && admin}
			<Callout title={`${org.login} is free again`}>
				<p>
					It has {members === null ? `${limit} or fewer members` : plural(members, 'member')}, so the registry
					no longer needs a subscription. Yours is still running: cancel it under Manage billing if you do not
					expect to grow past {limit} members. We never cancel it for you, because member counts go up and down.
				</p>
			</Callout>
		{/if}

		{#if error && failed}
			<ErrorNotice {error} title={errorTitles[failed]} />
		{/if}

		{#if admin && (portal || !live)}
			<div class="row">
				{#if !live && org.trial_available}
					<button class="btn btn-primary" type="button" onclick={() => go('trial')} disabled={busy !== null}>
						{busy === 'trial' ? 'Starting the trial…' : `Start ${TRIAL_MONTHS}-month free trial`}
					</button>
				{:else if !live && hasSubscription(org)}
					<button
						class="btn btn-primary"
						type="button"
						onclick={() => go('checkout')}
						disabled={busy !== null}
					>
						{busy === 'checkout' ? 'Opening Stripe Checkout…' : `Subscribe, $${PRICE_USD} per month`}
					</button>
				{/if}
				{#if portal}
					{#if plan === 'trial' && !hasCard && !trialEnding}
						<button class="btn btn-quiet" type="button" onclick={() => go('portal')} disabled={busy !== null}>
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
</div>

<style>
	.registry {
		display: grid;
		gap: 2.5rem;
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
		border-top: 2px solid var(--ink);
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
