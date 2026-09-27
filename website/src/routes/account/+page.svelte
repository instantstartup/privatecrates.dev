<script lang="ts">
	import { browser } from '$app/environment';
	import { page } from '$app/state';
	import { untrack } from 'svelte';
	import Checklist from '$lib/account/Checklist.svelte';
	import ErrorNotice from '$lib/account/ErrorNotice.svelte';
	import OrgList from '$lib/account/OrgList.svelte';
	import RegistryPanel from '$lib/account/RegistryPanel.svelte';
	import { api, ApiError, isActive, loginUrl, type Onboarding, type Session } from '$lib/api';
	import Callout from '$lib/components/Callout.svelte';
	import Seo from '$lib/components/Seo.svelte';
	import Crane from '$lib/illustrations/Crane.svelte';
	import { PRICE_USD, TRIAL_DAYS } from '$lib/site';

	type View = { kind: 'loading' } | { kind: 'failed'; error: ApiError } | { kind: 'ready'; session: Session };

	let view = $state<View>({ kind: 'loading' });

	// Query parameters exist only in the browser; the prerendered shell shows the loading state.
	const search = $derived(browser ? page.url.search : '');
	const params = $derived(new URLSearchParams(search));
	const checkoutSuccess = $derived(params.get('checkout') === 'success');

	const session = $derived(view.kind === 'ready' ? view.session : null);
	const orgs = $derived(session?.orgs ?? []);
	const selectedLogin = $derived(params.get('org') ?? (orgs.length === 1 ? orgs[0].login : null));
	const selected = $derived(orgs.find((o) => o.login === selectedLogin) ?? null);
	const installUrl = $derived(session?.install_url ?? null);
	/** A registry with a subscription (in any state); otherwise the set-up checklist is shown. */
	const subscribed = $derived(selected?.tenant?.status != null);
	const pageTitle = $derived(
		view.kind !== 'ready'
			? 'Account'
			: !session?.user
				? 'Sign in'
				: selected
					? `${selected.login}: account`
					: 'Account'
	);

	function toApiError(e: unknown): ApiError {
		return e instanceof ApiError ? e : new ApiError(0, [{ detail: String(e) }]);
	}

	async function loadSession(showLoading: boolean) {
		if (showLoading) view = { kind: 'loading' };
		try {
			view = { kind: 'ready', session: await api.session() };
		} catch (e) {
			view = { kind: 'failed', error: toApiError(e) };
		}
	}

	// Load on arrival, and again whenever the query changes (e.g. returning from GitHub or Stripe).
	$effect(() => {
		void search;
		untrack(() => loadSession(view.kind !== 'ready'));
	});

	// Onboarding for an organisation that is not set up yet.
	let onboarding = $state<Onboarding | null>(null);
	let onboardingError = $state<ApiError | null>(null);
	let refreshing = $state(false);

	async function loadOnboarding(login: string) {
		refreshing = true;
		onboardingError = null;
		try {
			const doc = await api.onboarding(login);
			if (selected?.login === login) onboarding = doc;
		} catch (e) {
			onboardingError = toApiError(e);
		} finally {
			refreshing = false;
		}
	}

	// A string, so re-reading the session (which replaces every object) does not reload the checklist.
	const onboardingFor = $derived(selected && !subscribed ? selected.login : null);
	$effect(() => {
		const login = onboardingFor;
		untrack(() => {
			onboarding = null;
			onboardingError = null;
			if (login) loadOnboarding(login);
		});
	});

	// After Stripe Checkout, the subscription reaches us by webhook; poll briefly until the tenant appears.
	let waitingForStripe = $state(false);
	const awaitingTenant = $derived(
		checkoutSuccess && selected !== null && !(selected.tenant && isActive(selected.tenant.status))
	);
	$effect(() => {
		if (!awaitingTenant) {
			waitingForStripe = false;
			return;
		}
		waitingForStripe = true;
		let tries = 0;
		const timer = setInterval(async () => {
			tries += 1;
			await loadSession(false);
			if (tries >= 10) {
				clearInterval(timer);
				waitingForStripe = false;
			}
		}, 2000);
		return () => clearInterval(timer);
	});

	let signingOut = $state(false);
	let signOutError = $state<ApiError | null>(null);
	async function signOut() {
		signingOut = true;
		signOutError = null;
		try {
			await api.logout();
			await loadSession(false);
		} catch (e) {
			signOutError = toApiError(e);
		} finally {
			signingOut = false;
		}
	}
