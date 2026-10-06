<script lang="ts">
	import { browser } from '$app/environment';
	import { page } from '$app/state';
	import { untrack } from 'svelte';
	import Checklist from '$lib/account/Checklist.svelte';
	import ErrorNotice from '$lib/account/ErrorNotice.svelte';
	import OrgList from '$lib/account/OrgList.svelte';
	import RegistryPanel from '$lib/account/RegistryPanel.svelte';
	import {
		allDone,
		api,
		ApiError,
		isActive,
		loginUrl,
		planOf,
		type Onboarding,
		type Org,
		type Session
	} from '$lib/api';
	import Callout from '$lib/components/Callout.svelte';
	import Seo from '$lib/components/Seo.svelte';
	import Crane from '$lib/illustrations/Crane.svelte';
	import { PREVIEW, PRICE_PHRASE, TERMS_PATH, TRIAL_MONTHS } from '$lib/site';

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
	/** The preview: free for everyone, billing off (docs/preview.md §1). Billing controls return when it ends. */
	const preview = $derived(session?.preview ?? false);
	const terms = $derived(session?.terms);
	// Onboarding documents that came back with every step done, by organisation. With billing not configured the
	// session alone cannot tell that the registry works, so these count too.
	let finished = $state<Record<string, Onboarding>>({});

	/** The registry works: configured, and free, trialling, paid or retrying a payment. */
	function isLive(org: Org): boolean {
		return org.tenant !== null && (planOf(org) !== 'inactive' || org.login in finished);
	}
	const live = $derived(selected ? isLive(selected) : false);
	/** Configured, but its subscription ended: the registry view explains, with the way back. */
	const paused = $derived(!!selected && !live && selected.tenant !== null && selected.tenant.status !== null);
	const showRegistry = $derived(live || paused);
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

	// Installing the reader App happens in a new tab; when the user comes back, their new organisation may be listed.
	let awaitingInstall = $state(false);
	$effect(() => {
		const onVisible = () => {
			if (document.visibilityState === 'visible' && awaitingInstall) {
				awaitingInstall = false;
				void loadSession(false);
			}
		};
		document.addEventListener('visibilitychange', onVisible);
		return () => document.removeEventListener('visibilitychange', onVisible);
	});

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
			onboardingChanged(login, doc);
		} catch (e) {
			onboardingError = toApiError(e);
		} finally {
			refreshing = false;
		}
	}

	/**
	 * After a POST (settings, trial) or a re-check: when every step is done, remember it. Either way re-read the
	 * session when something changed, so the organisation list and the registry view catch up without a reload.
	 */
	function onboardingChanged(login: string, doc: Onboarding, posted = false) {
		const done = allDone(doc);
		if (done) finished = { ...finished, [login]: doc };
		if (done || posted) void loadSession(false);
	}

	// Set when the trial is started from this page, for the confirmation above the registry.
	let trialStartedFor = $state<string | null>(null);

	// A string, so re-reading the session (which replaces every object) does not reload the checklist.
	const onboardingFor = $derived(selected && !showRegistry ? selected.login : null);
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

	let confirmingEverywhere = $state(false);
	let signOutEverywhereError = $state<ApiError | null>(null);
	async function signOutEverywhere() {
		signingOut = true;
		signOutEverywhereError = null;
		try {
			await api.logoutEverywhere();
			confirmingEverywhere = false;
			await loadSession(false);
		} catch (e) {
			signOutEverywhereError = toApiError(e);
		} finally {
			signingOut = false;
		}
	}
</script>

<Seo
	title={pageTitle}
	description="Sign in with GitHub to set up your organisation's private Cargo registry."
	path="/account"
	noindex
/>

