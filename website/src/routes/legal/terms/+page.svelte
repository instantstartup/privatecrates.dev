<script lang="ts">
	import LegalMeta from '$lib/components/LegalMeta.svelte';
	import Seo from '$lib/components/Seo.svelte';
	import { PAID_TERMS_DATE } from '$lib/legal/docs';
	import PaidTerms from '$lib/legal/PaidTerms.svelte';
	import PreviewTerms from '$lib/legal/PreviewTerms.svelte';
	import { FREE_MEMBER_LIMIT, PREVIEW, PRICE_CAP_USD, TERMS_DATE, TERMS_VERSION } from '$lib/site';
</script>

{#if PREVIEW}
	<Seo
		title="Preview terms"
		description="The terms for the PrivateCrates preview: open to any GitHub organisation, free, provided as is with no warranty or guarantee, and at your own risk, operated by INSTANTSTARTUP LTD."
		path="/legal/terms"
	/>
{:else}
	<Seo
		title="Terms of service"
		description="The terms of service for PrivateCrates, operated by INSTANTSTARTUP LTD: plans and prices, the no-card trial, billing, cancelling, acceptable use and liability."
		path="/legal/terms"
	/>
{/if}

<div class="page">
	<div class="legal">
		{#if PREVIEW}
			<header class="prose intro">
				<h1>Preview terms</h1>
				<LegalMeta version={TERMS_VERSION} date={TERMS_DATE} dateLabel="Published" />
				<p class="lede">
					PrivateCrates is in preview, open to any GitHub organisation or personal account. These terms apply
					to every organisation that uses it until general availability. They are short on purpose; the
					summary below is part of them.
				</p>
			</header>

			<section class="summary panel" aria-labelledby="summary-title">
				<h2 id="summary-title">In short</h2>
				<ul>
					<li>
						<strong>Open to everyone, at our discretion.</strong> Using the preview gives no right to the service:
						we may refuse or end any organisation’s use.
					</li>
					<li><strong>Free.</strong> No payments are taken during the preview.</li>
					<li>
						<strong>As is, at your own risk.</strong> No warranty or guarantee of any kind, no uptime or support
						commitment, and no liability beyond what the law does not let us exclude.
					</li>
					<li>
						<strong>Built on GitHub and others.</strong> When a service we depend on fails, PrivateCrates may too,
						and we are not responsible for it.
					</li>
					<li>
						<strong>Your crates stay yours,</strong> in a repository your GitHub organisation owns, not with us.
					</li>
				</ul>
			</section>
		{:else}
			<header class="prose intro">
				<h1>Terms of service</h1>
				<LegalMeta version={TERMS_VERSION} date={PAID_TERMS_DATE} />
				<p class="lede">
					These terms apply to every organisation and person that uses PrivateCrates. They are short on
					purpose; the summary below is part of them.
				</p>
			</header>

			<section class="summary panel" aria-labelledby="summary-title">
				<h2 id="summary-title">In short</h2>
				<ul>
					<li>
						<strong>Free for small teams.</strong> Personal accounts, and organisations with {FREE_MEMBER_LIMIT}
						members or fewer, pay nothing.
					</li>
					<li>
						<strong>Never more than ${PRICE_CAP_USD} a month.</strong> Larger organisations pay monthly, after a
						free trial that needs no card. We never charge without a card an admin has added.
					</li>
					<li>
						<strong>Cancel any time.</strong> It takes effect at the end of the month already paid for. We do not
						refund part-months, but we do refund billing errors.
					</li>
					<li>
						<strong>Built on GitHub and others.</strong> When a service we depend on fails, PrivateCrates may too.
					</li>
					<li>
						<strong>Your crates stay yours,</strong> in a repository your GitHub organisation owns, not with us.
					</li>
				</ul>
			</section>
		{/if}

		<div class="body">
			{#if PREVIEW}
				<PreviewTerms />
			{:else}
				<PaidTerms />
			{/if}
		</div>
	</div>
</div>

<style lang="postcss">
	@reference '../../../app.css';

	.legal {
		padding-top: var(--space-10);
	}
	h1 {
		font-size: var(--text-headline);
	}
	.intro > * + * {
		margin-top: var(--space-5);
	}
	/* The summary: the whole agreement at a glance, set apart like the trust centre's manifest. */
	.summary {
		margin-top: var(--space-9);
		padding: var(--space-7) var(--space-6);
		border: var(--border-outline) solid var(--line);
	}
	@media (width >= theme(--breakpoint-3)) {
		.summary {
			padding: var(--space-8) var(--space-9);
		}
	}
	.summary h2 {
		font-size: var(--text-title);
		margin-bottom: var(--space-4);
	}
	.summary ul {
		display: grid;
	}
	.summary li {
		padding-block: var(--space-row-y);
		border-top: var(--border-hairline) dashed var(--divider);
	}
	.summary li:first-child {
		border-top: 0;
		padding-top: 0;
	}

	.body {
		margin-top: var(--space-11);
	}
</style>
