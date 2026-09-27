<script lang="ts">
	import { api, ApiError, isActive, type Org, type Tenant } from '$lib/api';
	import Callout from '$lib/components/Callout.svelte';
	import CodeBlock from '$lib/components/CodeBlock.svelte';
	import { PRICE_USD } from '$lib/site';
	import { cargoConfig, dependency } from '$lib/snippets';
	import ErrorNotice from './ErrorNotice.svelte';
	import { addDays, formatDate, READ_GRACE_DAYS } from './format';

	interface Props {
		org: Org;
		tenant: Tenant;
	}

	let { org, tenant }: Props = $props();

	const admin = $derived(org.role === 'admin');
	const active = $derived(isActive(tenant.status));
	const trialEnds = $derived(formatDate(tenant.trial_ends_at));
	const periodEnd = $derived(formatDate(tenant.current_period_end));
	const readsUntil = $derived(
		tenant.current_period_end ? formatDate(addDays(tenant.current_period_end, READ_GRACE_DAYS)) : null
	);

	let busy = $state<'portal' | 'checkout' | null>(null);
	let error = $state<ApiError | null>(null);

	async function go(kind: 'portal' | 'checkout') {
		busy = kind;
		error = null;
		try {
			const { url } = kind === 'portal' ? await api.portal(org.login) : await api.checkout(org.login);
			location.assign(url);
		} catch (e) {
			error = e instanceof ApiError ? e : new ApiError(0, [{ detail: String(e) }]);
			busy = null;
		}
	}
</script>

<section class="registry" aria-labelledby="registry-{org.login}">
	<h3 id="registry-{org.login}">Your registry</h3>
	<p class="url"><a href={tenant.registry_url}>{tenant.registry_url}</a></p>

	<dl class="facts">
		<div>
			<dt>Registry name</dt>
			<dd><code>{tenant.slug}</code></dd>
		</div>
		<div>
			<dt>Plan</dt>
			<dd>
				{#if tenant.status === 'trialing'}
					Free trial{trialEnds ? `, ends ${trialEnds}` : ''}. Then ${PRICE_USD} per month.
				{:else if tenant.status === 'active'}
					${PRICE_USD} per month{periodEnd ? `, renews ${periodEnd}` : ''}.
				{:else if tenant.status === 'past_due'}
					Payment overdue. Stripe is retrying the card.
				{:else}
					Cancelled{periodEnd ? ` on ${periodEnd}` : ''}.
				{/if}
			</dd>
		</div>
	</dl>

	{#if tenant.status === 'past_due'}
		<Callout tone="warn" title="The last payment failed">
			<p>
				Your registry keeps working while Stripe retries.
				{admin
					? 'Update the card in the billing portal to settle it.'
					: 'Ask an admin of this organisation to update the card.'}
			</p>
		</Callout>
	{:else if !active}
		<Callout tone="danger" title="The subscription is not active">
			<p>
				Publishing is refused. Builds can still read crates{readsUntil
					? ` until ${readsUntil}`
					: ' for 14 days after the paid period ends'}, then reads stop too. Your crates and index stay in
				your storage repository.
			</p>
			{#if !admin}<p>Ask an admin of this organisation to restart the subscription.</p>{/if}
		</Callout>
	{/if}

	{#if error}
		<ErrorNotice
			{error}
			title={busy === 'checkout' ? 'Checkout could not be opened' : 'Billing could not be opened'}
		/>
	{/if}

	{#if admin}
		<div class="row">
			{#if !active}
				<button class="btn btn-primary" type="button" onclick={() => go('checkout')} disabled={busy !== null}>
					{busy === 'checkout' ? 'Opening Stripe Checkout…' : `Restart subscription, $${PRICE_USD} per month`}
				</button>
			{/if}
			<button class="btn btn-quiet" type="button" onclick={() => go('portal')} disabled={busy !== null}>
				{busy === 'portal' ? 'Opening billing…' : 'Manage billing'}
			</button>
			<span class="fine">Invoices, card details and cancellation are on Stripe.</span>
		</div>
	{/if}

	{#if active}
		<h4>Connect Cargo</h4>
		<p>
			Commit this to each project, install the credential provider with
			<code>cargo install cargo-credential-privatecrates --locked</code>, and build.
		</p>
		<CodeBlock caption=".cargo/config.toml" code={cargoConfig(tenant.slug, tenant.registry_url)} />
		<CodeBlock caption="Cargo.toml" code={dependency(tenant.slug)} />
		<p class="next">
			Next: <a href="/docs/ci">read crates in CI</a>, <a href="/docs/publishing">publish from a tag</a> and
			<a href="/docs/verify">verify the registry</a>.
		</p>
	{/if}
</section>

<style>
	.registry {
		display: grid;
		gap: 1rem;
	}
	h3 {
		font-size: var(--text-2xl);
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
	@media (min-width: 40rem) {
		.facts {
			grid-template-columns: 12rem 1fr;
		}
	}
	dt {
		font-size: var(--text-sm);
		color: var(--ink-soft);
	}
	h4 {
		font-family: var(--font-display);
		font-weight: 800;
		font-size: var(--text-xl);
		margin-top: 1rem;
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
	.next {
		color: var(--ink-soft);
	}
</style>
