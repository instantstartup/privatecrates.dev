<script lang="ts">
	import CodeBlock from '$lib/components/CodeBlock.svelte';
	import FaqItem from '$lib/components/FaqItem.svelte';
	import Seo from '$lib/components/Seo.svelte';
	import Crate from '$lib/illustrations/Crate.svelte';
	import Harbour from '$lib/illustrations/Harbour.svelte';
	import Lighthouse from '$lib/illustrations/Lighthouse.svelte';
	import { FREE_MEMBER_LIMIT, GA_TARGET, PREVIEW, PRICE_USD, TERMS_PATH, TRIAL_MONTHS } from '$lib/site';
	import { cargoConfig, ciBuild, ciPublish } from '$lib/snippets';
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
			{#if PREVIEW}
				<a class="btn btn-primary" href="/account">Ask for an invitation</a>
				<a class="btn btn-quiet" href="/docs/setup">How to set up a registry</a>
			{:else}
				<a class="btn btn-primary" href="/account">Set up your registry</a>
				<a class="btn btn-quiet" href="/docs/setup">How to set up a registry</a>
			{/if}
		</div>
		<p class="terms">
			{#if PREVIEW}
				In private preview, by invitation only. Free, and provided as is with no warranty:
				<a href={TERMS_PATH}>read the terms</a>. From general availability, free for organisations with up to
				{FREE_MEMBER_LIMIT} members, then ${PRICE_USD} per organisation per month. SSO included.
			{:else}
				Free for organisations with up to {FREE_MEMBER_LIMIT} members. Larger ones get {TRIAL_MONTHS} months free
				with no card, then ${PRICE_USD} per organisation per month. SSO included.
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
	<CodeBlock caption=".github/workflows/build.yml" code={ciBuild} />
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
	<CodeBlock caption=".github/workflows/publish.yml" code={ciPublish('acme')} />
</section>

<section class="page pricing-teaser" aria-labelledby="price">
	<div class="panel price-card">
		<div class="price-stack" aria-hidden="true">
			<Crate tone="container" class="c1" />
			<Crate tone="signal" class="c2" label="SSO" />
			<Crate tone="harbour" class="c3" />
		</div>
		<div>
			{#if PREVIEW}
				<h2 id="price">Free during the private preview</h2>
				<p class="planned">Planned pricing from general availability ({GA_TARGET})</p>
			{:else}
				<h2 id="price">Free for small teams, one price for the rest</h2>
			{/if}
			<dl class="tiers">
				<div class="tier">
					<dt class="figure">Free</dt>
					<dd>up to {FREE_MEMBER_LIMIT} members</dd>
				</div>
				<div class="tier">
					<dt class="figure">${PRICE_USD}</dt>
					<dd>per GitHub organisation per month, above {FREE_MEMBER_LIMIT} members</dd>
				</div>
			</dl>
			<ul class="includes">
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
				{#if PREVIEW}
					<a class="btn btn-primary" href="/account">Ask for an invitation</a>
				{:else}
					<a class="btn btn-primary" href="/account">Set up your registry</a>
				{/if}
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
			<FaqItem question="How do we join the private preview?">
				<p>
					<a href="/account">Sign in and ask</a>, naming your GitHub organisation. Once it is invited, an
					admin sets the registry up from the <a href="/account">account page</a>. The private preview is
					free, provided as is with no warranty or guarantee, and at your own risk: the
					<a href={TERMS_PATH}>terms</a> have the details.
				</p>
			</FaqItem>
			<FaqItem question="What if the private preview ends?">
				<p>
					Your crates and index are in your own repository, not with us, so they stay there. We give notice
					where we can, and billing starts only after general availability, with at least 30 days’ notice. The
					<a href={TERMS_PATH}>private preview terms</a> have the details.
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
	.hero {
		display: grid;
		gap: 2.5rem;
		align-items: center;
		padding-block: 3rem 4rem;
	}
	@media (min-width: 60rem) {
		.hero {
			grid-template-columns: 5fr 6fr;
			padding-block: 4.5rem 5rem;
		}
	}
	h1 {
		font-size: var(--text-display);
		text-wrap: balance;
		/* Big Shoulders' long descenders (the g in "signed") need clear space above the next line's ascenders. */
		line-height: 1.15;
		margin-bottom: 1.25rem;
	}
	.actions {
		display: flex;
		flex-wrap: wrap;
		gap: 0.75rem;
		margin-top: 1.75rem;
	}
	.terms {
		margin-top: 1rem;
		color: var(--ink-soft);
		font-size: var(--text-sm);
	}
	.scene {
		min-width: 0;
	}

	h2 {
		font-size: var(--text-3xl);
		margin-bottom: 1.25rem;
	}
	.inline-cmd {
		font-family: var(--font-display);
		font-size: 1em;
		background: none;
		padding: 0;
		text-decoration: underline;
		text-decoration-color: var(--signal);
		text-decoration-thickness: 0.12em;
		text-underline-offset: 0.12em;
	}
	h3 {
		font-size: var(--text-xl);
		margin-bottom: 0.4rem;
	}

	/* How it works: a ruled manifest */
	.steps {
		padding-block: 2rem 4rem;
	}
	.manifest {
		display: grid;
		border: 2px solid var(--line);
		border-radius: 10px;
		background: var(--deck);
		overflow: hidden;
	}
	@media (min-width: 52rem) {
		.manifest {
			grid-template-columns: repeat(3, 1fr);
		}
		.manifest li + li {
			border-left: 2px dashed var(--rule);
			border-top: 0 !important;
		}
	}
	.manifest li {
		display: flex;
		gap: 1rem;
		padding: 1.4rem 1.25rem 1.5rem;
	}
	.manifest li + li {
		border-top: 2px dashed var(--rule);
	}
	.num {
		flex: none;
		display: grid;
		place-items: center;
		width: 2.6rem;
		height: 2.6rem;
		border: 2px solid var(--line);
		border-radius: 4px;
		background: var(--signal);
		color: var(--signal-ink);
		font-family: var(--font-display);
		font-weight: 800;
		font-size: var(--text-xl);
		line-height: 1;
	}
	.manifest p {
		color: var(--ink-soft);
	}
	.config {
		margin-top: 1.5rem;
	}
	.agents {
		margin-top: 1.25rem;
		padding-left: 1rem;
		border-left: 4px solid var(--signal);
		max-width: var(--container-prose);
		color: var(--ink-soft);
	}
	.agents strong {
		color: var(--ink);
	}

	/* Security: the night band with the lighthouse */
	.security {
		padding-block: 4.5rem;
		border-block: 2px solid var(--rule);
	}
	.security-grid {
		display: grid;
		gap: 2rem;
		align-items: start;
	}
	@media (min-width: 60rem) {
		.security-grid {
			grid-template-columns: 1fr 16rem;
		}
	}
	.security .lede {
		margin-bottom: 2rem;
	}
	.claims {
		display: grid;
		gap: 2rem;
	}
	@media (min-width: 48rem) {
		.claims {
			grid-template-columns: repeat(3, 1fr);
		}
	}
	.claims p {
		color: var(--ink-soft);
	}
	.claims p + p {
		margin-top: 0.75rem;
	}
	.security :global(.security-art) {
		width: 100%;
		max-width: 14rem;
		height: auto;
		justify-self: center;
		order: -1;
	}
	@media (min-width: 60rem) {
		.security :global(.security-art) {
			order: 0;
			max-width: 16rem;
		}
	}

	/* Text beside code */
	.split {
		display: grid;
		gap: 1.5rem 3rem;
		align-items: start;
		padding-top: 4.5rem;
	}
	@media (min-width: 60rem) {
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
		margin-top: 0.85rem;
	}
	.split-text {
		max-width: var(--container-prose);
	}

	/* Pricing teaser */
	.pricing-teaser {
		padding-top: 5rem;
	}
	.price-card {
		display: grid;
		gap: 2rem;
		padding: 2rem 1.25rem;
		border: 2px solid var(--line);
	}
	@media (min-width: 48rem) {
		.price-card {
			grid-template-columns: 14rem 1fr;
			align-items: center;
			padding: 2.5rem;
		}
	}
	.price-stack {
		display: grid;
		grid-template-columns: repeat(4, 1fr);
		max-width: 14rem;
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
		color: var(--ink-soft);
	}
	/* The two plans side by side with equal weight; they stack on narrow screens. */
	.tiers {
		display: grid;
		grid-template-columns: repeat(auto-fit, minmax(11rem, 1fr));
		gap: 1rem 2rem;
		margin-top: 1rem;
	}
	.tier {
		display: grid;
		align-content: start;
		gap: 0.25rem;
	}
	.tier dd {
		color: var(--ink-soft);
		max-width: 22ch;
	}
	.figure {
		font-family: var(--font-display);
		font-weight: 800;
		font-size: var(--text-3xl);
		line-height: 1;
		color: var(--ink);
	}
	.includes {
		margin-top: 1rem;
		display: grid;
		gap: 0.35rem;
	}
	.includes li {
		padding-left: 1.6rem;
		position: relative;
	}
	.includes li::before {
		content: '';
		position: absolute;
		left: 0.15rem;
		top: 0.45em;
		width: 0.8rem;
		height: 0.55rem;
		border-left: 3px solid var(--ok);
		border-bottom: 3px solid var(--ok);
		transform: rotate(-45deg);
	}

	.faq {
		padding-top: 5rem;
	}
	.faq-list {
		max-width: var(--container-prose);
	}
</style>
