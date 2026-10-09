<script lang="ts">
	import CodeBlock from '$lib/components/CodeBlock.svelte';
	import FaqItem from '$lib/components/FaqItem.svelte';
	import Seo from '$lib/components/Seo.svelte';
	import Crate from '$lib/illustrations/Crate.svelte';
	import Harbour from '$lib/illustrations/Harbour.svelte';
	import Lighthouse from '$lib/illustrations/Lighthouse.svelte';
	import {
		FREE_MEMBER_LIMIT,
		GA_TARGET,
		MEMBER_PRICE_USD,
		PREVIEW,
		PRICE_CAP_USD,
		PRICE_PHRASE,
		TERMS_PATH,
		TRIAL_MONTHS
	} from '$lib/site';
	import { cargoConfig, pushTag } from '$lib/snippets';
</script>

<Seo
	title="PrivateCrates: private Rust crates, signed for by GitHub"
	description="A private Cargo registry for your GitHub organisation. GitHub permissions, SSO and offboarding apply as they are, CI needs no secrets, and every version carries provenance you can verify."
	path="/"
/>

<section class="hero page">
	<div class="pitch">
		<h1>Private Rust crates, signed for by GitHub</h1>
		<p class="lede">
			A Cargo registry for your GitHub organisation. Whoever can read a repository can use its crates; whoever
			can push to it can publish them. No new accounts, no keys to hand out, no access lists to keep in sync.
		</p>
		<div class="actions">
			<a class="btn btn-primary" href="/account">Set up your registry</a>
			<a class="btn btn-quiet" href="/docs/setup">How to set up a registry</a>
		</div>
		<p class="terms">
			{#if PREVIEW}
				In preview: free, and provided as is with no warranty:
				<a href={TERMS_PATH}>read the terms</a>. From general availability, {PRICE_PHRASE}. SSO included.
			{:else}
				Free for organisations with up to {FREE_MEMBER_LIMIT} members. Larger ones get {TRIAL_MONTHS} months free
				with no card, then ${MEMBER_PRICE_USD} a member a month past {FREE_MEMBER_LIMIT}, never more than ${PRICE_CAP_USD}.
				SSO included.
			{/if}
		</p>
	</div>
	<div class="scene">
		<Harbour />
	</div>
</section>

<section class="page steps" aria-labelledby="how">
	<h2 id="how">From repository to <code class="inline-cmd">cargo build</code> in three steps</h2>
	<ol class="manifest">
		<li>
			<span class="num" aria-hidden="true">1</span>
			<div>
				<h3>Install two GitHub Apps</h3>
				<p>
					The reader App sees repository metadata, nothing more. The storage App writes to one private
					repository you create, which holds the index and every crate file.
				</p>
			</div>
		</li>
		<li>
			<span class="num" aria-hidden="true">2</span>
			<div>
				<h3>Point Cargo at your registry</h3>
				<p>
					Three lines of config and our credential provider. The first build signs each developer in with
					GitHub; their existing access decides what they can use.
				</p>
			</div>
		</li>
		<li>
			<span class="num" aria-hidden="true">3</span>
			<div>
				<h3>Publish from CI with a tag</h3>
				<p>
					Plain <code>cargo publish</code> in GitHub Actions. Every version becomes an immutable GitHub release
					with signed proof of the workflow that built it.
				</p>
			</div>
		</li>
	</ol>
	<div class="config">
		<CodeBlock caption=".cargo/config.toml" code={cargoConfig('acme', 'https://acme.privatecrates.dev')} />
	</div>
	<p class="agents">
		<strong>Works with AI coding agents.</strong> Ask Claude Code or another agent to set up the registry and
		configure your crates: it uses the <code>cargo privatecrates</code> CLI and your own <code>gh</code>
		login, and hands you a link where GitHub needs a person.
		<a href="/docs/agents">Set up with an AI agent</a>
	</p>
</section>

<section class="night security" aria-labelledby="security">
	<div class="page security-grid">
		<div class="security-text">
			<h2 id="security">Security that was already yours</h2>
			<p class="lede">
				A registry is a supply-chain component. We built this one so that the answer to “what if PrivateCrates
				is compromised?” is short.
			</p>
			<div class="claims">
				<div>
					<h3>GitHub is your identity provider</h3>
					<p>
						Every permission decision is GitHub’s. SAML SSO, SCIM provisioning and offboarding apply with
						nothing to configure: remove someone from a team and their access to its crates ends within
						seconds. SSO is part of the one price, with no SSO tax.
					</p>
				</div>
				<div>
					<h3>We hold nothing that matters</h3>
					<p>
						Your index and crate files live in a repository your organisation owns. Developers hold tokens
						that can only read repository metadata; CI holds one-hour read tokens. The only thing we store is
						a record of who accepted our terms, and every write we make is a verified commit you can audit.
					</p>
				</div>
				<div>
					<h3>Don’t trust us, verify us</h3>
					<p>
						Every version is an immutable GitHub release carrying a GitHub-signed record of the workflow that
						built it. The open-source <code>privatecrates-verify</code> runs in your own CI and reports anything
						we should not have done, even if we are fully compromised.
					</p>
					<p><a href="/docs/security">Read the security model</a></p>
				</div>
			</div>
		</div>
		<Lighthouse class="security-art" />
	</div>
</section>

<section class="page split" aria-labelledby="ci">
	<div class="split-text">
		<h2 id="ci">CI with nothing to leak</h2>
		<p>
			GitHub Actions already issues each job a short-lived OIDC token. The credential provider trades it for a
			one-hour, read-only registry token. There is no secret to store, rotate or accidentally print in a log.
		</p>
		<p>
			Workflows triggered from forks get no OIDC token, so they cannot read your private crates. That is the
			point.
		</p>
		<p><a href="/docs/ci">Set up CI</a></p>
	</div>
	<ol class="chain" aria-label="What a CI job holds">
		<li>
			<span class="num" aria-hidden="true">1</span>
			<div>
				<h3>GitHub issues the job a token</h3>
				<p>It names the repository and the workflow, and expires within minutes.</p>
			</div>
		</li>
		<li>
			<span class="num" aria-hidden="true">2</span>
			<div>
				<h3>Traded for a registry token</h3>
				<p>Read-only, for your registry alone, for one hour.</p>
			</div>
		</li>
		<li>
			<span class="num" aria-hidden="true">3</span>
			<div>
				<h3>Nothing left to leak</h3>
				<p>No secret in the repository or its settings, nothing to rotate, nothing to print in a log.</p>
			</div>
		</li>
	</ol>
</section>

<section class="page split reverse" aria-labelledby="publish">
	<div class="split-text">
		<h2 id="publish">Plain <code class="inline-cmd">cargo publish</code>, with a paper trail</h2>
		<p>
			Push a tag and CI publishes. The token it uses names one crate, one version and the SHA-256 of the exact
			bytes Cargo packaged, so it cannot be replayed for anything else.
		</p>
		<p>
			Publishing from a laptop is refused by default, with the workflow ready to paste. Each crate’s owners
			file, in your repository, says which workflow may publish it and whether a GitHub environment’s
			reviewers must approve first.
		</p>
		<p><a href="/docs/publishing">Publishing guide</a></p>
	</div>
	<div class="release">
		<CodeBlock caption="Releasing 0.2.0" code={pushTag} />
		<p>
			The workflow that <code>cargo privatecrates init</code> adds does the rest: it runs
			<code>cargo publish</code> and records GitHub’s signed statement of which workflow published which bytes.
		</p>
	</div>
</section>

<section class="page pricing-teaser" aria-labelledby="price">
	<div class="panel price-card">
		<div class="price-stack" aria-hidden="true">
			<Crate tone="accent" class="c1" />
			<Crate tone="action" class="c2" label="SSO" />
			<Crate tone="muted" class="c3" />
		</div>
		<div>
			{#if PREVIEW}
				<h2 id="price">Free during the preview</h2>
				<p class="planned">Planned pricing from general availability ({GA_TARGET})</p>
			{:else}
				<h2 id="price">Free for small teams, never more than ${PRICE_CAP_USD} a month</h2>
			{/if}
			<dl class="tiers">
				<div class="tier">
					<dt class="figure">Free</dt>
					<dd>up to {FREE_MEMBER_LIMIT} members</dd>
				</div>
				<div class="tier">
					<dt class="figure">${MEMBER_PRICE_USD}</dt>
					<dd>a month for each member past {FREE_MEMBER_LIMIT}</dd>
				</div>
				<div class="tier">
					<dt class="figure">${PRICE_CAP_USD}</dt>
					<dd>a month at most, however many members</dd>
				</div>
			</dl>
			<ul class="includes">
				<li>Always free for personal GitHub accounts</li>
				<li>Unlimited CI jobs, and unlimited users on the paid plan</li>
				<li>
					{TRIAL_MONTHS} months free for larger organisations, no card to start{PREVIEW
						? ', once billing starts'
						: ''}
				</li>
				<li>SAML SSO and SCIM through GitHub, included</li>
				<li>Provenance on every version, and the verifier</li>
				<li>Search across your crates and crates.io</li>
			</ul>
			<div class="actions">
				<a class="btn btn-primary" href="/account">Set up your registry</a>
				<a class="btn btn-quiet" href="/pricing">Pricing details</a>
			</div>
		</div>
	</div>
</section>

<section class="page faq" aria-labelledby="faq">
	<h2 id="faq">Questions</h2>
	<div class="faq-list">
		<FaqItem question="Do developers need a new account?">
			<p>
				No. They sign in with GitHub the first time Cargo needs a token, and what they can read or publish
				follows their access to each crate’s repository.
			</p>
		</FaqItem>
		<FaqItem question="Where are our crates stored?">
			<p>
				In a private repository in your organisation: the index as files, and each version as an immutable
				GitHub release. We keep caches only. If you leave, everything is already in your repository.
			</p>
		</FaqItem>
		<FaqItem question="Does it replace crates.io?">
			<p>
				No. Public dependencies stay on crates.io. Search shows your private crates first, then crates.io
				results, and publishing a name that also exists on crates.io is refused by default to prevent
				dependency confusion.
			</p>
		</FaqItem>
		<FaqItem question="Can outside collaborators use our crates?">
			<p>
				Yes, the crates of the repositories they can read, and nothing else. A crate someone cannot read is a
				404 to them, so its name is not revealed either.
			</p>
		</FaqItem>
		<FaqItem question="What about CI outside GitHub Actions, or Docker builds?">
			<p>
				At launch, only GitHub Actions can read the registry, so no long-lived credential exists anywhere.
				Build images in Actions using a BuildKit secret mount, or run <code>cargo vendor</code> in Actions and
				hand the vendored tree to the other system. See
				<a href="/docs/ci#elsewhere">Docker builds and CI outside GitHub Actions</a>.
			</p>
		</FaqItem>
		<FaqItem question="Can we read the code?">
			<p>
				The credential provider and the verifier, the parts that run on your machines and in your CI, are open
				source under MIT or Apache-2.0. The hosted service is source-available under the Business Source
				License 1.1, so your security team can read the code that handles your tokens.
			</p>
		</FaqItem>
		{#if PREVIEW}
			<FaqItem question="Can anyone use the preview?">
				<p>
					Yes. An admin of a GitHub organisation, or anyone for their personal account, sets a registry up
					from the <a href="/account">account page</a>. The preview is free, provided as is with no warranty
					or guarantee, and at your own risk: the <a href={TERMS_PATH}>terms</a> have the details.
				</p>
			</FaqItem>
			<FaqItem question="What if the preview ends?">
				<p>
					Your crates and index are in your own repository, not with us, so they stay there. We give notice
					where we can, and billing starts only after general availability, with at least 30 days’ notice. The
					<a href={TERMS_PATH}>preview terms</a> have the details.
				</p>
			</FaqItem>
		{:else}
			<FaqItem question="What happens if we stop paying?">
				<p>
					Publishing stops when the subscription or free trial ends. Builds keep reading crates for 14 more
					days, so nothing breaks overnight. Your crates and index stay in your repository either way.
				</p>
			</FaqItem>
		{/if}
	</div>
</section>

<style>
	@reference '../app.css';

	.hero {
		display: grid;
		gap: var(--space-10);
		align-items: center;
		padding-block: var(--space-11) var(--space-12);
	}
	@media (width >= theme(--breakpoint-8)) {
		.hero {
			grid-template-columns: 5fr 6fr;
			padding-block: var(--space-section-inset) var(--space-section);
		}
	}
	h1 {
		font-size: var(--text-display);
		text-wrap: balance;
		/* Big Shoulders' long descenders (the g in "signed") need clear space above the next line's ascenders. */
		line-height: 1.15;
		margin-bottom: var(--space-6);
	}
	.actions {
		display: flex;
		flex-wrap: wrap;
		gap: var(--space-4);
		margin-top: var(--space-8);
	}
	.terms {
		margin-top: var(--space-5);
		color: var(--ink-muted);
		font-size: var(--text-meta);
	}
	.scene {
		min-width: 0;
	}

	h2 {
		font-size: var(--text-headline);
		margin-bottom: var(--space-6);
	}
	.inline-cmd {
		font-family: var(--font-display);
		font-size: inherit;
		background: none;
		padding: 0;
		text-decoration: underline;
		text-decoration-color: var(--action);
		text-decoration-thickness: var(--underline-display);
		text-underline-offset: var(--underline-display);
	}
	h3 {
		font-size: var(--text-title);
		margin-bottom: var(--space-after-title);
	}

	/* How it works: a ruled manifest */
	.steps {
		padding-block: var(--space-9) var(--space-12);
	}
	.manifest {
		display: grid;
		border: var(--border-outline) solid var(--line);
		border-radius: var(--radius-panel);
		background: var(--surface-raised);
		overflow: hidden;
	}
	@media (width >= theme(--breakpoint-6)) {
		.manifest {
			grid-template-columns: repeat(3, 1fr);
		}
		.manifest li + li {
			border-left: var(--border-outline) dashed var(--divider);
			border-top: 0 !important;
		}
	}
	.manifest li {
		display: flex;
		gap: var(--space-5);
		padding: var(--space-panel-step-top) var(--space-6) var(--space-7);
	}
	.manifest li + li {
		border-top: var(--border-outline) dashed var(--divider);
	}
	.num {
		flex: none;
		display: grid;
		place-items: center;
		width: var(--size-marker-large);
		height: var(--size-marker-large);
		border: var(--border-outline) solid var(--line);
		border-radius: var(--radius-inline);
		background: var(--action);
		color: var(--action-ink);
		font-family: var(--font-display);
		font-weight: 800;
		font-size: var(--text-title);
		line-height: 1;
	}
	.manifest p {
		color: var(--ink-muted);
	}
	.config {
		margin-top: var(--space-7);
	}
	.agents {
		margin-top: var(--space-6);
		padding-left: var(--space-5);
		border-left: var(--border-rule) solid var(--action);
		max-width: var(--container-prose);
		color: var(--ink-muted);
	}
	.agents strong {
		color: var(--ink);
	}

	/* Security: the night band with the lighthouse */
	.security {
		padding-block: var(--space-section-inset);
		border-block: var(--border-outline) solid var(--divider);
	}
	.security-grid {
		display: grid;
		gap: var(--space-9);
		align-items: start;
	}
	@media (width >= theme(--breakpoint-8)) {
		.security-grid {
			grid-template-columns: 1fr var(--container-aside);
		}
	}
	.security .lede {
		margin-bottom: var(--space-9);
	}
	.claims {
		display: grid;
		gap: var(--space-9);
	}
	@media (width >= theme(--breakpoint-5)) {
		.claims {
			grid-template-columns: repeat(3, 1fr);
		}
	}
	.claims p {
		color: var(--ink-muted);
	}
	.claims p + p {
		margin-top: var(--space-4);
	}
	.security :global(.security-art) {
		width: 100%;
		max-width: var(--size-illustration-small);
		height: auto;
		justify-self: center;
		order: -1;
	}
	@media (width >= theme(--breakpoint-8)) {
		.security :global(.security-art) {
			order: 0;
			max-width: var(--container-aside);
		}
	}

	/* Text beside code */
	.split {
		display: grid;
		gap: var(--space-7) var(--space-11);
		align-items: start;
		padding-top: var(--space-section-inset);
	}
	@media (width >= theme(--breakpoint-8)) {
		.split {
			grid-template-columns: 5fr 6fr;
		}
		.split.reverse {
			grid-template-columns: 6fr 5fr;
		}
		.split.reverse .split-text {
			order: 2;
		}
	}
	.split-text p + p {
		margin-top: var(--space-paragraph);
	}
	.split-text {
		max-width: var(--container-prose);
	}

	/* The CI token chain: the manifest's ruled panel, in one column */
	.chain {
		display: grid;
		border: var(--border-outline) solid var(--line);
		border-radius: var(--radius-panel);
		background: var(--surface-raised);
		overflow: hidden;
	}
	.chain li {
		display: flex;
		gap: var(--space-5);
		padding: var(--space-compact-step-top) var(--space-6) var(--space-compact-step-bottom);
	}
	.chain li + li {
		border-top: var(--border-outline) dashed var(--divider);
	}
	.chain h3 {
		font-size: var(--text-lead);
		margin-bottom: var(--space-after-title-compact);
	}
	.chain p {
		color: var(--ink-muted);
	}
	.release p {
		margin-top: var(--space-paragraph);
		color: var(--ink-muted);
		max-width: var(--container-prose);
	}

	/* Pricing teaser */
	.pricing-teaser {
		padding-top: var(--space-section);
	}
	.price-card {
		display: grid;
		gap: var(--space-9);
		padding: var(--space-9) var(--space-6);
		border: var(--border-outline) solid var(--line);
	}
	@media (width >= theme(--breakpoint-5)) {
		.price-card {
			grid-template-columns: var(--size-illustration-small) 1fr;
			align-items: center;
			padding: var(--space-10);
		}
	}
	.price-stack {
		display: grid;
		grid-template-columns: repeat(4, 1fr);
		max-width: var(--size-illustration-small);
	}
	.price-stack :global(svg) {
		width: 100%;
		height: auto;
	}
	.price-stack :global(.c1) {
		grid-column: 1 / 3;
		grid-row: 2;
	}
	.price-stack :global(.c2) {
		grid-column: 3 / 5;
		grid-row: 2;
	}
	.price-stack :global(.c3) {
		grid-column: 2 / 4;
		grid-row: 1;
	}
	.planned {
		color: var(--ink-muted);
	}
	/* The two plans side by side with equal weight; they stack on narrow screens. */
	.tiers {
		display: grid;
		grid-template-columns: repeat(auto-fit, minmax(var(--size-column-tier), 1fr));
		gap: var(--space-5) var(--space-9);
		margin-top: var(--space-5);
	}
	.tier {
		display: grid;
		align-content: start;
		gap: var(--space-2);
	}
	.tier dd {
		color: var(--ink-muted);
		max-width: var(--container-tier-note);
	}
	.figure {
		font-family: var(--font-display);
		font-weight: 800;
		font-size: var(--text-headline);
		line-height: 1;
		color: var(--ink);
	}
	.includes {
		margin-top: var(--space-5);
		display: grid;
		gap: var(--space-list-item);
	}
	.includes li {
		padding-left: var(--space-list-indent-tick);
		position: relative;
	}
	.includes li::before {
		content: '';
		position: absolute;
		left: var(--space-tick-x);
		top: var(--space-tick-y);
		width: var(--size-tick-long);
		height: var(--size-tick-short);
		border-left: var(--border-strong) solid var(--ok);
		border-bottom: var(--border-strong) solid var(--ok);
		transform: rotate(-45deg);
	}

	.faq {
		padding-top: var(--space-section);
	}
	.faq-list {
		max-width: var(--container-prose);
	}
</style>