</script>

<Seo
	title={pageTitle}
	description="Sign in with GitHub to set up your organisation's private Cargo registry, start a trial and manage billing."
	path="/account"
	noindex
/>

<div class="page account">
	{#if view.kind === 'loading'}
		<h1>Account</h1>
		<div class="loading panel" role="status" aria-busy="true">
			<span class="bar"></span>
			<span class="bar short"></span>
			<span class="visually-hidden">Loading your account…</span>
		</div>
	{:else if view.kind === 'failed'}
		<h1>Account</h1>
		<ErrorNotice error={view.error} title="Your account could not be loaded">
			<p>
				<button class="btn btn-quiet" type="button" onclick={() => loadSession(true)}>Try again</button>
			</p>
		</ErrorNotice>
	{:else if !view.session.user}
		<div class="signed-out">
			<div>
				<h1>Sign in with GitHub to set up your registry</h1>
				<p class="lede">
					We ask GitHub who you are and which organisations you belong to. Admins can set up a registry and
					manage billing; members can see its status.
				</p>
				<div class="cta">
					<a class="btn btn-primary" href={loginUrl('/account')} data-sveltekit-reload>Sign in with GitHub</a>
				</div>
				<p class="fine">
					{TRIAL_DAYS}-day free trial, then ${PRICE_USD} per organisation per month. The sign-in lasts 8 hours and
					is kept only in an encrypted cookie.
				</p>
			</div>
			<Crane class="signed-out-art" label="you" />
		</div>
	{:else}
		{@const user = view.session.user}
		<div class="who">
			<h1>Account</h1>
			<p class="signed-in">
				<span class="avatar" aria-hidden="true">{user.login.slice(0, 1).toUpperCase()}</span>
				Signed in as <strong>{user.name ?? user.login}</strong>
				{#if user.name}<span class="login">({user.login})</span>{/if}
				<button class="btn btn-quiet" type="button" onclick={signOut} disabled={signingOut}>
					{signingOut ? 'Signing out…' : 'Sign out'}
				</button>
			</p>
		</div>
		{#if signOutError}
			<ErrorNotice error={signOutError} title="You are still signed in" />
		{/if}

		{#if orgs.length === 0}
			<div class="empty panel">
				<h2>Set up your first organisation</h2>
				<p>
					A registry belongs to a GitHub organisation. Organisations appear here once they have installed the
					PrivateCrates reader App, which only reads repository metadata and membership.
				</p>
				{#if installUrl}
					<div>
						<a class="btn btn-primary" href={installUrl}>Set up a new organisation</a>
					</div>
					<p class="fine">
						On GitHub, choose the organisation and approve the install; it needs an organisation owner. GitHub
						then brings you back here.
					</p>
				{:else}
					<p>Ask an owner of your organisation to install the reader App, then reload this page.</p>
				{/if}
				<p class="fine">
					Already installed? Your organisation may restrict third-party access; an owner can approve
					PrivateCrates in its settings. <a href="/docs/setup">Read the set-up guide</a>.
				</p>
			</div>
		{:else}
			<div class="layout">
				<div class="orgs">
					<OrgList {orgs} selected={selected?.login ?? null} />
					{#if installUrl}
						<div class="add-org">
							<a class="btn btn-quiet" href={installUrl}>Set up a new organisation</a>
							<p class="fine">Installs the reader App on another organisation, then returns here.</p>
						</div>
					{/if}
				</div>

				<div class="detail">
					{#if !selected}
						<div class="empty panel">
							<h2>Choose an organisation</h2>
							<p>Pick one from the list to see its registry, or to set one up.</p>
						</div>
					{:else}
						<div class="panel detail-panel">
							<h2 class="org-name">{selected.login}</h2>
							<p class="role">
								You are {selected.role === 'admin' ? 'an admin' : 'a member'} of this organisation on GitHub.
							</p>

							{#if checkoutSuccess}
								{#if selected.tenant && isActive(selected.tenant.status)}
									<Callout tone="ok" role="status" title="Your trial has started">
										<p>Your registry is live. Connect Cargo below.</p>
									</Callout>
								{:else if waitingForStripe}
									<Callout role="status" title="Checkout complete">
										<p>Waiting for Stripe to confirm the subscription. This usually takes a few seconds.</p>
									</Callout>
								{:else}
									<Callout tone="warn" role="status" title="Stripe has not confirmed yet">
										<p>
											Checkout finished, but the subscription has not reached us. Reload this page in a
											minute; if it still shows as not set up, contact support.
										</p>
									</Callout>
								{/if}
							{/if}

							{#if selected.tenant && subscribed}
								<RegistryPanel org={selected} tenant={selected.tenant} />
							{:else if onboardingError}
								<ErrorNotice error={onboardingError} title="The set-up checklist could not be loaded">
									<p>
										<button class="btn btn-quiet" type="button" onclick={() => loadOnboarding(selected.login)}
											>Try again</button
										>
									</p>
								</ErrorNotice>
							{:else if onboarding}
								<Checklist
									doc={onboarding}
									admin={selected.role === 'admin'}
									{refreshing}
									onrefresh={() => loadOnboarding(selected.login)}
									onchange={(doc) => (onboarding = doc)}
								/>
							{:else}
								<div class="loading" role="status" aria-busy="true">
									<span class="bar"></span>
									<span class="bar short"></span>
									<span class="visually-hidden">Loading the set-up checklist…</span>
								</div>
							{/if}
						</div>
					{/if}
				</div>
			</div>
		{/if}
	{/if}
</div>

<style>
	.account {
		padding-top: 2.5rem;
		min-height: 60vh;
	}
	h1 {
		font-size: var(--text-3xl);
		margin-bottom: 1.5rem;
	}
	.loading {
		display: grid;
		gap: 0.75rem;
		padding: 1.5rem;
		min-height: 10rem;
	}
	.bar {
		display: block;
		height: 1.1rem;
		border-radius: 4px;
		background: color-mix(in srgb, var(--ink) 10%, transparent);
		animation: pulse 1.4s ease-in-out infinite;
	}
	.bar.short {
		width: 60%;
	}
	@keyframes pulse {
		50% {
			opacity: 0.45;
		}
	}

	.signed-out {
		display: grid;
		gap: 2rem;
		align-items: center;
	}
	@media (min-width: 52rem) {
		.signed-out {
			grid-template-columns: 1fr 16rem;
		}
	}
	.signed-out :global(.signed-out-art) {
		width: 100%;
		max-width: 14rem;
		height: auto;
		justify-self: center;
	}
	.cta {
		margin-top: 1.75rem;
	}
	.fine {
		margin-top: 1rem;
		font-size: var(--text-sm);
		color: var(--ink-soft);
		max-width: 38rem;
	}

	.who {
		display: flex;
		flex-wrap: wrap;
		align-items: baseline;
		justify-content: space-between;
		gap: 0 1.5rem;
		margin-bottom: 1.5rem;
	}
	.who h1 {
		margin-bottom: 0;
	}
	.signed-in {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 0.5rem;
	}
	.login {
		color: var(--ink-soft);
	}
	.avatar {
		display: inline-grid;
		place-items: center;
		width: 2rem;
		height: 2rem;
		border: 2px solid var(--ink);
		border-radius: 50%;
		background: var(--signal);
		color: var(--signal-ink);
		font-weight: 700;
	}
	.orgs {
		display: grid;
		gap: 1.25rem;
	}
	.add-org {
		display: grid;
		gap: 0.4rem;
		justify-items: start;
		padding-inline: 0.75rem;
	}

	.layout {
		display: grid;
		gap: 2rem;
		align-items: start;
	}
	@media (min-width: 56rem) {
		.layout {
			grid-template-columns: 18rem minmax(0, 1fr);
		}
	}
	.detail {
		min-width: 0;
	}
	.detail-panel,
	.empty {
		padding: 1.5rem 1.25rem;
		display: grid;
		gap: 1.25rem;
	}
	@media (min-width: 40rem) {
		.detail-panel,
		.empty {
			padding: 2rem;
		}
	}
	.empty {
		max-width: var(--container-prose);
	}
	.empty h2 {
		font-size: var(--text-2xl);
	}
	.org-name {
		font-size: var(--text-2xl);
		overflow-wrap: anywhere;
	}
	.role {
		color: var(--ink-soft);
		margin-top: -0.75rem;
	}
</style>
