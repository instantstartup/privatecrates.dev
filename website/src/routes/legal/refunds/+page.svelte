<script lang="ts">
	import Callout from '$lib/components/Callout.svelte';
	import LegalMeta from '$lib/components/LegalMeta.svelte';
	import Seo from '$lib/components/Seo.svelte';
	import { REFUNDS_DOC } from '$lib/legal/docs';
	import { FREE_MEMBER_LIMIT, PREVIEW, READ_GRACE_DAYS, TERMS_PATH } from '$lib/site';
	import { CONTACT_EMAIL } from '$lib/trust';
</script>

<Seo
	title="Refunds and cancellation"
	description="How cancelling a PrivateCrates subscription works, when it takes effect, and when we refund: no refunds for part of a month, and billing errors always put right."
	path="/legal/refunds"
/>

<div class="page">
	<article class="prose legal">
		<h1>Refunds and cancellation</h1>
		<LegalMeta version={REFUNDS_DOC.version} date={REFUNDS_DOC.date} />
		<p class="lede">
			How cancelling works, and when we give money back.{#if !PREVIEW}
				This page is part of the <a href={TERMS_PATH}>terms</a>.{/if}
		</p>
		{#if PREVIEW}
			<Callout title="Nothing to refund during the preview">
				<p>
					PrivateCrates takes no payments during the preview, so there is nothing to cancel or refund yet.
					This page says what will apply once billing starts.
				</p>
			</Callout>
		{/if}

		<h2 id="cancelling">Cancelling</h2>
		<p>
			An admin of your GitHub organisation can cancel at any time, under Manage billing on the
			<a href="/account">account page</a>, which opens Stripe’s billing portal.
		</p>
		<ul>
			<li>
				<strong>It takes effect at the end of the current billing month,</strong> the one already paid for. Until
				then the registry works normally.
			</li>
			<li><strong>Nothing more is charged</strong> after you cancel.</li>
			<li>
				<strong>When the month ends,</strong> publishing stops. Builds keep reading crates for {READ_GRACE_DAYS}
				more days, then reading stops too.
			</li>
			<li>
				<strong>Nothing is deleted:</strong> your index and every crate file are in your own GitHub repository,
				and stay there.
			</li>
		</ul>
		<p>
			Cancelling during the free trial means you are never charged. A trial with no card needs no cancelling:
			it ends by itself, and nothing is charged.
		</p>
		<p>
			Uninstalling the PrivateCrates GitHub Apps stops the registry but does not cancel the subscription:
			cancel it as well.
		</p>
		<p>
			If your organisation shrinks to {FREE_MEMBER_LIMIT} members or fewer, there is no need to cancel: it pays
			nothing from its next invoice, and the subscription stays, at no cost, in case it grows again.
		</p>

		<h2 id="refunds">Refunds</h2>
		<p>
			We charge monthly, in advance, and <strong>do not refund part of a month</strong>. That includes
			cancelling part-way through a month, members leaving, and shrinking to the free plan: each takes effect
			from the next invoice, without proration. In the same way, members who join part-way through a month are
			not charged until the next invoice.
		</p>
		<p>We do refund:</p>
		<ul>
			<li>
				<strong>billing errors,</strong> such as a charge after the subscription was cancelled, a charge taken
				twice, or an invoice for more members than your organisation had: email
				<a href="mailto:{CONTACT_EMAIL}">{CONTACT_EMAIL}</a> with the invoice number, and we refund the difference
				to the card it was charged to;
			</li>
			<li>
				<strong>the time after the service ends,</strong> if we stop offering PrivateCrates, or end your organisation’s
				use for any reason other than a breach of the terms.
			</li>
		</ul>

		<h2 id="contact">Questions</h2>
		<p>
			Billing questions and refund requests: <a href="mailto:{CONTACT_EMAIL}">{CONTACT_EMAIL}</a>, from or
			naming your GitHub organisation. Admins can see and download every invoice under Manage billing on the
			account page.
		</p>
	</article>
</div>

<style>
	.legal {
		padding-top: var(--space-10);
	}
	h1 {
		font-size: var(--text-headline);
	}
</style>