<div class={['page', 'account', view.kind === 'loading' && 'busy']}>
	{#if view.kind === 'loading'}
		<!-- The same header as when signed in, with the sign-in line's space kept, so nothing moves on arrival. -->
		<div class="who">
			<h1>Account</h1>
			<p class="signed-in placeholder" aria-hidden="true"></p>
		</div>
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
					We ask GitHub who you are and which organisations you belong to. Admins can set up a registry;
					members can see its status.
				</p>
				<div class="cta">
					<a class="btn btn-primary" href={loginUrl('/account')} data-sveltekit-reload>Sign in with GitHub</a>
				</div>
				<p class="fine">
					{#if PREVIEW}
						Free during the preview, and provided as is: read the <a href={TERMS_PATH}>preview terms</a>.
					{:else}
						Organisations are {PRICE_PHRASE}, after {TRIAL_MONTHS} months free with no card.
					{/if}
					The sign-in lasts 8 hours and is kept only in an encrypted cookie.
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
					A registry belongs to a GitHub organisation or to your personal account, which is always free. Each
					appears here once it has installed the PrivateCrates reader App, which only reads repository
					metadata and membership.
				</p>
				{#if installUrl}
					<div>
						<a
							class="btn btn-primary"
							href={installUrl}
							target="_blank"
							rel="noopener noreferrer"
							onclick={() => (awaitingInstall = true)}
							>Set up a new organisation<span class="new-tab" aria-hidden="true">↗</span><span
								class="visually-hidden"
							>
								(opens in a new tab)</span
							></a
						>
					</div>
					<p class="fine">
						GitHub opens in a new tab: choose the organisation and approve the install (it needs an
						organisation owner), then come back to this tab.
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
					<OrgList {orgs} selected={selected?.login ?? null} {isLive} {preview} />
					{#if installUrl}
						<div class="add-org">
							<a
								class="btn btn-quiet"
								href={installUrl}
								target="_blank"
								rel="noopener noreferrer"
								onclick={() => (awaitingInstall = true)}
								>Set up a new organisation<span class="new-tab" aria-hidden="true">↗</span><span
									class="visually-hidden"
								>
									(opens in a new tab)</span
								></a
							>
							<p class="fine">
								Installs the reader App on another organisation, or on your personal account, in a new tab.
							</p>
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
								{#if selected.personal}
									Your personal GitHub account: you are its only admin, and it is always free.
								{:else}
									You are {selected.role === 'admin' ? 'an admin' : 'a member'} of this organisation on GitHub.
								{/if}
							</p>

							{#if checkoutSuccess}
								{#if selected.tenant && isActive(selected.tenant.status)}
									<Callout tone="ok" role="status" title="Your subscription has started">
										<p>Your registry is live again. Publishing works from now on.</p>
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

							{#if trialStartedFor === selected.login && planOf(selected) === 'trial'}
								<Callout tone="ok" role="status" title="Your {TRIAL_MONTHS}-month free trial has started">
									<p>
										No card needed until it ends. Add one any time under Manage billing, at the bottom of this
										page.
									</p>
								</Callout>
							{/if}

							{#if selected.tenant && showRegistry}
								<RegistryPanel
									org={selected}
									tenant={selected.tenant}
									{live}
									{preview}
									{terms}
									onterms={() => void loadSession(false)}
									planDetail={finished[selected.login]?.steps.find((s) => s.id === 'plan')?.detail}
									onchange={() => {
										trialStartedFor = selected.login;
										void loadSession(false);
									}}
								/>
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
									org={selected}
									admin={selected.role === 'admin'}
									{refreshing}
									{preview}
									{terms}
									onrefresh={() => loadOnboarding(selected.login)}
									onchange={(doc) => {
										onboarding = doc;
										onboardingChanged(selected.login, doc, true);
									}}
									ontrialstarted={() => (trialStartedFor = selected.login)}
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

		<section class="devices" aria-labelledby="devices-heading">
			<h2 id="devices-heading">Lost a laptop?</h2>
			<p>
				Sign out of PrivateCrates on every device at once: this browser, other browsers, and every machine
				where Cargo is signed in. GitHub cancels every token it gave you for PrivateCrates, so a stolen laptop
				can no longer read your crates. Nobody else is signed out, and you can sign in again straight away.
			</p>
			{#if confirmingEverywhere}
				<div class="confirm" role="group" aria-label="Confirm signing out everywhere">
					<p>Each machine will ask you to sign in again the next time Cargo needs the registry.</p>
					<button
						{@attach (el) => el.focus()}
						class="btn btn-danger"
						type="button"
						onclick={signOutEverywhere}
						disabled={signingOut}
					>
						{signingOut ? 'Signing out…' : 'Sign out of every device'}
					</button>
					<button
						class="btn btn-quiet"
						type="button"
						onclick={() => (confirmingEverywhere = false)}
						disabled={signingOut}
					>
						Cancel
					</button>
				</div>
			{:else}
				<button class="btn btn-quiet" type="button" onclick={() => (confirmingEverywhere = true)}>
					Sign out everywhere…
				</button>
			{/if}
			{#if signOutEverywhereError}
				<ErrorNotice error={signOutEverywhereError} title="You are still signed in on your devices" />
			{/if}
		</section>
	{/if}
</div>

<style>
	.account {
		padding-top: 2.5rem;
		min-height: 60vh;
	}
	/* While loading, keep the footer below the fold so it does not jump down when the account arrives. */
	.account.busy {
		min-height: 100vh;
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
	.devices {
		max-width: 46rem;
		margin-top: 3rem;
		padding-top: 1.5rem;
		border-top: 1px solid var(--rule);
	}
	.devices h2 {
		margin-bottom: 0.5rem;
	}
	.confirm {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 0.75rem;
		padding: 1rem;
		border-radius: 8px;
		background: var(--danger-bg);
	}
	.confirm p {
		flex-basis: 100%;
		margin: 0;
	}
	.btn-danger {
		background: var(--container);
		color: var(--container-ink);
	}
	.btn-danger:hover {
		filter: brightness(1.1);
	}
	.who h1 {
		margin-bottom: 0;
	}
	.placeholder {
		min-height: 2.75rem;
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
		border: 2px solid var(--line);
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
